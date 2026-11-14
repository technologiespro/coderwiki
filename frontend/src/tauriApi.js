// Нативный мост Tauri: те же сигнатуры, что и в web api.js, но через invoke/events
const t = () => window.__TAURI__;
// invoke с нормализацией ошибок: Rust возвращает String, UI ждёт Error с .message
const invoke = async (cmd, args) => {
  try {
    return await t().core.invoke(cmd, args);
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
};
const listen = (ev, cb) => t().event.listen(ev, cb);

export const invokeRaw = (cmd, args) => invoke(cmd, args);

// Версия десктопного приложения (из Cargo.toml через нативную команду)
export async function appVersion() {
  return invoke('app_version');
}

export async function fetchModels(lmUrl) {
  return invoke('fetch_models', { lmUrl: lmUrl || null });
}

export async function startGeneration(payload) {
  return invoke('start_generate', { payload });
}

export async function startLocalGeneration(payload) {
  return invoke('start_generate_local', { payload });
}

export async function startLangGeneration(payload) {
  return invoke('start_generate_lang', { payload });
}

export function openProgress(jobId, onMessage) {
  let un;
  listen('gen-progress', (e) => onMessage(e.payload)).then((u) => (un = u));
  return { close: () => un && un() };
}

export async function fetchDocs(repo) {
  return invoke('get_docs', { repo });
}

export async function fetchRepos() {
  return invoke('list_repos');
}

export async function deleteRepoApi(repo) {
  return invoke('delete_repo', { repo });
}

export async function cancelGeneration() {
  return invoke('cancel_generate');
}

export async function createShare() {
  throw new Error('Ссылки «Поделиться» недоступны в desktop-версии');
}

export async function fetchShared() {
  throw new Error('Недоступно в desktop-версии');
}

export async function sendFeedback(repo, vote) {
  return invoke('send_feedback', { repo, vote });
}

export async function streamRegenerateSection(payload, onDelta) {
  const un = await listen('section-delta', (e) => onDelta(e.payload));
  try {
    await invoke('generate_section', { payload });
  } finally {
    un();
  }
}

export async function streamChat(payload, { onDelta, onSources }) {
  const u1 = await listen('chat-sources', (e) => onSources && onSources(e.payload));
  const u2 = await listen('chat-delta', (e) => onDelta(e.payload));
  try {
    await invoke('chat', { payload });
  } finally {
    u1();
    u2();
  }
}

export async function importXdpFile(file) {
  const buf = new Uint8Array(await file.arrayBuffer());
  let bin = '';
  const CH = 0x8000;
  for (let i = 0; i < buf.length; i += CH) {
    bin += String.fromCharCode.apply(null, buf.subarray(i, i + CH));
  }
  return invoke('import_xdp', { dataB64: btoa(bin) });
}

// Экспорт .xdp с прогрессом обучения словаря и системным диалогом сохранения
export async function exportXdpDesktop(repo, onLine) {
  const un = await listen('xdp-progress', (e) => onLine && onLine(e.payload));
  try {
    return await invoke('export_xdp', { repo });
  } finally {
    un();
  }
}

export async function exportZipDesktop(repo) {
  return invoke('export_zip', { repo });
}

// Выбор папки с кодом через системный диалог (только desktop)
export async function pickLocalFolder() {
  return invoke('pick_local_folder');
}

// Диалог выбора файла .xdp с импортом (Desktop)
export async function pickXdpFile() {
  return invoke('pick_xdp_file');
}

export async function importXdpPath(path) {
  return invoke('import_xdp_path', { path });
}

export async function readLocalFolder(path) {
  return invoke('read_local_folder', { path });
}

// Нативный drag-drop Tauri: HTML5 drop не срабатывает, пути приходят событиями
export function onNativeDrop({ onEnter, onLeave, onDrop }) {
  const uns = [];
  listen('tauri://drag-enter', () => onEnter && onEnter()).then((u) => uns.push(u));
  listen('tauri://drag-leave', () => onLeave && onLeave()).then((u) => uns.push(u));
  listen('tauri://drag-drop', (e) => onDrop && onDrop(e.payload?.paths || [])).then((u) => uns.push(u));
  return () => uns.forEach((u) => u());
}

