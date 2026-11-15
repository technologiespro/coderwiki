import { chatStream } from './lmstudio.js';
import { searchChunks, searchDirSummaries } from './embeddings.js';
import { getCache } from './db.js';
import { estimateTokens, sliceByTokens, slidingHistory, getMaxTokens } from '../utils/tokenManager.js';

export function selectContext(record, question) {
  const words = [
    ...new Set(
      question
        .toLowerCase()
        .split(/[^a-zа-яё0-9_.]+/i)
        .filter((w) => w.length > 2)
    ),
  ];
  // Скоринг по разделам wiki (или legacy per-file докам)
  const entries = record.sections
    ? record.sections.map((s) => ({ name: s.title, text: `${s.title}\n${s.files.join('\n')}\n${s.md}`, body: s.md, files: s.files }))
    : Object.entries(record.docs || {}).map(([p, d]) => ({ name: p, text: `${p}\n${d}`, body: d, files: [p] }));
  const scored = entries
    .map((e) => {
      const text = e.text.toLowerCase();
      let score = 0;
      for (const w of words) {
        score += text.split(w).length - 1;
        if (e.name.toLowerCase().includes(w)) score += 5;
      }
      return { ...e, score };
    })
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 4);

  let context = '';
  if (record.overview) context += `## Repository overview\n${record.overview.slice(0, 3000)}\n\n`;
  for (const s of scored) context += `## ${s.name}\n${s.body.slice(0, 2500)}\n\n`;
  if (!scored.length) {
    context += `## Files in repository\n${(record.files || Object.keys(record.docs || {})).join('\n')}\n`;
  }
  const sources = [...new Set(scored.flatMap((s) => s.files.slice(0, 3)))].slice(0, 6);
  return { context: context.slice(0, 16000), sources };
}

async function buildVectorContext(record, question, lmUrl, embeddingModel, maxT) {
  const hits = await searchChunks({ repoKey: record.repoKey.split('@')[0], question, lmUrl, embeddingModel, topK: 12 });
  if (!hits || !hits.length) return null;
  const budget = maxT - 3500;
  let context = '';
  let truncated = false;
  if (record.overview) context += `## Repository overview\n${record.overview.slice(0, 1500)}\n\n`;
  context += 'Context from codebase:\n\n';
  const used = [];
  for (const h of hits) {
    const block = `File: ${h.filePath} (lines ${h.startLine}-${h.endLine})\nCode:\n\`\`\`\n${h.text}\n\`\`\`\n\n`;
    if (estimateTokens(context) + estimateTokens(block) > budget) {
      truncated = true;
      break;
    }
    context += block;
    used.push(h);
  }
  const seen = new Set();
  const sources = [];
  for (const h of used) {
    const key = `${h.filePath}#${h.startLine}-${h.endLine}`;
    if (seen.has(key)) continue;
    seen.add(key);
    sources.push({ path: h.filePath, lines: [h.startLine, h.endLine] });
  }
  return { context, sources, truncated };
}

function treeText(record, expandDirs = []) {
  const files = record.files || Object.keys(record.docs || {});
  const summaries = record.dirSummaries || {};
  const lines = [`Репозиторий ${record.repoKey}, всего файлов: ${files.length}`];
  const rootFiles = files.filter((f) => !f.includes('/'));
  for (const f of rootFiles) lines.push(`- ${f}`);
  const level1 = [...new Set(files.filter((f) => f.includes('/')).map((f) => f.split('/')[0]))];
  let truncated = false;
  for (const d of level1) {
    const members = files.filter((f) => f.startsWith(`${d}/`));
    if (expandDirs.some((e) => e === d || e.startsWith(`${d}/`))) {
      lines.push(`- /${d}: ${summaries[d] || ''}`);
      for (const f of members) lines.push(`  - ${f}`);
    } else if (members.length) {
      truncated = true;
      lines.push(`- /${d}: ${summaries[d] || ''} (directory truncated, ${members.length} files inside)`);
    }
  }
  return { text: sliceByTokens(lines.join('\n'), 2500), truncated };
}

const PAGE_TOKENS = 3000;

function pageOf(doc, page, path) {
  const per = PAGE_TOKENS * 4;
  const total = Math.max(1, Math.ceil(doc.length / per));
  const p = Math.min(Math.max(1, page || 1), total);
  const text = doc.slice((p - 1) * per, p * per);
  const hint =
    p < total
      ? `\n\n[System: Showing part ${p}/${total} of the file. To continue reading, reply EXACTLY with {"action":"READ_FILE_CHUNK","path":"${path}","page":${p + 1}}]`
      : `\n\n[System: Part ${p}/${total} — end of file.]`;
  return text + hint;
}

// Инструменты агента читают ИСХОДНЫЙ КОД из LevelDB (src:), а не доки
async function execTool(record, action, rawPath, page) {
  const p = String(rawPath).replace(/^\/+/, '');
  const allFiles = record.files || Object.keys(record.docs || {});
  if (action === 'READ_FILE' || action === 'READ_FILE_CHUNK') {
    let doc = await getCache(`src:${record.repoKey.split('@')[0]}:${p}`);
    if (doc === null && record.docs) doc = record.docs[p] ?? null;
    if (doc === null || doc === undefined) {
      return `Файл не найден: ${p}. Доступные файлы:\n${allFiles.slice(0, 40).join('\n')}`;
    }
    if (action === 'READ_FILE_CHUNK') return pageOf(doc, page, p);
    if (estimateTokens(doc) > PAGE_TOKENS) return pageOf(doc, 1, p);
    return doc;
  }
  const members = allFiles.filter((f) => f === p || f.startsWith(`${p}/`));
  if (!members.length) return `Папка не найдена: ${p}`;
  const lines = [];
  for (const f of members.slice(0, 60)) {
    const src = await getCache(`src:${record.repoKey.split('@')[0]}:${f}`);
    lines.push(`- ${f}: ${(((src || '').split('\n').find((l) => l.trim())) || '').slice(0, 80)}`);
  }
  return sliceByTokens(lines.join('\n'), 2500);
}

const TOOLS_NOTE =
  '\n\nTools available: if the provided context is NOT enough, you may request more data. To see a directory, reply EXACTLY and ONLY with {"action":"READ_DIR","path":"dir/path"}. To see full documentation of one file, reply EXACTLY and ONLY with {"action":"READ_FILE","path":"file/path"}. If a file is shown partially, read the next page with {"action":"READ_FILE_CHUNK","path":"file/path","page":2}. Otherwise answer normally.';

export async function answerQuestion({ record, question, history, modelId, lmUrl, embeddingModel, mode, maxTokens, onDelta, onSources }) {
  const maxT = getMaxTokens(maxTokens);
  let context = '';
  let sources = [];
  let searchMode = 'basic';
  let truncated = false;

  const { history: hist, truncated: hTrunc } = slidingHistory(history || [], 1500);
  truncated = hTrunc;

  let system;
  if (mode === 'arch') {
    let expand = [];
    try {
      expand = (await searchDirSummaries({ repoKey: record.repoKey.split('@')[0], question, topN: 2 })).map((d) => d.path);
    } catch {
      /* без семантического роутинга */
    }
    const t = treeText(record, expand);
    truncated = truncated || t.truncated;
    context = `Project structure:\n${t.text}\n\n${record.overview ? `Overview:\n${record.overview.slice(0, 2000)}` : ''}`;
    sources = expand.map((p) => ({ path: p, lines: null }));
    searchMode = 'arch';
    system = `You are a senior software architect analyzing the "${record.repoKey}" repository. Answer with high-level system design patterns, module responsibilities and data flow, NOT specific lines of code. Use the provided project structure. Reference paths in backticks. Answer in Markdown, strictly in ${record.language}.${TOOLS_NOTE}`;
  } else {
    try {
      const vec = await buildVectorContext(record, question, lmUrl, embeddingModel, maxT);
      if (vec) {
        context = vec.context;
        sources = vec.sources;
        truncated = truncated || vec.truncated;
        searchMode = 'vector';
      }
    } catch {
      /* fallback */
    }
    if (searchMode === 'basic') {
      const r = selectContext(record, question);
      context = sliceByTokens(r.context, maxT - 3500);
      sources = r.sources.map((p) => ({ path: p, lines: null }));
    }
    system = `You are a helpful expert assistant for the "${record.repoKey}" repository. Answer the user's questions using ONLY the provided documentation context. Reference relevant file paths in backticks. If the answer is not in the context, say so honestly. Answer in Markdown, strictly in ${record.language}.${TOOLS_NOTE}`;
  }

  onSources && onSources({ sources, mode: searchMode, truncated });

  const messages = [
    { role: 'system', content: system },
    ...hist,
    { role: 'user', content: `Context:\n\n${context}\n\nQuestion: ${question}` },
  ];

  let full = '';
  for (let iter = 0; iter < 5; iter++) {
    let buffered = '';
    let streamState = 'unknown';
    full = await chatStream({
      lmUrl,
      modelId,
      messages,
      onChunk: (f, delta) => {
        if (streamState === 'silent') return;
        if (streamState === 'stream') return onDelta(delta);
        buffered += delta;
        const t = buffered.trimStart();
        if (t.startsWith('{')) {
          streamState = 'silent';
          return;
        }
        if (t.length < 2) return;
        streamState = 'stream';
        onDelta(buffered);
      },
    });
    const tool = full.match(
      /\{\s*"action"\s*:\s*"(READ_DIR|READ_FILE|READ_FILE_CHUNK)"\s*,\s*"path"\s*:\s*"([^"]+)"(?:\s*,\s*"page"\s*:\s*(\d+))?\s*\}/
    );
    if (!tool || iter === 4) {
      if (streamState !== 'stream') onDelta(full);
      return full;
    }
    messages.push({ role: 'assistant', content: tool[0] });
    const toolResult = await execTool(record, tool[1], tool[2], parseInt(tool[3], 10));
    messages.push({
      role: 'user',
      content: `Tool response (${tool[1]} ${tool[2]}${tool[3] ? ` page ${tool[3]}` : ''}):\n${toolResult}\n\nNow answer the original question.`,
    });
  }
  return full;
}
