import { Router } from 'express';
import express from 'express';
import archiver from 'archiver';
import crypto from 'crypto';
import fs from 'fs/promises';
import path from 'path';
import { fetchModels, generateSectionDoc } from '../services/lmstudio.js';
import { createJob, createLocalJob, createLangJob, getJob, attachClient, snapshot } from '../services/pipeline.js';
import { answerQuestion } from '../services/chat.js';
import { resolveEmbeddingModel, embedFile, getRagParams, mutateRagParams } from '../services/embeddings.js';
import { buildXdp, importXdp } from '../services/packager.js';
import { getCache, setCache, delCache, delPrefix, listRepos, getByPrefix } from '../services/db.js';
import { parseRepoUrl } from '../services/git.js';
import { getMaxTokens, sliceByTokens } from '../utils/tokenManager.js';

const router = Router();

router.get('/', (req, res) => res.json({ message: 'CoderWiki backend is running' }));

router.get('/models', async (req, res) => {
  try {
    const models = await fetchModels(req.query.lmUrl);
    res.json({ models });
  } catch (err) {
    res.status(err.status || 502).json({ error: err.message });
  }
});

router.post('/generate', (req, res) => {
  const { repoUrl, language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens } = req.body || {};
  if (!repoUrl) return res.status(400).json({ error: 'repoUrl is required' });
  if (!modelId) return res.status(400).json({ error: 'modelId is required' });
  try {
    const { key } = parseRepoUrl(repoUrl);
    const job = createJob({ repoUrl, language: language || 'russian', modelId, lmUrl, embeddingModel, maxFiles, maxTokens, improvementIdeas: req.body.improvementIdeas, githubToken: req.body.githubToken });
    res.json({ jobId: job.id, repoKey: job.repoKey });
  } catch (err) {
    res.status(err.status || 500).json({ error: err.message });
  }
});

router.post('/generate-local', (req, res) => {
  const { name, language, modelId, lmUrl, embeddingModel, maxFiles, maxTokens, files } = req.body || {};
  if (!modelId) return res.status(400).json({ error: 'modelId is required' });
  if (!Array.isArray(files) || !files.length) {
    return res.status(400).json({ error: 'files array is required' });
  }
  const job = createLocalJob({ name, language: language || 'russian', modelId, lmUrl, embeddingModel, maxFiles, maxTokens, files, improvementIdeas: req.body.improvementIdeas });
  res.json({ jobId: job.id, repoKey: job.repoKey });
});

// Догенерация недостающей языковой версии из сохранённых исходников (кнопка «+» в wiki)
router.post('/generate-lang', async (req, res) => {
  const { repo, language, modelId, lmUrl, embeddingModel, maxTokens } = req.body || {};
  if (!repo || !language) return res.status(400).json({ error: 'repo and language are required' });
  if (!modelId) return res.status(400).json({ error: 'modelId is required' });
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Documentation not found for this repo' });
  const base = String(repo).split('@')[0];
  const prefix = `src:${base}:`;
  const entries = await getByPrefix(prefix);
  const files = entries.map(({ key, value }) => ({ path: key.slice(prefix.length), content: value }));
  if (!files.length) {
    return res.status(404).json({ error: 'Исходный код не найден в кэше — перегенерируйте репозиторий целиком' });
  }
  const job = createLangJob({ baseKey: base, repoUrl: record.repoUrl, language, modelId, lmUrl, embeddingModel, maxTokens, files, improvementIdeas: req.body.improvementIdeas });
  res.json({ jobId: job.id, repoKey: job.repoKey });
});

router.get('/progress', (req, res) => {
  const job = getJob(req.query.jobId);
  if (!job) return res.status(404).json({ error: 'Job not found' });
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
  });
  res.write(`data: ${JSON.stringify(snapshot(job))}\n\n`);
  attachClient(job, res);
  req.on('close', () => job.clients.delete(res));
});

router.post('/chat', async (req, res) => {
  const { repo, question, modelId, lmUrl, embeddingModel, history, mode, maxTokens } = req.body || {};
  if (!repo || !question || !modelId) {
    return res.status(400).json({ error: 'repo, question and modelId are required' });
  }
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Documentation not found for this repo' });
  let started = false;
  const start = () => {
    started = true;
    res.writeHead(200, {
      'Content-Type': 'text/plain; charset=utf-8',
      'Cache-Control': 'no-cache',
      'X-Accel-Buffering': 'no',
    });
  };
  try {
    const full = await answerQuestion({
      record,
      question,
      history: Array.isArray(history) ? history.slice(-6) : [],
      modelId,
      lmUrl,
      embeddingModel,
      mode,
      maxTokens,
      onSources: (payload) => {
        if (!started) start();
        res.write(`__SOURCES__${JSON.stringify(payload)}\n`);
      },
      onDelta: (delta) => {
        if (!started) start();
        res.write(delta);
      },
    });
    if (!started) {
      start();
      res.write(full);
    }
    res.end();
  } catch (err) {
    if (!started) res.status(err.status || 500).json({ error: err.message });
    else {
      res.write(`\n\n**Ошибка:** ${err.message}`);
      res.end();
    }
  }
});

router.post('/cancel', (req, res) => {
  const job = getJob(req.body?.jobId);
  if (!job) return res.status(404).json({ error: 'Job not found' });
  if (['done', 'error', 'cancelled'].includes(job.status)) return res.json({ ok: false });
  job.cancelled = true;
  job.abort.abort();
  res.json({ ok: true });
});

router.get('/download', async (req, res) => {
  const repo = req.query.repo;
  if (!repo) return res.status(400).json({ error: 'repo query param is required' });
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Documentation not found for this repo' });
  res.setHeader('Content-Type', 'application/zip');
  res.setHeader('Content-Disposition', `attachment; filename="${repo.replace('/', '__')}-docs.zip"`);
  const archive = archiver('zip', { zlib: { level: 9 } });
  archive.on('error', () => res.end());
  archive.pipe(res);
  if (record.overview) archive.append(record.overview, { name: '_OVERVIEW.md' });
  for (const s of record.sections || []) {
    archive.append(s.md, { name: `${s.id}.md` });
  }
  for (const [rel, mdText] of Object.entries(record.docs || {})) {
    archive.append(mdText, { name: `${rel}.md` });
  }
  archive.finalize();
});

router.post('/feedback', async (req, res) => {
  const { repo, vote } = req.body || {};
  if (!repo) return res.status(400).json({ error: 'repo is required' });
  let params = await getRagParams(repo);
  if (vote === 'down') params = await mutateRagParams(repo);
  res.json({ ok: true, params });
});

router.post('/import-xdp', express.raw({ type: 'application/octet-stream', limit: '200mb' }), async (req, res) => {
  if (!req.body || !req.body.length) return res.status(400).json({ error: 'Пустой файл' });
  try {
    const repoKeys = await importXdp(req.body);
    res.json({ ok: true, repoKey: repoKeys[0], repoKeys });
  } catch (err) {
    res.status(400).json({ error: `Не удалось импортировать .xdp: ${err.message}` });
  }
});

router.get('/export-xdp', async (req, res) => {
  const repo = req.query.repo;
  if (!repo) return res.status(400).json({ error: 'repo query param is required' });
  try {
    const buf = await buildXdp(repo);
    if (!buf) return res.status(404).json({ error: 'Documentation not found for this repo' });
    res.setHeader('Content-Type', 'application/octet-stream');
    res.setHeader('Content-Disposition', `attachment; filename="${repo.split('@')[0].replace('/', '__')}.xdp"`);
    res.end(buf);
  } catch (err) {
    res.status(500).json({ error: `Ошибка упаковки .xdp: ${err.message}` });
  }
});

// Джобы экспорта .xdp с прогрессом обучения словаря
const exportJobs = new Map();

router.post('/export-xdp-job', async (req, res) => {
  const { repo } = req.body || {};
  if (!repo) return res.status(400).json({ error: 'repo is required' });
  const jobId = crypto.randomUUID();
  const job = { repo, status: 'packing', gen: 0, gens: 0, lines: [], buf: null, error: null };
  exportJobs.set(jobId, job);
  setTimeout(() => exportJobs.delete(jobId), 10 * 60 * 1000).unref();
  buildXdp(repo, (line) => {
    const m = line.match(/^gen (\d+)\/(\d+)/);
    if (m) {
      job.gen = Number(m[1]);
      job.gens = Number(m[2]);
    }
    job.lines.push(line);
  })
    .then((buf) => {
      if (!buf) {
        job.status = 'error';
        job.error = 'Документация не найдена для этого репозитория';
      } else {
        job.buf = buf;
        job.status = 'done';
      }
    })
    .catch((e) => {
      job.status = 'error';
      job.error = e.message;
    });
  res.json({ jobId });
});

router.get('/export-xdp-progress', (req, res) => {
  const job = exportJobs.get(req.query.jobId);
  if (!job) return res.status(404).json({ error: 'job not found' });
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  res.flushHeaders();
  const send = () => {
    res.write(
      `data: ${JSON.stringify({ status: job.status, gen: job.gen, gens: job.gens, lines: job.lines.slice(-3), size: job.buf ? job.buf.length : 0, error: job.error })}\n\n`
    );
    if (job.status === 'done' || job.status === 'error') {
      clearInterval(timer);
      res.end();
    }
  };
  const timer = setInterval(send, 250);
  send();
  req.on('close', () => clearInterval(timer));
});

router.get('/export-xdp-file', (req, res) => {
  const job = exportJobs.get(req.query.jobId);
  if (!job || job.status !== 'done') return res.status(404).json({ error: 'Файл не готов' });
  res.setHeader('Content-Type', 'application/octet-stream');
  res.setHeader('Content-Disposition', `attachment; filename="${job.repo.split('@')[0].replace('/', '__')}.xdp"`);
  res.end(job.buf);
});

router.post('/share', async (req, res) => {
  const { repo } = req.body || {};
  if (!repo) return res.status(400).json({ error: 'repo is required' });
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Documentation not found for this repo' });
  let token = await getCache(`share:repo:${repo}`);
  if (!token) {
    token = crypto.randomUUID().replace(/-/g, '').slice(0, 16);
    await setCache(`share:repo:${repo}`, token);
    await setCache(`share:token:${token}`, repo);
  }
  res.json({ token });
});

router.get('/shared', async (req, res) => {
  const token = req.query.token;
  if (!token) return res.status(400).json({ error: 'token is required' });
  const repo = await getCache(`share:token:${token}`);
  if (!repo) return res.status(404).json({ error: 'Ссылка недействительна' });
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Документация была удалена' });
  res.json(record);
});

router.get('/docs', async (req, res) => {
  const repo = req.query.repo;
  if (!repo) return res.status(400).json({ error: 'repo query param is required' });
  const record = await getCache(`repo:${repo}`);
  if (!record) return res.status(404).json({ error: 'Documentation not found for this repo' });
  res.json(record);
});

router.get('/repos', async (req, res) => {
  res.json({ repos: await listRepos() });
});

router.delete('/repos', async (req, res) => {
  const repo = req.query.repo;
  if (!repo) return res.status(400).json({ error: 'repo query param is required' });
  await delCache(`repo:${repo}`);
  // src/emb общие для языковых версий — чистим только если других версий не осталось
  const base = String(repo).split('@')[0];
  const others = (await getByPrefix('repo:')).filter(({ key }) => key.slice(5).split('@')[0] === base);
  if (!others.length) {
    await delPrefix(`emb:${base}:`);
    await delPrefix(`src:${base}:`);
  }
  const docsDir = path.join(path.resolve(process.env.DOCS_DIR || './data/docs'), repo.replace('/', '__'));
  await fs.rm(docsDir, { recursive: true, force: true }).catch(() => {});
  res.json({ ok: true });
});

router.post('/generate-section', async (req, res) => {
  const { repo, sectionId, modelId, lmUrl, maxTokens } = req.body || {};
  if (!repo || !sectionId) return res.status(400).json({ error: 'repo and sectionId are required' });
  const record = await getCache(`repo:${repo}`);
  const section = record && record.sections ? record.sections.find((s) => s.id === sectionId) : null;
  if (!record || !section) return res.status(404).json({ error: 'Раздел не найден — перегенерируйте репозиторий' });
  const perFile = Math.max(400, Math.floor((getMaxTokens(maxTokens) - 2500) / Math.max(1, section.files.length)));
  let codeContext = '';
  for (const f of section.files) {
    const src = await getCache(`src:${repo.split('@')[0]}:${f}`);
    if (src !== null) codeContext += `### ${f}\n\`\`\`\n${sliceByTokens(src, perFile)}\n\`\`\`\n\n`;
  }
  if (!codeContext) {
    return res.status(404).json({ error: 'Исходный код раздела не найден в кэше — перегенерируйте репозиторий целиком' });
  }
  const model = modelId || record.modelId;
  let started = false;
  const start = () => {
    started = true;
    res.writeHead(200, {
      'Content-Type': 'text/plain; charset=utf-8',
      'Cache-Control': 'no-cache',
      'X-Accel-Buffering': 'no',
    });
  };
  try {
    const markdown = await generateSectionDoc({
      lmUrl,
      modelId: model,
      language: record.language,
      repoKey: repo,
      title: section.title,
      codeContext,
      onChunk: (full, delta) => {
        if (!started) start();
        res.write(delta);
      },
    });
    if (!started) {
      start();
      res.write(markdown);
    }
    section.md = markdown;
    await setCache(`repo:${repo}`, record);
    const outFile = path.join(
      path.resolve(process.env.DOCS_DIR || './data/docs'),
      repo.replace('/', '__'),
      `${section.id}.md`
    );
    await fs.mkdir(path.dirname(outFile), { recursive: true });
    await fs.writeFile(outFile, markdown, 'utf-8');
    res.end();
  } catch (err) {
    if (!started) res.status(err.status || 500).json({ error: err.message });
    else {
      res.write(`\n\n**Ошибка:** ${err.message}`);
      res.end();
    }
  }
});

// Готовый раздел во время генерации: читаем сохранённый .md с диска
router.get('/live-section', async (req, res) => {
  const { repo, id } = req.query;
  if (!repo || !id || String(id).includes('..') || String(repo).includes('..')) {
    return res.status(400).json({ error: 'repo and id are required' });
  }
  const f = path.join(
    path.resolve(process.env.DOCS_DIR || './data/docs'),
    String(repo).replace('/', '__'),
    `${id}.md`
  );
  try {
    res.json({ md: await fs.readFile(f, 'utf-8') });
  } catch {
    res.status(404).json({ error: 'Раздел ещё не готов' });
  }
});

export default router;
