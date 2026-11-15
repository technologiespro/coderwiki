import { Level } from 'level';
import path from 'path';

const db = new Level(path.resolve(process.env.LEVELDB_PATH || './data/leveldb'), {
  valueEncoding: 'json',
});

export async function getCache(key) {
  try {
    const value = await db.get(key);
    return value === undefined ? null : value;
  } catch {
    return null;
  }
}

export async function setCache(key, value) {
  await db.put(key, value);
}

export async function delCache(key) {
  await db.del(key);
}

export async function getByPrefix(prefix) {
  const out = [];
  for await (const [key, value] of db.iterator({ gte: prefix, lt: `${prefix}\xff` })) {
    out.push({ key, value });
  }
  return out;
}

export async function delPrefix(prefix) {
  const ops = [];
  for await (const key of db.keys({ gte: prefix, lt: `${prefix}\xff` })) {
    ops.push({ type: 'del', key });
  }
  if (ops.length) await db.batch(ops);
}

export async function listRepos() {
  const repos = [];
  for await (const [key, value] of db.iterator({ gte: 'repo:', lt: 'repo:\xff' })) {
    repos.push({
      repoKey: key.slice(5),
      repoUrl: value.repoUrl,
      language: value.language,
      modelId: value.modelId,
      generatedAt: value.generatedAt,
      fileCount: (value.files || Object.keys(value.docs || {})).length,
      sectionCount: (value.sections || []).length,
    });
  }
  return repos;
}

export default db;
