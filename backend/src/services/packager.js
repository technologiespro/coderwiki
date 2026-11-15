import archiver from 'archiver';
import fs from 'fs/promises';
import { existsSync } from 'fs';
import path from 'path';
import os from 'os';
import crypto from 'crypto';
import { execFile, spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { getByPrefix, getCache, setCache } from './db.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Поиск Rust-утилиты xdp-util (приоритет: XDP_UTIL_PATH из .env > release > debug)
function findXdpUtil() {
  const exe = process.platform === 'win32' ? 'xdp-util.exe' : 'xdp-util';
  const root = path.resolve(__dirname, '../../..');
  const candidates = [
    process.env.XDP_UTIL_PATH,
    path.join(root, 'xdp-rust', 'target', 'release', exe),
    path.join(root, 'xdp-rust', 'target', 'debug', exe),
  ].filter(Boolean);
  return candidates.find((p) => existsSync(p)) || null;
}

function runXdpUtil(bin, args) {
  return new Promise((resolve, reject) => {
    execFile(bin, args, { maxBuffer: 1024 * 1024 }, (err, _stdout, stderr) => {
      if (err) reject(new Error(`xdp-util: ${stderr || err.message}`));
      else resolve(stderr);
    });
  });
}

// Стриминговый запуск: stderr читается построчно (прогресс обучения словаря)
function runXdpUtilStream(bin, args, onLine) {
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args);
    let buf = '';
    let lastLines = [];
    child.stderr.on('data', (d) => {
      buf += d.toString();
      let idx;
      while ((idx = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, idx).trim();
        buf = buf.slice(idx + 1);
        if (line) {
          lastLines = [...lastLines.slice(-4), line];
          if (onLine) onLine(line);
        }
      }
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`xdp-util exit ${code}: ${lastLines.join(' | ')}`));
    });
  });
}

async function withTmpFiles(fn) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'xdp-'));
  try {
    return await fn(dir);
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
}

let zstdPromise = null;
function getZstd() {
  if (!zstdPromise) {
    zstdPromise = import('zlib').then((zlib) => {
      const hasZstd = typeof zlib.zstdCompressSync === 'function';
      return {
        name: hasZstd ? 'node:zlib zstd' : 'gzip-fallback',
        compress: async (b) => (hasZstd ? zlib.zstdCompressSync(b) : zlib.gzipSync(b, { level: 9 })),
        decompress: async (b) => {
          if (!hasZstd) throw new Error('Для распаковки zstd нужен Node.js 22.15+ или 24 (либо переэкспортируйте .xdp на этой системе — будет gzip)');
          return zlib.zstdDecompressSync(b);
        },
        gunzip: async (b) => zlib.gunzipSync(b),
      };
    });
  }
  return zstdPromise;
}

function geneticDictionary(samples) {
  const freq = new Map();
  for (const s of samples) {
    const start = Math.floor(Math.random() * Math.max(1, s.length - 2000));
    for (const term of s.slice(start, start + 2000).split(/[^\w$.]+/)) {
      if (term.length < 3) continue;
      freq.set(term, (freq.get(term) || 0) + 1);
    }
  }
  return [...freq.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 300)
    .map(([t]) => t);
}

export async function buildXdp(repoKey, onProgress) {
  const record = await getCache(`repo:${repoKey}`);
  if (!record) return null;
  const base = repoKey.split('@')[0];
  // Все языковые версии репозитория — в один контейнер (общие исходники и индекс не дублируются)
  const allRecords = (await getByPrefix('repo:'))
    .filter(({ key }) => key.slice(5).split('@')[0] === base)
    .map(({ key, value }) => ({ ...value, repoKey: key.slice(5) }));
  const srcs = await getByPrefix(`src:${base}:`);
  const embs = await getByPrefix(`emb:${base}:`);
  const prefix = `src:${base}:`.length;

  const samples = [
    ...allRecords.flatMap((r) => (r.sections || []).map((s) => s.md)),
    ...allRecords.flatMap((r) => Object.values(r.docs || {})),
    ...srcs.map((s) => s.value),
  ];
  const dictionary = geneticDictionary(samples);

  const archive = archiver('tar');
  const chunks = [];
  archive.on('data', (c) => chunks.push(c));
  const done = new Promise((resolve, reject) => {
    archive.on('end', resolve);
    archive.on('error', reject);
  });

  archive.append(
    JSON.stringify({ format: 'xdp/2', repoKey, baseKey: base, languages: allRecords.map((r) => r.language), generatedAt: record.generatedAt, language: record.language, modelId: record.modelId, dictionaryTerms: dictionary.length }, null, 2),
    { name: 'manifest.json' }
  );
  archive.append(dictionary.join('\n'), { name: 'dictionary.txt' });
  // record.json — версия, из которой запущен экспорт (совместимость со старыми импортёрами)
  archive.append(JSON.stringify(record), { name: 'record.json' });
  archive.append(JSON.stringify(allRecords), { name: 'records.json' });
  if (record.overview) archive.append(record.overview, { name: 'docs/_OVERVIEW.md' });
  for (const r of allRecords) {
    const lang = r.language || 'default';
    for (const s of r.sections || []) archive.append(s.md, { name: `sections/${lang}/${s.id}.md` });
  }
  for (const [rel, mdText] of Object.entries(record.docs || {})) archive.append(mdText, { name: `docs/${rel}.md` });
  for (const s of srcs) archive.append(s.value, { name: `src/${s.key.slice(prefix)}` });
  archive.append(
    JSON.stringify(embs.map(({ key, value }) => ({ key, ...value }))),
    { name: 'index/vectors.json' }
  );
  archive.finalize();
  await done;
  const tarBuf = Buffer.concat(chunks);

  // Приоритет: Rust-утилита (ZSTD + генетический словарь), иначе JS-fallback
  const bin = findXdpUtil();
  if (bin) {
    try {
      return await withTmpFiles(async (dir) => {
        const tarPath = path.join(dir, 'in.tar');
        const xdpPath = path.join(dir, 'out.xdp');
        await fs.writeFile(tarPath, tarBuf);
        let lastLine = '';
        await runXdpUtilStream(bin, ['pack', '--input', tarPath, '--output', xdpPath, '--level', '19'], (line) => {
          lastLine = line;
          if (onProgress) onProgress(line);
        });
        console.log(`[packager] rust xdp-util: ${lastLine}`);
        return await fs.readFile(xdpPath);
      });
    } catch (e) {
      console.warn(`[packager] rust xdp-util упал (${e.message}), fallback на JS`);
    }
  }
  const zstd = await getZstd();
  return Buffer.from(await zstd.compress(tarBuf));
}

function parseTar(buf) {
  const entries = [];
  let off = 0;
  while (off + 512 <= buf.length) {
    const name = buf.subarray(off, off + 100).toString().replace(/\0.*$/s, '');
    if (!name) break;
    const size = parseInt(buf.subarray(off + 124, off + 136).toString().replace(/\0.*$/s, '').trim(), 8) || 0;
    entries.push({ name, data: buf.subarray(off + 512, off + 512 + size) });
    off += 512 + Math.ceil(size / 512) * 512;
  }
  return entries;
}

export async function importXdp(buffer) {
  const buf = Buffer.from(buffer);
  let tarBuf;
  if (buf.length > 8 && buf.subarray(0, 4).toString() === 'XDP2') {
    // Контейнер XDP2 (Rust): magic + словарь + zstd-фрейм
    const bin = findXdpUtil();
    if (!bin) throw new Error('Этот .xdp создан Rust-утилитой (формат XDP2). Соберите её: cd xdp-rust && cargo build --release');
    tarBuf = await withTmpFiles(async (dir) => {
      const xdpPath = path.join(dir, 'in.xdp');
      const tarPath = path.join(dir, 'out.tar');
      await fs.writeFile(xdpPath, buf);
      await runXdpUtil(bin, ['unpack', '--input', xdpPath, '--output', tarPath]);
      return await fs.readFile(tarPath);
    });
  } else if (buf[0] === 0x1f && buf[1] === 0x8b) {
    const zstd = await getZstd();
    tarBuf = Buffer.from(await zstd.gunzip(buf));
  } else if (buf[0] === 0x28 && buf[1] === 0xb5 && buf[2] === 0x2f && buf[3] === 0xfd) {
    const zstd = await getZstd();
    tarBuf = Buffer.from(await zstd.decompress(buf));
  } else {
    throw new Error('Неизвестный формат файла (ожидается XDP2, zstd или gzip контейнер .xdp)');
  }
  const entries = parseTar(tarBuf);
  // xdp/2: records.json со всеми языковыми версиями; иначе одиночный record.json (старый формат)
  const recsEntry = entries.find((e) => e.name === 'records.json');
  let records = null;
  if (recsEntry) {
    try {
      records = JSON.parse(recsEntry.data.toString());
    } catch {
      records = null;
    }
  }
  if (!Array.isArray(records) || !records.length) {
    const recEntry = entries.find((e) => e.name === 'record.json');
    if (!recEntry) throw new Error('Файл .xdp не содержит record.json (старый формат — переэкспортируйте контейнер)');
    records = [JSON.parse(recEntry.data.toString())];
  }
  const repoKeys = [];
  for (const record of records) {
    if (!record.repoKey || !(record.docs || record.sections)) continue;
    await setCache(`repo:${record.repoKey}`, record);
    const outDir = path.join(path.resolve(process.env.DOCS_DIR || './data/docs'), record.repoKey.replace('/', '__'));
    await fs.rm(outDir, { recursive: true, force: true });
    await fs.mkdir(outDir, { recursive: true });
    for (const s of record.sections || []) {
      await fs.writeFile(path.join(outDir, `${s.id}.md`), s.md, 'utf-8');
    }
    for (const [rel, mdText] of Object.entries(record.docs || {})) {
      const f = path.join(outDir, `${rel}.md`);
      await fs.mkdir(path.dirname(f), { recursive: true });
      await fs.writeFile(f, mdText, 'utf-8');
    }
    repoKeys.push(record.repoKey);
  }
  if (!repoKeys.length) throw new Error('Повреждённый record.json в .xdp');
  const base = repoKeys[0].split('@')[0];
  for (const e of entries) {
    if (e.name.startsWith('src/')) {
      await setCache(`src:${base}:${e.name.slice(4)}`, e.data.toString());
    } else if (e.name === 'index/vectors.json') {
      for (const v of JSON.parse(e.data.toString())) {
        const { key, ...val } = v;
        if (key) await setCache(key, val);
      }
    }
  }
  return repoKeys;
}
