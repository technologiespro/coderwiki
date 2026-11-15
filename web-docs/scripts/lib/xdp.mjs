// Импорт .xdp (контейнер XDP2 CoderWiki): ZSTD → tar → records.json → папки owner__repo@language-docs
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { readTar } from './tar.mjs';

const ZSTD_MAGIC = [0x28, 0xb5, 0x2f, 0xfd];

// Встроенный zstd Node.js (22.15+/23.8+, словари — 24+), иначе CLI xdp-util
function decompressNative(frame, dict) {
  if (typeof zlib.zstdDecompressSync !== 'function') return null;
  try {
    return zlib.zstdDecompressSync(frame, {
      ...(dict ? { dictionary: dict } : {}),
      params: { [zlib.constants.ZSTD_d_windowLogMax]: 31 },
    });
  } catch {
    return null;
  }
}

function findCli(root) {
  const exe = process.platform === 'win32' ? '.exe' : '';
  const candidates = [
    process.env.XDP_UTIL_BIN,
    path.resolve(root, `../xdp-rust/target/release/xdp-util${exe}`),
    path.resolve(root, `../xdp-rust/target/debug/xdp-util${exe}`),
  ].filter(Boolean);
  return candidates.find((p) => fs.existsSync(p)) || `xdp-util${exe}`;
}

function decompressCli(file, tmpDir, root) {
  const out = path.join(tmpDir, `${path.basename(file)}.tar`);
  const bin = findCli(root);
  try {
    execFileSync(bin, ['unpack', '--input', file, '--output', out], { stdio: ['ignore', 'ignore', 'pipe'] });
  } catch (e) {
    const msg = e.code === 'ENOENT' ? `не найден xdp-util (${bin}); задайте XDP_UTIL_BIN` : String(e.stderr || e.message).trim();
    throw new Error(msg);
  }
  const tar = fs.readFileSync(out);
  fs.rmSync(out, { force: true });
  return tar;
}

export function xdpToTar(file, tmpDir, root) {
  const data = fs.readFileSync(file);
  if (data.length > 8 && data.subarray(0, 4).toString('latin1') === 'XDP2') {
    const dictLen = data.readUInt32LE(4);
    if (data.length < 8 + dictLen) throw new Error('повреждённый контейнер (длина словаря)');
    const dict = dictLen ? data.subarray(8, 8 + dictLen) : null;
    return decompressNative(data.subarray(8 + dictLen), dict) || decompressCli(file, tmpDir, root);
  }
  if (ZSTD_MAGIC.every((b, i) => data[i] === b)) return decompressNative(data, null) || decompressCli(file, tmpDir, root);
  throw new Error('не XDP2-контейнер');
}

// repoKey: owner/name@language или local/owner/name@language
function parseRepoKey(key, language) {
  const [base, lang] = String(key || '').split('@');
  const parts = base.split('/').filter(Boolean);
  if (parts[0] === 'local' && parts.length > 1) parts.shift();
  const repo = parts.pop();
  const owner = parts.pop() || 'local';
  return { owner, repo, langName: String(language || lang || 'english').toLowerCase() };
}

// Разворачивает все языковые версии из .xdp; возвращает кандидатов-источников для sync
export function expandXdp(file, outRoot, root) {
  const entries = readTar(xdpToTar(file, outRoot, root));
  const json = (name) => {
    const b = entries.get(name);
    return b ? JSON.parse(b.toString('utf8')) : null;
  };
  const many = json('records.json');
  const records = Array.isArray(many) && many.length ? many : [json('record.json')].filter(Boolean);
  if (!records.length) throw new Error('нет records.json / record.json');
  const mtime = fs.statSync(file).mtimeMs;
  const out = [];
  for (const rec of records) {
    if (!Array.isArray(rec.sections) || !rec.repoKey) continue;
    const { owner, repo, langName } = parseRepoKey(rec.repoKey, rec.language);
    const stem = `${owner}__${repo}@${langName}-docs`;
    const dir = path.join(outRoot, stem);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    if (rec.overview) fs.writeFileSync(path.join(dir, '_OVERVIEW.md'), rec.overview);
    const meta = { sections: {}, repoFiles: Array.isArray(rec.files) ? rec.files : [] };
    for (const s of rec.sections) {
      const id = String(s.id || 'section').replace(/[\\/]/g, '-');
      fs.writeFileSync(path.join(dir, `${id}.md`), s.md || '');
      meta.sections[id] = { files: Array.isArray(s.files) ? s.files : [] };
    }
    fs.writeFileSync(path.join(dir, '.cw-meta.json'), JSON.stringify(meta));
    out.push({ stem, owner, repo, langName, full: dir, isZip: false, mtime, origin: path.basename(file) });
  }
  if (!out.length) throw new Error('в контейнере нет разделов документации');
  return out;
}
