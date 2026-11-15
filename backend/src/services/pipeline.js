import crypto from 'crypto';
import fs from 'fs/promises';
import path from 'path';
import ignoreLib from 'ignore';
import { getCache, setCache, delCache, delPrefix } from './db.js';
import { cloneRepo, parseRepoUrl } from './git.js';

// Ключ документации: owner/name@language; исходники и RAG-индекс — по базовому ключу
export function baseRepo(k) {
  return String(k).split('@')[0];
}
import { planSections, generateSectionDoc, generateOverview, generateIdeas, assignNewFiles, generateInstallDoc } from './lmstudio.js';
import { resolveEmbeddingModel, embedFile, embedTexts } from './embeddings.js';
import { getMaxTokens, sliceByTokens } from '../utils/tokenManager.js';

const jobs = new Map();

const IGNORE_DIRS = new Set([
  'node_modules', '.git', 'dist', 'build', 'out', '.next', '.nuxt', 'coverage',
  'vendor', '__pycache__', '.venv', 'venv', 'target', '.idea', '.vscode',
  '.cache', 'tmp', '.github',
]);

const IGNORE_FILES = new Set([
  'yarn.lock', 'package-lock.json', 'pnpm-lock.yaml', '.ds_store', 'cargo.lock',
  'poetry.lock', 'composer.lock', 'gemfile.lock',
]);

const ALLOWED_EXT = new Set([
  '.js', '.jsx', '.ts', '.tsx', '.vue', '.svelte', '.mjs', '.cjs',
  '.py', '.go', '.rs', '.java', '.kt', '.c', '.cpp', '.h', '.hpp', '.cs',
  '.rb', '.php', '.swift', '.scala', '.sh', '.bash', '.sql',
  '.html', '.css', '.scss', '.less', '.json', '.yml', '.yaml', '.toml',
  '.md', '.txt', '.xml', '.ini', '.cfg', '.dockerfile',
]);

export function getJob(id) {
  return jobs.get(id);
}

export function snapshot(job) {
  return {
    jobId: job.id,
    status: job.status,
    currentFile: job.currentFile,
    processed: job.processed,
    total: job.total,
    repoKey: job.repoKey,
    error: job.error,
    cachedHits: job.cachedHits,
    partial: job.partial,
    sections: job.sectionsMeta || [],
  };
}

export function attachClient(job, res) {
  job.clients.add(res);
}

function emit(job, patch = {}) {
  Object.assign(job, patch);
  const data = `data: ${JSON.stringify(snapshot(job))}\n\n`;
  for (const res of job.clients) {
    res.write(data);
    if (['done', 'error', 'cancelled'].includes(job.status)) res.end();
  }
  if (['done', 'error', 'cancelled'].includes(job.status)) job.clients.clear();
}

// Локализованный заголовок обязательного раздела «Установка и использование»
const INSTALL_TITLES = {
  english: 'Installation & Usage',
  russian: 'Установка и использование',
  chinese: '安装与使用',
  japanese: 'インストールと使い方',
  spanish: 'Instalación y uso',
  german: 'Installation und Verwendung',
  portuguese: 'Instalação e uso',
  french: 'Installation et utilisation',
  arabic: 'التثبيت والاستخدام',
  hindi: 'इंस्टॉलेशन और उपयोग',
  indonesian: 'Instalasi dan Penggunaan',
  vietnamese: 'Cài đặt và sử dụng',
  kazakh: 'Орнату және пайдалану',
  swahili: 'Usakinishaji na matumizi',
};

const INSTALL_KEYWORDS = ['install', 'установк', 'getting started', 'начало работы', 'setup', 'quick start', 'быстрый старт', 'использовани', 'usage', '安装', 'インストール', 'instalac'];

export function isInstallTitle(title) {
  const l = String(title || '').toLowerCase();
  return INSTALL_KEYWORDS.some((k) => l.includes(k));
}

const INSTALL_MANIFESTS = ['package.json', 'requirements.txt', 'pyproject.toml', 'setup.py', 'cargo.toml', 'go.mod', 'composer.json', 'gemfile', 'makefile', 'docker-compose.yml', 'dockerfile', '.env.example', 'install.md', 'usage.md', 'bower.json'];

// Гарантирует раздел «Установка и использование» вторым после обзора (как на deepwiki.com)
function ensureInstallSection(plan, files, language) {
  if (plan.some((s) => isInstallTitle(s.title))) return;
  const picks = files
    .filter((f) => {
      if (f.includes('/')) return false;
      const l = f.toLowerCase();
      return l.startsWith('readme') || INSTALL_MANIFESTS.includes(l);
    })
    .slice(0, 8);
  if (!picks.length) return;
  plan.splice(Math.min(1, plan.length), 0, {
    title: INSTALL_TITLES[language] || INSTALL_TITLES.english,
    files: picks,
    ...(plan[0] && plan[0].group ? { group: plan[0].group } : {}),
  });
}

// Локализованный заголовок страницы «Идеи развития»
const IDEAS_TITLES = {
  english: 'Improvement Ideas',
  russian: 'Идеи развития',
  chinese: '改进建议',
  japanese: '改善アイデア',
  spanish: 'Ideas de mejora',
  german: 'Verbesserungsideen',
  portuguese: 'Ideias de melhoria',
  french: "Idées d'amélioration",
  arabic: 'أفكار للتحسين',
  hindi: 'सुधार के विचार',
  indonesian: 'Ide Pengembangan',
  vietnamese: 'Ý tưởng cải tiến',
  kazakh: 'Даму идеялары',
  swahili: 'Mawazo ya kuboresha',
};

export function createJob({ repoUrl, language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens, improvementIdeas, githubToken }) {
  const { key } = parseRepoUrl(repoUrl);
  return launch({
    id: crypto.randomUUID(),
    repoUrl, language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens,
    improvementIdeas: !!improvementIdeas,
    githubToken: githubToken || '',
    repoKey: `${key}@${language || 'russian'}`,
    status: 'queued',
    currentFile: '',
    processed: 0,
    total: 0,
    cachedHits: 0,
    partial: '',
    error: '',
    cancelled: false,
    abort: new AbortController(),
    clients: new Set(),
  });
}

export function createLocalJob({ name, language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens, files, improvementIdeas }) {
  const safe = String(name || 'folder').replace(/[^\w.-]+/g, '-').slice(0, 60) || 'folder';
  return launch({
    id: crypto.randomUUID(),
    repoUrl: `local:${safe}`,
    language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens,
    improvementIdeas: !!improvementIdeas,
    repoKey: `local/${safe}@${language || 'russian'}`,
    localFiles: files,
    status: 'queued',
    currentFile: '',
    processed: 0,
    total: 0,
    cachedHits: 0,
    partial: '',
    error: '',
    cancelled: false,
    abort: new AbortController(),
    clients: new Set(),
  });
}

// Догенерация новой языковой версии из сохранённых исходников (без повторного клонирования)
export function createLangJob({ baseKey, repoUrl, language, modelId, lmUrl, embeddingModel, maxTokens, files, improvementIdeas }) {
  return launch({
    id: crypto.randomUUID(),
    repoUrl,
    language, modelId, lmUrl, embeddingModel, maxTokens,
    improvementIdeas: !!improvementIdeas,
    maxFiles: files.length,
    repoKey: `${baseKey}@${language || 'russian'}`,
    localFiles: files,
    status: 'queued',
    currentFile: '',
    processed: 0,
    total: 0,
    cachedHits: 0,
    partial: '',
    error: '',
    cancelled: false,
    abort: new AbortController(),
    clients: new Set(),
  });
}

function launch(job) {
  jobs.set(job.id, job);
  runJob(job).catch((err) => {
    fs.rm(tempDir(job), { recursive: true, force: true }).catch(() => {});
    if (job.cancelled) emit(job, { status: 'cancelled', currentFile: '', partial: '' });
    else emit(job, { status: 'error', error: err.message });
  });
  return job;
}

function tempDir(job) {
  return path.join(path.resolve(process.env.REPOS_DIR || './data/repos'), baseRepo(job.repoKey).replace('/', '__'));
}

async function collectFiles(root, maxFilesOverride) {
  const maxSize = (parseInt(process.env.MAX_FILE_SIZE_KB, 10) || 200) * 1024;
  const ig = ignoreLib();
  try {
    ig.add(await fs.readFile(path.join(root, '.gitignore'), 'utf-8'));
  } catch {
    /* .gitignore отсутствует */
  }
  const files = [];
  async function walk(dir, rel) {
    const entries = await fs.readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      const relPath = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        if (IGNORE_DIRS.has(entry.name.toLowerCase())) continue;
        if (ig.ignores(`${relPath}/`)) continue;
        await walk(path.join(dir, entry.name), relPath);
      } else if (entry.isFile()) {
        const lower = entry.name.toLowerCase();
        if (IGNORE_FILES.has(lower)) continue;
        if (ig.ignores(relPath)) continue;
        const ext = path.extname(lower);
        if (!ALLOWED_EXT.has(ext) && lower !== 'dockerfile' && lower !== 'makefile') continue;
        const stat = await fs.stat(path.join(dir, entry.name));
        if (stat.size === 0 || stat.size > maxSize) continue;
        files.push(relPath);
      }
    }
  }
  await walk(root, '');
  files.sort();
  const maxFiles = parseInt(maxFilesOverride, 10) || parseInt(process.env.MAX_FILES, 10) || 50;
  return files.slice(0, maxFiles);
}

function buildTree(paths) {
  const root = [];
  for (const p of paths) {
    const parts = p.split('/');
    let level = root;
    let acc = '';
    parts.forEach((part, i) => {
      acc = acc ? `${acc}/${part}` : part;
      let node = level.find((n) => n.name === part);
      if (!node) {
        const isFile = i === parts.length - 1;
        node = { name: part, path: acc, type: isFile ? 'file' : 'dir' };
        if (!isFile) node.children = [];
        level.push(node);
      }
      if (node.children) level = node.children;
    });
  }
  const sortTree = (nodes) => {
    nodes.sort((a, b) => (a.type === b.type ? a.name.localeCompare(b.name) : a.type === 'dir' ? -1 : 1));
    nodes.forEach((n) => n.children && sortTree(n.children));
  };
  sortTree(root);
  return root;
}

function filterLocalFiles(list, maxFilesOverride) {
  const maxSize = (parseInt(process.env.MAX_FILE_SIZE_KB, 10) || 200) * 1024;
  const maxFiles = parseInt(maxFilesOverride, 10) || parseInt(process.env.MAX_FILES, 10) || 50;
  const map = new Map();
  for (const f of list) {
    if (!f || typeof f.path !== 'string' || typeof f.content !== 'string') continue;
    const parts = f.path.split('/');
    if (parts.some((p) => IGNORE_DIRS.has(p.toLowerCase()))) continue;
    const lower = parts[parts.length - 1].toLowerCase();
    if (IGNORE_FILES.has(lower)) continue;
    const ext = path.extname(lower);
    if (!ALLOWED_EXT.has(ext) && lower !== 'dockerfile' && lower !== 'makefile') continue;
    if (!f.content.length || Buffer.byteLength(f.content) > maxSize) continue;
    map.set(f.path, f.content);
  }
  return new Map([...map.entries()].sort((a, b) => a[0].localeCompare(b[0])).slice(0, maxFiles));
}

function extractDeps(files, contents) {
  const set = new Set(files);
  const edges = [];
  const re = /(?:import[^'"]*|require\s*\(\s*|from\s+)['"](\.{1,2}\/[^'"]+)['"]/g;
  for (const f of files) {
    const content = contents.get(f) || '';
    const dir = f.includes('/') ? f.slice(0, f.lastIndexOf('/')) : '';
    let m;
    while ((m = re.exec(content))) {
      const parts = (dir ? `${dir}/${m[1]}` : m[1]).split('/');
      const stack = [];
      for (const p of parts) {
        if (p === '.' || p === '') continue;
        else if (p === '..') stack.pop();
        else stack.push(p);
      }
      const base = stack.join('/');
      const target = set.has(base)
        ? base
        : ['.js', '.ts', '.jsx', '.tsx', '.vue', '.py', '/index.js'].map((e) => base + e).find((c) => set.has(c));
      if (target && target !== f) edges.push([f, target]);
    }
  }
  return edges.slice(0, 80);
}

function buildDirSummaries(files, docs) {
  const dirs = {};
  for (const f of files) {
    const parts = f.split('/');
    for (let i = 1; i < parts.length; i++) {
      const d = parts.slice(0, i).join('/');
      (dirs[d] = dirs[d] || []).push(f);
    }
  }
  const summaries = {};
  for (const [d, members] of Object.entries(dirs)) {
    const heads = members
      .slice(0, 3)
      .map((f) => (docs[f] || '').split('\n').find((l) => l.trim()) || f)
      .join('; ');
    summaries[d] = `Папка ${d}: ${members.length} файлов (${members.slice(0, 5).map((f) => f.split('/').pop()).join(', ')}). ${heads}`.slice(0, 400);
  }
  return summaries;
}

function firstLine(content) {
  return ((content || '').split('\n').find((l) => l.trim().length > 2) || '').trim().slice(0, 80);
}

function slugify(t) {
  return (
    t.toLowerCase().replace(/[^a-zа-яё0-9]+/gi, '-').replace(/^-+|-+$/g, '').slice(0, 50) || 'section'
  );
}

// Разбор JSON-плана разделов от LLM с валидацией путей
function parseSectionPlan(raw, files) {
  const m = String(raw).match(/\[[\s\S]*\]/);
  if (!m) return null;
  let arr;
  try {
    arr = JSON.parse(m[0]);
  } catch {
    return null;
  }
  if (!Array.isArray(arr)) return null;
  const known = new Set(files);
  const used = new Set();
  const plan = [];
  const push = (title, list, group) => {
    const fl = list.filter((f) => known.has(f) && !used.has(f));
    fl.forEach((f) => used.add(f));
    if (fl.length) {
      plan.push({
        title: String(title).trim().slice(0, 120),
        files: fl,
        ...(group ? { group: String(group).trim().slice(0, 120) } : {}),
      });
    }
  };
  for (const s of arr) {
    if (!s) continue;
    // Двухуровневый формат: {chapter, sections:[{title, files}]}
    if (typeof s.chapter === 'string' && Array.isArray(s.sections)) {
      for (const sub of s.sections) {
        if (sub && typeof sub.title === 'string' && Array.isArray(sub.files)) push(sub.title, sub.files, s.chapter);
      }
    } else if (typeof s.title === 'string' && Array.isArray(s.files)) {
      push(s.title, s.files, s.group);
    }
  }
  const rest = files.filter((f) => !used.has(f));
  if (rest.length && plan.length) plan.push({ title: 'Прочие файлы', files: rest });
  return plan.length ? plan : null;
}

// Fallback: группировка по папкам верхнего уровня
function fallbackSections(files) {
  const groups = new Map();
  for (const f of files) {
    const top = f.includes('/') ? f.split('/')[0] : 'Корень проекта';
    if (!groups.has(top)) groups.set(top, []);
    groups.get(top).push(f);
  }
  return [...groups.entries()].map(([d, fl]) => ({
    title: d === 'Корень проекта' ? d : `Модуль ${d}`,
    files: fl,
  }));
}

function buildSectionContext(secFiles, contentsMap, budgetTokens) {
  const perFile = Math.max(400, Math.floor(budgetTokens / Math.max(1, secFiles.length)));
  let ctx = '';
  for (const f of secFiles) {
    ctx += `### ${f}\n\`\`\`\n${sliceByTokens(contentsMap.get(f) || '', perFile)}\n\`\`\`\n\n`;
  }
  return ctx;
}

// Эвристика для новых файлов: раздел с самым длинным общим префиксом пути
function assignHeuristic(plan, f) {
  const fb = f.split('/');
  let best = plan[plan.length - 1];
  let bestScore = -1;
  for (const s of plan) {
    let score = 0;
    for (const sf of s.files) {
      const a = sf.split('/');
      let i = 0;
      while (i < a.length - 1 && i < fb.length - 1 && a[i] === fb[i]) i++;
      score = Math.max(score, i);
    }
    if (score > bestScore) {
      bestScore = score;
      best = s;
    }
  }
  return best;
}

function parseAssignments(raw) {
  const m = String(raw).match(/\{[\s\S]*\}/);
  if (!m) return null;
  try {
    const obj = JSON.parse(m[0]);
    return obj && typeof obj === 'object' && !Array.isArray(obj) ? obj : null;
  } catch {
    return null;
  }
}

async function runJob(job) {
  const targetDir = tempDir(job);
  let files;
  let localMap = null;

  if (job.localFiles) {
    emit(job, { status: 'parsing', currentFile: '' });
    localMap = filterLocalFiles(job.localFiles, job.maxFiles);
    files = [...localMap.keys()];
  } else {
    emit(job, { status: 'cloning', currentFile: job.repoUrl });
    await cloneRepo(job.repoUrl, targetDir, job.githubToken);
    emit(job, { status: 'parsing', currentFile: '' });
    files = await collectFiles(targetDir, job.maxFiles);
  }
  if (!files.length) throw new Error('Не найдено подходящих исходных файлов');
  emit(job, { total: files.length });

  const docs = {};
  const changedFiles = new Set();
  const contentsMap = new Map();
  for (const relPath of files) {
    const content = localMap
      ? localMap.get(relPath)
      : await fs.readFile(path.join(targetDir, relPath), 'utf-8');
    contentsMap.set(relPath, content);
    const prevSrc = await getCache(`src:${baseRepo(job.repoKey)}:${relPath}`);
    if (prevSrc !== content) changedFiles.add(relPath);
    await setCache(`src:${baseRepo(job.repoKey)}:${relPath}`, content);
  }

  // Этап 1: план разделов wiki (LLM группирует файлы в темы)
  emit(job, { status: 'sectioning', currentFile: 'Планирование разделов wiki', partial: '' });
  // Диф-обновление: если wiki уже существует, переиспользуем прежний план разделов
  // и перегенерируем только разделы, затронутые изменёнными/новыми/удалёнными файлами
  const prevRecord0 = await getCache(`repo:${job.repoKey}`);
  const prevSections = prevRecord0 && Array.isArray(prevRecord0.sections)
    ? prevRecord0.sections.filter((s) => s && !s.ideas && s.title)
    : [];
  const prevMdByTitle = new Map(prevSections.map((s) => [s.title, s.md]));
  const prevFilesByTitle = new Map(prevSections.map((s) => [s.title, s.files || []]));
  let plan = null;
  if (prevSections.length) {
    const fileSet = new Set(files);
    const known = new Set(prevSections.flatMap((s) => s.files || []));
    plan = prevSections.map((s) => ({
      title: s.title,
      files: (s.files || []).filter((f) => fileSet.has(f)),
      ...(s.group ? { group: s.group } : {}),
    }));
    const addedFiles = files.filter((f) => !known.has(f));
    if (addedFiles.length && plan.some((s) => s.files.length)) {
      emit(job, { currentFile: 'Распределение новых файлов' });
      let assigned = null;
      try {
        const raw = await assignNewFiles({
          lmUrl: job.lmUrl,
          modelId: job.modelId,
          sections: plan,
          newFiles: addedFiles,
          signal: job.abort.signal,
        });
        assigned = parseAssignments(raw);
      } catch (e) {
        if (job.cancelled) throw e;
      }
      for (const f of addedFiles) {
        const target = (assigned && plan.find((s) => s.title === assigned[f])) || assignHeuristic(plan, f);
        target.files.push(f);
      }
    }
    plan = plan.filter((s) => s.files.length);
    if (!plan.length) plan = null;
  }
  if (!plan) {
    const fileSummaries = files.map((f) => `${f}: ${firstLine(contentsMap.get(f))}`).join('\n').slice(0, 20000);
    const planHash = crypto
      .createHash('sha256')
      .update(`plan::${files.join(',')}::${job.language}::${job.modelId}::plan-v2`)
      .digest('hex');
    plan = await getCache(`cache:${planHash}`);
    if (!plan) {
      try {
        const raw = await planSections({
          lmUrl: job.lmUrl,
          modelId: job.modelId,
          language: job.language,
          repoKey: baseRepo(job.repoKey),
          fileSummaries,
          signal: job.abort.signal,
        });
        plan = parseSectionPlan(raw, files);
      } catch (e) {
        if (job.cancelled) throw e;
        plan = null;
      }
      if (!plan) plan = fallbackSections(files);
      await setCache(`cache:${planHash}`, plan);
    }
  }
  ensureInstallSection(plan, files, job.language);

  // Этап 2: генерация страницы каждого раздела
  // Инкрементальное сохранение: .md пишется сразу после генерации каждого раздела
  const docsOut = path.join(path.resolve(process.env.DOCS_DIR || './data/docs'), job.repoKey.replace('/', '__'));
  await fs.rm(docsOut, { recursive: true, force: true });
  await fs.mkdir(docsOut, { recursive: true });
  const sectionsMeta = plan.map(({ title, group }, i) => ({
    id: `${String(i + 1).padStart(2, '0')}-${slugify(title)}`,
    title,
    ...(group ? { group } : {}),
    done: false,
  }));
  emit(job, { status: 'generating', total: plan.length, processed: 0, sectionsMeta });
  // README для первой страницы: помогает LLM определить назначение проекта
  const readmeFile = files.find((f) => /(^|\/)readme(\.|$)/i.test(f));
  const projectHint = readmeFile ? (contentsMap.get(readmeFile) || '').slice(0, 2000) : '';
  const sections = [];
  const updatedSectionIds = [];
  for (let i = 0; i < plan.length; i++) {
    if (job.cancelled) throw new Error('Отменено пользователем');
    const { title, files: secFiles } = plan[i];
    const isInstall = isInstallTitle(title);
    emit(job, { currentFile: title, partial: '' });
    const codeContext = buildSectionContext(secFiles, contentsMap, getMaxTokens(job.maxTokens) - 2500);
    const hash = crypto
      .createHash('sha256')
      .update(`section::${title}::${secFiles.map((f) => contentsMap.get(f)).join('\x00')}::${job.language}::${job.modelId}${i === 0 ? '::first-v2' : ''}${isInstall ? '::install-v1' : ''}`)
      .digest('hex');
    // Диф-обновление: раздел без изменённых файлов берём из прежней записи как есть
    const prevSecFiles = prevFilesByTitle.get(title);
    const secUnchanged =
      prevSecFiles &&
      JSON.stringify(prevSecFiles) === JSON.stringify(secFiles) &&
      !secFiles.some((f) => changedFiles.has(f)) &&
      prevMdByTitle.get(title);
    let mdText = secUnchanged ? prevMdByTitle.get(title) : await getCache(`cache:${hash}`);
    if (mdText) {
      job.cachedHits += 1;
    } else {
      updatedSectionIds.push(sectionsMeta[i].id);
      let lastEmit = 0;
      // Для раздела установки — специализированный промпт без mermaid-диаграммы
      const genFn = isInstall ? generateInstallDoc : generateSectionDoc;
      mdText = await genFn({
        lmUrl: job.lmUrl,
        modelId: job.modelId,
        language: job.language,
        repoKey: baseRepo(job.repoKey),
        title,
        codeContext,
        isFirst: i === 0,
        projectHint: i === 0 ? projectHint : '',
        signal: job.abort.signal,
        onChunk: (text) => {
          const now = Date.now();
          if (now - lastEmit > 400) {
            lastEmit = now;
            emit(job, { partial: text });
          }
        },
      });
      await setCache(`cache:${hash}`, mdText);
    }
    const section = { id: sectionsMeta[i].id, title, files: secFiles, md: mdText, ...(plan[i].group ? { group: plan[i].group } : {}) };
    sections.push(section);
    await fs.writeFile(path.join(docsOut, `${section.id}.md`), mdText, 'utf-8');
    sectionsMeta[i].done = true;
    emit(job, { processed: job.processed + 1, partial: '', sectionsMeta });
  }

  let embeddingModel = await resolveEmbeddingModel(job.lmUrl, job.embeddingModel);
  if (embeddingModel) {
    emit(job, { status: 'embedding', currentFile: '', partial: '' });
    for (const relPath of files) {
      if (job.cancelled) throw new Error('Отменено пользователем');
      emit(job, { currentFile: relPath });
      try {
        if (
          !changedFiles.has(relPath) &&
          (await getCache(`emb:${baseRepo(job.repoKey)}:${relPath}:0`)) !== null
        ) {
          continue;
        }
        const content = contentsMap.get(relPath);
        await embedFile({
          repoKey: baseRepo(job.repoKey),
          filePath: relPath,
          content,
          lmUrl: job.lmUrl,
          model: embeddingModel,
          signal: job.abort.signal,
        });
      } catch (e) {
        if (job.cancelled) throw e;
        embeddingModel = null;
        break;
      }
    }
  }

  if (job.cancelled) throw new Error('Отменено пользователем');
  emit(job, { status: 'overview', currentFile: 'Общий обзор репозитория', partial: '' });
  const excerpts = sections
    .map((s) => `### ${s.title}\n${s.md.slice(0, 500)}`)
    .join('\n\n')
    .slice(0, 24000);
  const ovHash = crypto
    .createHash('sha256')
    .update(`overview::${excerpts}::${job.language}::${job.modelId}`)
    .digest('hex');
  let overview = await getCache(`cache:${ovHash}`);
  if (!overview) {
    let lastEmit = 0;
    overview = await generateOverview({
      lmUrl: job.lmUrl,
      modelId: job.modelId,
      language: job.language,
      repoKey: job.repoKey,
      excerpts,
      signal: job.abort.signal,
      onChunk: (text) => {
        const now = Date.now();
        if (now - lastEmit > 400) {
          lastEmit = now;
          emit(job, { partial: text });
        }
      },
    });
    await setCache(`cache:${ovHash}`, overview);
  }

  if (overview) await fs.writeFile(path.join(docsOut, '_OVERVIEW.md'), overview, 'utf-8');

  // Доп. страница «Идеи развития» (включается в настройках, по умолчанию выключена)
  if (job.improvementIdeas && !job.cancelled) {
    const ideasTitle = IDEAS_TITLES[job.language] || IDEAS_TITLES.english;
    emit(job, { currentFile: ideasTitle, partial: '' });
    const ideasHash = crypto
      .createHash('sha256')
      .update(`ideas::${excerpts}::${job.language}::${job.modelId}`)
      .digest('hex');
    let ideasMd = await getCache(`cache:${ideasHash}`);
    if (!ideasMd) {
      let lastEmit = 0;
      ideasMd = await generateIdeas({
        lmUrl: job.lmUrl,
        modelId: job.modelId,
        language: job.language,
        repoKey: baseRepo(job.repoKey),
        title: ideasTitle,
        excerpts,
        signal: job.abort.signal,
        onChunk: (text) => {
          const now = Date.now();
          if (now - lastEmit > 400) {
            lastEmit = now;
            emit(job, { partial: text });
          }
        },
      });
      await setCache(`cache:${ideasHash}`, ideasMd);
    }
    if (ideasMd) {
      const ideasId = `${String(plan.length + 1).padStart(2, '0')}-improvement-ideas`;
      sections.push({ id: ideasId, title: ideasTitle, files: [], md: ideasMd, ideas: true });
      await fs.writeFile(path.join(docsOut, `${ideasId}.md`), ideasMd, 'utf-8');
    }
    emit(job, { partial: '' });
  }

  const deps = extractDeps(files, contentsMap);
  const heads = {};
  for (const f of files) heads[f] = firstLine(contentsMap.get(f));
  const dirSummaries = buildDirSummaries(files, heads);
  try {
    await delPrefix(`dirsum:${job.repoKey}:`);
    const dirPaths = Object.keys(dirSummaries);
    if (dirPaths.length) {
      const vectors = await embedTexts(dirPaths.map((d) => dirSummaries[d]));
      for (let i = 0; i < dirPaths.length; i++) {
        await setCache(`dirsum:${job.repoKey}:${dirPaths[i]}`, { summary: dirSummaries[dirPaths[i]], vector: vectors[i] });
      }
    }
  } catch {
    /* dir-эмбеддинги необязательны */
  }

  const prevRecord = await getCache(`repo:${job.repoKey}`);
  let changes = null;
  const prevFiles = prevRecord ? prevRecord.files || Object.keys(prevRecord.docs || {}) : null;
  if (prevFiles && prevFiles.length) {
    const added = files.filter((f) => !prevFiles.includes(f));
    const modified = files.filter((f) => prevFiles.includes(f) && changedFiles.has(f));
    const removed = prevFiles.filter((f) => !files.includes(f));
    changes = { added, modified, removed, sectionsUpdated: updatedSectionIds, updatedAt: new Date().toISOString() };
    for (const f of removed) {
      await delCache(`src:${baseRepo(job.repoKey)}:${f}`);
      await delPrefix(`emb:${baseRepo(job.repoKey)}:${f}:`);
    }
  }

  const record = {
    repoUrl: job.repoUrl,
    repoKey: job.repoKey,
    language: job.language,
    modelId: job.modelId,
    generatedAt: new Date().toISOString(),
    docsDir: docsOut,
    embeddingModel,
    changes,
    deps,
    dirSummaries,
    overview,
    files,
    sections,
    tree: buildTree(files),
  };
  await setCache(`repo:${job.repoKey}`, record);

  fs.rm(targetDir, { recursive: true, force: true }).catch(() => {});
  emit(job, { status: 'done', currentFile: '', partial: '' });
}
