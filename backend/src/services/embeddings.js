import { fetchModels, getEmbeddings } from './lmstudio.js';
import { setCache, getByPrefix, delPrefix, getCache } from './db.js';

let extractorPromise = null;
function getExtractor() {
  if (!extractorPromise) {
    extractorPromise = import('@xenova/transformers').then(({ pipeline, env }) => {
      env.allowLocalModels = false;
      return pipeline('feature-extraction', 'Xenova/all-MiniLM-L6-v2');
    });
  }
  return extractorPromise;
}

export async function embedTexts(texts) {
  const extractor = await getExtractor();
  const out = [];
  for (const t of texts) {
    const res = await extractor(String(t).slice(0, 2000), { pooling: 'mean', normalize: true });
    out.push(Array.from(res.data));
  }
  return out;
}

const DEFAULT_PARAMS = { topK: 5, chunkLines: 60, semanticWeight: 0.7, bm25Weight: 0.3 };

export async function getRagParams(repoKey) {
  return (await getCache(`ragparams:${repoKey}`)) || { ...DEFAULT_PARAMS };
}

export async function mutateRagParams(repoKey) {
  const p = await getRagParams(repoKey);
  const shift = (Math.random() - 0.5) * 0.2;
  p.semanticWeight = Math.min(0.9, Math.max(0.3, p.semanticWeight + shift));
  p.bm25Weight = 1 - p.semanticWeight;
  p.topK = Math.min(8, Math.max(3, p.topK + (Math.random() > 0.5 ? 1 : -1)));
  await setCache(`ragparams:${repoKey}`, p);
  return p;
}

function keywordScore(question, text) {
  const words = [...new Set(question.toLowerCase().split(/[^a-zа-яё0-9_]+/i).filter((w) => w.length > 2))];
  if (!words.length) return 0;
  const lower = text.toLowerCase();
  let hits = 0;
  for (const w of words) if (lower.includes(w)) hits++;
  return hits / words.length;
}

const CHUNK_LINES = 60;
const OVERLAP = 10;

const BOUNDARY =
  /^\s*(export\s+)?(default\s+)?(async\s+)?(function\b|class\b|def\s|fn\s|func\s|interface\s|impl\s|public\s|private\s|protected\s|const\s+\w+\s*=\s*(async\s*)?(\(|function))/;

export function chunkCode(content) {
  const lines = content.split('\n');
  const chunks = [];
  const push = (s, e) => {
    const text = lines.slice(s, e).join('\n').trim();
    if (text.length >= 30) chunks.push({ text: text.slice(0, 4000), startLine: s + 1, endLine: e });
  };
  let start = 0;
  let count = 0;
  for (let i = 0; i < lines.length; i++) {
    count++;
    const nextIsBoundary = i + 1 < lines.length && BOUNDARY.test(lines[i + 1]);
    if ((count >= CHUNK_LINES - OVERLAP && nextIsBoundary) || count >= CHUNK_LINES * 2) {
      push(start, i + 1);
      start = i + 1;
      count = 0;
    }
  }
  if (start < lines.length) push(start, lines.length);
  return chunks;
}

export function cosineSimilarity(a, b) {
  let dot = 0;
  let na = 0;
  let nb = 0;
  const len = Math.min(a.length, b.length);
  for (let i = 0; i < len; i++) {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  const denom = Math.sqrt(na) * Math.sqrt(nb);
  return denom ? dot / denom : 0;
}

export async function resolveEmbeddingModel(lmUrl, override) {
  try {
    await getExtractor();
    return 'Xenova/all-MiniLM-L6-v2';
  } catch {
    /* локальная модель недоступна — пробуем LM Studio */
  }
  if (override) return override;
  if (process.env.EMBEDDING_MODEL) return process.env.EMBEDDING_MODEL;
  try {
    const models = await fetchModels(lmUrl);
    const m = models.find((x) => /embed/i.test(x.id));
    return m ? m.id : null;
  } catch {
    return null;
  }
}

async function vectorize({ texts, lmUrl, model }) {
  if (model === 'Xenova/all-MiniLM-L6-v2') return embedTexts(texts);
  return getEmbeddings({ lmUrl, model, input: texts });
}

export async function embedFile({ repoKey, filePath, content, lmUrl, model, signal }) {
  const chunks = chunkCode(content);
  await delPrefix(`emb:${repoKey}:${filePath}:`);
  if (!chunks.length) return 0;
  const vectors = await vectorize({ texts: chunks.map((c) => c.text), lmUrl, model });
  for (let i = 0; i < chunks.length; i++) {
    if (!vectors[i]) continue;
    await setCache(`emb:${repoKey}:${filePath}:${i}`, { ...chunks[i], vector: vectors[i] });
  }
  return chunks.length;
}

export async function searchChunks({ repoKey, question, lmUrl, embeddingModel, topK }) {
  const model = await resolveEmbeddingModel(lmUrl, embeddingModel);
  if (!model) return null;
  const params = await getRagParams(repoKey);
  const all = await getByPrefix(`emb:${repoKey}:`);
  if (!all.length) return null;
  const [qv] = await vectorize({ texts: [question], lmUrl, model });
  if (!qv) return null;
  const prefixLen = `emb:${repoKey}:`.length;
  return all
    .map(({ key, value }) => ({
      filePath: key.slice(prefixLen).replace(/:\d+$/, ''),
      text: value.text,
      startLine: value.startLine,
      endLine: value.endLine,
      score:
        params.semanticWeight * cosineSimilarity(qv, value.vector) +
        params.bm25Weight * keywordScore(question, value.text),
    }))
    .sort((a, b) => b.score - a.score)
    .slice(0, topK || params.topK);
}

export async function searchDirSummaries({ repoKey, question, topN = 2 }) {
  const all = await getByPrefix(`dirsum:${repoKey}:`);
  if (!all.length) return [];
  const [qv] = await embedTexts([question]);
  const prefixLen = `dirsum:${repoKey}:`.length;
  return all
    .map(({ key, value }) => ({
      path: key.slice(prefixLen),
      summary: value.summary,
      score: cosineSimilarity(qv, value.vector),
    }))
    .sort((a, b) => b.score - a.score)
    .slice(0, topN);
}
