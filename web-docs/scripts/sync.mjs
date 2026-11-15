// Sync: data/ (папки, zip и .xdp CoderWiki) → docs/<lang>/<owner>/<repo>/*.md + манифест для VitePress
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import AdmZip from 'adm-zip';
import dotenv from 'dotenv';
import { LANGS, langByName } from './lib/langs.mjs';
import { makeSlug, uniqueSlug } from './lib/slug.mjs';
import { firstH1, description, headings, rewriteLinks, frontmatter, stripFrontmatter } from './lib/markdown.mjs';
import { expandXdp } from './lib/xdp.mjs';
import { features, linkChapters } from './lib/match.mjs';
import { autoTags } from './lib/tags.mjs';

const LANGS_BY_CODE = Object.fromEntries(LANGS.map((l) => [l.code, l.name]));

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
dotenv.config({ path: path.join(ROOT, '.env') });

const SOURCE = path.resolve(ROOT, process.env.DOCS_SOURCE_DIR || './data');
const SITE_URL = (process.env.SITE_URL || 'http://localhost:4173').replace(/\/$/, '');
const BASE_PATH = normalizeBase(process.env.BASE_PATH || '/');
const DOCS = path.join(ROOT, 'docs');
const CACHE = path.join(ROOT, '.cache', 'unzipped');
const MANIFEST = path.join(DOCS, '.vitepress', 'generated-manifest.json');
const KEEP = new Set(['.vitepress', 'public', 'index.md']);
const NAME_RE = /^(.+?)__(.+?)@([a-z]+)-docs$/i;

function normalizeBase(b) {
  let s = b.trim();
  if (!s.startsWith('/')) s = '/' + s;
  if (!s.endsWith('/')) s += '/';
  return s;
}

const report = { repos: 0, langs: new Set(), chapters: 0, broken: [], skipped: [], zips: 0, xdp: 0, matching: [] };

// --- 1. Источники: папки, zip и .xdp; при дубле побеждает более свежий ---
function discoverSources() {
  if (!fs.existsSync(SOURCE)) {
    console.error(`[sync] Папка с данными не найдена: ${SOURCE}`);
    return [];
  }
  const byStem = new Map();
  const offer = (cand) => {
    const prev = byStem.get(cand.stem);
    if (!prev || cand.mtime > prev.mtime) byStem.set(cand.stem, cand);
  };
  for (const entry of fs.readdirSync(SOURCE, { withFileTypes: true })) {
    if (entry.isFile() && /\.xdp$/i.test(entry.name)) {
      // Один .xdp содержит все языковые версии репозитория
      try {
        const cands = expandXdp(path.join(SOURCE, entry.name), path.join(CACHE, 'xdp'), ROOT);
        report.xdp++;
        cands.forEach(offer);
      } catch (e) {
        report.skipped.push(`${entry.name} (ошибка .xdp: ${e.message})`);
      }
      continue;
    }
    const isZip = entry.isFile() && /\.zip$/i.test(entry.name);
    const stem = isZip ? entry.name.replace(/\.zip$/i, '') : entry.name;
    const m = stem.match(NAME_RE);
    if (!m || (!entry.isDirectory() && !isZip)) {
      if (!entry.name.startsWith('.')) report.skipped.push(entry.name);
      continue;
    }
    const full = path.join(SOURCE, entry.name);
    const mtime = fs.statSync(full).mtimeMs;
    offer({ stem, owner: m[1], repo: m[2], langName: m[3].toLowerCase(), full, isZip, mtime });
  }
  return [...byStem.values()];
}

// Распаковка zip; корень архива — либо .md файлы, либо одна папка с ними
function materialize(src) {
  if (!src.isZip) return src.full;
  const out = path.join(CACHE, src.stem);
  fs.rmSync(out, { recursive: true, force: true });
  new AdmZip(src.full).extractAllTo(out, true);
  report.zips++;
  const entries = fs.readdirSync(out, { withFileTypes: true }).filter((e) => !e.name.startsWith('.') && e.name !== '__MACOSX');
  if (entries.length === 1 && entries[0].isDirectory()) return path.join(out, entries[0].name);
  return out;
}

// --- 2. Очистка рабочей папки документов ---
function cleanDocs() {
  fs.mkdirSync(DOCS, { recursive: true });
  for (const e of fs.readdirSync(DOCS)) {
    if (!KEEP.has(e)) fs.rmSync(path.join(DOCS, e), { recursive: true, force: true });
  }
}

// --- 3. Обработка одной языковой версии репозитория ---
function processRepoLang(src, dir) {
  const lang = langByName(src.langName);
  const files = fs.readdirSync(dir).filter((f) => !f.startsWith('.'));
  const mdFiles = files.filter((f) => /\.md$/i.test(f));
  if (!mdFiles.length) {
    report.skipped.push(`${src.stem} (нет .md файлов)`);
    return null;
  }
  const basePath = `/${lang.code}/${src.owner}/${src.repo}/`;
  const outDir = path.join(DOCS, lang.code, src.owner, src.repo);
  fs.mkdirSync(outDir, { recursive: true });
  // Метаданные из .xdp: исходные файлы каждой главы и всего репозитория
  const metaFile = path.join(dir, '.cw-meta.json');
  const meta = fs.existsSync(metaFile) ? JSON.parse(fs.readFileSync(metaFile, 'utf8')) : { sections: {}, repoFiles: [] };

  // План страниц: имя файла → слаг/номер/заголовок
  const used = new Set(['index']);
  const pages = [];
  for (const file of mdFiles) {
    const base = file.replace(/\.md$/i, '');
    const raw = stripFrontmatter(fs.readFileSync(path.join(dir, file), 'utf8'));
    const isOverview = /^_?overview$/i.test(base);
    const numMatch = base.match(/^(\d+)[-_ ]/);
    const num = isOverview ? 0 : numMatch ? parseInt(numMatch[1], 10) : null;
    const h1 = firstH1(raw);
    const title = h1 || (isOverview ? `${src.owner}/${src.repo}` : base.replace(/^\d+[-_ ]/, '').replace(/[-_]/g, ' '));
    const slug = isOverview ? 'index' : uniqueSlug(makeSlug(base, num, h1), used);
    pages.push({ file, base, raw, isOverview, num, title, slug });
  }
  if (!pages.some((p) => p.isOverview)) {
    // Без _OVERVIEW главной становится первая по номеру глава
    const first = [...pages].sort(sortPages)[0];
    first.isOverview = true;
    first.slug = 'index';
    first.num = first.num ?? 0;
  }
  pages.sort(sortPages);

  const byOld = new Map();
  for (const p of pages) {
    byOld.set(p.base.toLowerCase(), p);
    byOld.set(p.file.toLowerCase(), p);
  }
  const resolve = (old) => {
    const p = byOld.get(old.toLowerCase()) || byOld.get(old.toLowerCase().replace(/\.md$/i, ''));
    return p ? (p.slug === 'index' ? basePath : basePath + p.slug) : null;
  };

  const overview = pages.find((p) => p.isOverview);
  const repoDescription = description(overview.raw);
  const chapters = [];
  for (const p of pages) {
    let body = rewriteLinks(p.raw, resolve, (t) => report.broken.push(`${src.stem}/${p.file} → ${t}`));
    if (p.isOverview && !firstH1(p.raw)) body = `# ${src.repo}\n\n${body}`;
    const pagePath = p.slug === 'index' ? basePath : basePath + p.slug;
    const fm = frontmatter({
      title: p.isOverview ? `${src.repo} — ${p.title === `${src.owner}/${src.repo}` ? 'Docs' : p.title}` : p.title,
      description: description(p.raw) || repoDescription,
      keywords: [src.owner, src.repo, ...headings(p.raw).slice(0, 12)].join(', '),
      lang: lang.htmlLang,
      cwRepo: `${src.owner}/${src.repo}`,
      cwLang: lang.code,
      cwNum: p.num,
      cwPath: pagePath,
    });
    fs.writeFileSync(path.join(outDir, `${p.slug}.md`), fm + body, 'utf8');
    if (!p.isOverview) chapters.push({ num: p.num, slug: p.slug, title: p.title, path: pagePath, _f: features(p.raw, meta.sections[p.base]?.files || []) });
  }
  // Картинки и прочие ресурсы — рядом с главами
  for (const f of files.filter((f) => !/\.md$/i.test(f))) {
    const s = path.join(dir, f);
    if (fs.statSync(s).isFile()) fs.copyFileSync(s, path.join(outDir, f));
  }
  report.chapters += chapters.length;
  report.langs.add(lang.code);
  return {
    code: lang.code,
    name: lang.name,
    label: lang.label,
    short: lang.short,
    dir: lang.dir,
    path: basePath,
    title: overview.title,
    description: repoDescription,
    chapters,
    chapterCount: chapters.length,
    updatedAt: src.mtime,
    _text: pages.map((p) => p.raw).join('\n'),
    _files: [...meta.repoFiles, ...chapters.flatMap((c) => [...c._f.files])],
  };
}

// Порядок: обзор, затем по номеру, затем без номера по алфавиту
function sortPages(a, b) {
  if (a.isOverview !== b.isOverview) return a.isOverview ? -1 : 1;
  if (a.num != null && b.num != null) return a.num - b.num;
  if (a.num != null) return -1;
  if (b.num != null) return 1;
  return a.base.localeCompare(b.base);
}

// --- 4. Сборка ---
function run() {
  const started = Date.now();
  cleanDocs();
  fs.rmSync(CACHE, { recursive: true, force: true });
  const sources = discoverSources();
  const repos = new Map();
  for (const src of sources) {
    const dir = materialize(src);
    const result = processRepoLang(src, dir);
    if (!result) continue;
    const key = `${src.owner}/${src.repo}`;
    if (!repos.has(key)) repos.set(key, { key, owner: src.owner, repo: src.repo, langs: {}, updatedAt: 0 });
    const r = repos.get(key);
    r.langs[result.code] = result;
    r.updatedAt = Math.max(r.updatedAt, src.mtime);
  }
  // Сопоставление глав между языками и автотеги; служебные поля не попадают в манифест
  for (const r of repos.values()) {
    for (const st of linkChapters(r.langs)) report.matching.push({ repo: r.key, ...st });
    const langs = Object.values(r.langs);
    r.tags = autoTags(langs.map((l) => l._text).join('\n'), new Set(langs.flatMap((l) => l._files)));
    for (const l of langs) {
      delete l._text;
      delete l._files;
      for (const c of l.chapters) delete c._f;
    }
  }
  const repoList = [...repos.values()].sort((a, b) => b.updatedAt - a.updatedAt || a.key.localeCompare(b.key));
  report.repos = repoList.length;

  const manifest = {
    generatedAt: new Date().toISOString(),
    siteUrl: SITE_URL,
    basePath: BASE_PATH,
    languages: [...report.langs].sort(),
    repos: repoList.map((r) => ({ ...r, updatedAt: new Date(r.updatedAt).toISOString() })),
  };
  fs.mkdirSync(path.dirname(MANIFEST), { recursive: true });
  fs.writeFileSync(MANIFEST, JSON.stringify(manifest, null, 2));

  // Главная для каждой локали (/ru/, /en/ …) — та же витрина с интерфейсом на этом языке
  for (const code of new Set(['en', 'ru', ...report.langs])) {
    const l = langByName(LANGS_BY_CODE[code] || code);
    fs.mkdirSync(path.join(DOCS, code), { recursive: true });
    fs.writeFileSync(
      path.join(DOCS, code, 'index.md'),
      frontmatter({ layout: 'page', title: 'CoderWiki Docs', sidebar: false, lang: l.htmlLang, cwLang: code, cwPath: `/${code}/` }) + '<RepoShowcase />\n'
    );
  }

  fs.mkdirSync(path.join(DOCS, 'public'), { recursive: true });
  fs.writeFileSync(path.join(DOCS, 'public', 'robots.txt'), `User-agent: *\nAllow: /\n\nSitemap: ${SITE_URL}${BASE_PATH}sitemap.xml\n`);

  console.log(`[sync] Источник: ${SOURCE}`);
  console.log(`[sync] Репозиториев: ${report.repos}, языков: ${report.langs.size} (${[...report.langs].join(', ') || '—'}), глав: ${report.chapters}, zip распаковано: ${report.zips}, .xdp: ${report.xdp}`);
  for (const m of report.matching) {
    console.log(`[sync] Главы ${m.repo} ${m.pair}: по смыслу ${m.meaning}, по номеру ${m.number}, ближайшая ${m.near}, без пары ${m.none}, с другим номером ${m.moved}`);
  }
  for (const r of repoList) if (r.tags.length) console.log(`[sync] Теги ${r.key}: ${r.tags.join(', ')}`);
  if (report.skipped.length) console.log(`[sync] Пропущено: ${report.skipped.join(', ')}`);
  if (report.broken.length) {
    console.log(`[sync] Битых внутренних ссылок: ${report.broken.length}`);
    for (const b of report.broken) console.log(`   - ${b}`);
  }
  console.log(`[sync] Манифест: ${path.relative(ROOT, MANIFEST)} · ${Date.now() - started} мс`);
  if (!report.repos) console.warn('[sync] Внимание: документация не найдена — сайт будет пустым');
}

run();
