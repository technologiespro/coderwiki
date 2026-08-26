import * as native from './tauriApi';

// Desktop-режим: приложение собрано как Tauri (без Node-сервера)
export const isTauri = typeof window !== 'undefined' && !!window.__TAURI__;

const BACKEND = import.meta.env.VITE_BACKEND_URL;
export const API = `${BACKEND}/api`;

async function handle(res) {
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `Ошибка ${res.status}`);
  return data;
}

export async function fetchModels(lmUrl) {
  if (isTauri) return native.fetchModels(lmUrl);
  const q = lmUrl ? `?lmUrl=${encodeURIComponent(lmUrl)}` : '';
  return handle(await fetch(`${API}/models${q}`));
}

export async function startGeneration(payload) {
  if (isTauri) return native.startGeneration(payload);
  return handle(
    await fetch(`${API}/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
  );
}

export async function startLocalGeneration(payload) {
  if (isTauri) return native.startLocalGeneration(payload);
  return handle(
    await fetch(`${API}/generate-local`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
  );
}

export function openProgress(jobId, onMessage, onError) {
  if (isTauri) return native.openProgress(jobId, onMessage);
  const es = new EventSource(`${API}/progress?jobId=${jobId}`);
  es.onmessage = (e) => onMessage(JSON.parse(e.data));
  es.onerror = () => {
    es.close();
    onError && onError();
  };
  return es;
}

export async function fetchDocs(repoKey) {
  if (isTauri) return native.fetchDocs(repoKey);
  return handle(await fetch(`${API}/docs?repo=${encodeURIComponent(repoKey)}`));
}

export async function fetchRepos() {
  if (isTauri) return native.fetchRepos();
  return handle(await fetch(`${API}/repos`));
}

export async function deleteRepoApi(repoKey) {
  if (isTauri) return native.deleteRepoApi(repoKey);
  return handle(await fetch(`${API}/repos?repo=${encodeURIComponent(repoKey)}`, { method: 'DELETE' }));
}

export async function createShare(repoKey) {
  if (isTauri) return native.createShare(repoKey);
  return handle(
    await fetch(`${API}/share`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ repo: repoKey }),
    })
  );
}

export async function fetchShared(token) {
  if (isTauri) return native.fetchShared(token);
  return handle(await fetch(`${API}/shared?token=${encodeURIComponent(token)}`));
}

export async function cancelGeneration(jobId) {
  if (isTauri) return native.cancelGeneration(jobId);
  return handle(
    await fetch(`${API}/cancel`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jobId }),
    })
  );
}

export async function streamRegenerateSection(payload, onDelta) {
  if (isTauri) return native.streamRegenerateSection(payload, onDelta);
  const res = await fetch(`${API}/generate-section`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data.error || `Ошибка ${res.status}`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    onDelta(decoder.decode(value, { stream: true }));
  }
}

export async function importXdpFile(file) {
  if (isTauri) return native.importXdpFile(file);
  const res = await fetch(`${API}/import-xdp`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/octet-stream' },
    body: file,
  });
  return handle(res);
}

export async function sendFeedback(repoKey, vote) {
  if (isTauri) return native.sendFeedback(repoKey, vote);  return handle(
    await fetch(`${API}/feedback`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ repo: repoKey, vote }),
    })
  );
}

export async function streamChat(payload, { onDelta, onSources }) {
  if (isTauri) return native.streamChat(payload, { onDelta, onSources });
  const res = await fetch(`${API}/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data.error || `Ошибка ${res.status}`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const MARK = '__SOURCES__';
  let buf = '';
  let headerDone = false;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    const chunk = decoder.decode(value, { stream: true });
    if (headerDone) {
      onDelta(chunk);
      continue;
    }
    buf += chunk;
    if (buf.length < MARK.length && MARK.startsWith(buf)) continue;
    if (buf.startsWith(MARK)) {
      const nl = buf.indexOf('\n');
      if (nl === -1) continue;
      try {
        onSources && onSources(JSON.parse(buf.slice(MARK.length, nl)));
      } catch {
        /* некорректный заголовок источников */
      }
      headerDone = true;
      const rest = buf.slice(nl + 1);
      if (rest) onDelta(rest);
    } else {
      headerDone = true;
      onDelta(buf);
    }
  }
  if (!headerDone && buf) onDelta(buf);
}

// Desktop-only: сохранение через системные диалоги
export const exportXdpDesktop = native.exportXdpDesktop;
export const exportZipDesktop = native.exportZipDesktop;
export const pickLocalFolder = native.pickLocalFolder;
