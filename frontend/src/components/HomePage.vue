<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { store } from '../store';
import { t, DOC_LANGUAGES, docLangLabel, docLangShort } from '../i18n';
import { loadDocs, deleteRepo, loadRepos } from '../actions';
import { importXdpFile, isTauri, pickLocalFolder, pickXdpFile, importXdpPath, readLocalFolder, onNativeDrop } from '../api';

// eslint-disable-next-line no-undef -- макрос компилятора Vue
const props = defineProps({
  onOpenUrl: Function,
  onGenerateLocal: Function,
});

const search = ref('');
const docLangFilter = ref('');
const dragOver = ref(false);
const reading = ref(false);
// Языковые версии из импортированного мультиязычного .xdp — диалог выбора, что открыть
const importedKeys = ref(null);

function importedLang(k) {
  const r = store.repos.find((x) => x.repoKey === k);
  return r ? r.language : k.split('@')[1] || '';
}

async function openImported(k) {
  importedKeys.value = null;
  try {
    await loadDocs(k);
  } catch (e) {
    store.error = e.message;
  }
}

const IGNORE_DIRS = new Set([
  'node_modules', '.git', 'dist', 'build', 'out', '.next', '.nuxt', 'coverage',
  'vendor', '__pycache__', '.venv', 'venv', 'target', '.idea', '.vscode', '.cache', 'tmp',
]);
const IGNORE_FILES = new Set(['yarn.lock', 'package-lock.json', 'pnpm-lock.yaml', '.ds_store']);
const ALLOWED_EXT = new Set([
  '.js', '.jsx', '.ts', '.tsx', '.vue', '.svelte', '.mjs', '.cjs',
  '.py', '.go', '.rs', '.java', '.kt', '.c', '.cpp', '.h', '.hpp', '.cs',
  '.rb', '.php', '.swift', '.scala', '.sh', '.bash', '.sql',
  '.html', '.css', '.scss', '.less', '.json', '.yml', '.yaml', '.toml',
  '.md', '.txt', '.xml', '.ini', '.cfg',
]);

const filteredRepos = computed(() => {
  const q = search.value.trim().toLowerCase();
  let list = store.repos;
  if (docLangFilter.value) list = list.filter((r) => r.language === docLangFilter.value);
  if (!q || q.includes('github.com')) return list;
  return list.filter((r) => r.repoKey.toLowerCase().includes(q));
});

function submitSearch() {
  const v = search.value.trim();
  if (v.includes('github.com')) {
    props.onOpenUrl && props.onOpenUrl(v);
  }
}

async function openRepo(repoKey) {
  try {
    await loadDocs(repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

async function removeRepo(repoKey) {
  try {
    await deleteRepo(repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

function readEntries(reader) {
  return new Promise((resolve, reject) => reader.readEntries(resolve, reject));
}

async function collectFromEntry(entry, prefix, out) {
  if (out.length >= 400) return;
  if (entry.isFile) {
    const name = entry.name.toLowerCase();
    if (IGNORE_FILES.has(name)) return;
    const dot = name.lastIndexOf('.');
    const ext = dot === -1 ? '' : name.slice(dot);
    if (!ALLOWED_EXT.has(ext) && name !== 'dockerfile' && name !== 'makefile') return;
    const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
    if (!file.size || file.size > 300 * 1024) return;
    out.push({ path: prefix + entry.name, content: await file.text() });
  } else if (entry.isDirectory) {
    if (IGNORE_DIRS.has(entry.name.toLowerCase())) return;
    const reader = entry.createReader();
    let batch;
    do {
      batch = await readEntries(reader);
      for (const e of batch) await collectFromEntry(e, `${prefix}${entry.name}/`, out);
    } while (batch.length);
  }
}

// После импорта .xdp: обновить список и открыть документацию (или предложить выбор языка)
async function afterXdpImport({ repoKey, repoKeys, path }) {
  if (path) rememberXdp(path);
  await loadRepos();
  const keys = Array.isArray(repoKeys) && repoKeys.length ? repoKeys : [repoKey];
  if (keys.length > 1) {
    importedKeys.value = keys;
  } else {
    await loadDocs(keys[0]);
  }
}

// Desktop: недавно открытые .xdp (пути на диске), максимум 5
const RECENT_KEY = 'recentXdp';
function loadRecent() {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]');
    return Array.isArray(v) ? v.filter((x) => typeof x === 'string') : [];
  } catch {
    return [];
  }
}
const recentXdp = ref(isTauri ? loadRecent() : []);
function saveRecent(list) {
  recentXdp.value = list;
  localStorage.setItem(RECENT_KEY, JSON.stringify(list));
}
function rememberXdp(path) {
  saveRecent([path, ...recentXdp.value.filter((p) => p !== path)].slice(0, 5));
}
function forgetXdp(path) {
  saveRecent(recentXdp.value.filter((p) => p !== path));
}
const xdpName = (p) => p.split(/[\\/]/).pop();
async function openRecent(path) {
  if (reading.value) return;
  reading.value = true;
  try {
    await afterXdpImport(await importXdpPath(path));
  } catch (err) {
    // Файл переехал или удалён — убираем из списка
    if (/Не удалось прочитать файл/.test(err.message)) forgetXdp(path);
    store.error = err.message;
  } finally {
    reading.value = false;
  }
}

async function onDrop(e) {
  dragOver.value = false;
  const entry = e.dataTransfer.items?.[0]?.webkitGetAsEntry?.();
  if (entry && entry.isFile && entry.name.toLowerCase().endsWith('.xdp')) {
    reading.value = true;
    try {
      await afterXdpImport(await importXdpFile(e.dataTransfer.files[0]));
    } catch (err) {
      store.error = err.message;
    } finally {
      reading.value = false;
    }
    return;
  }
  if (!entry || !entry.isDirectory) {
    store.error = 'Перетащите папку с исходным кодом или файл .xdp';
    return;
  }
  reading.value = true;
  try {
    const out = [];
    const reader = entry.createReader();
    let batch;
    do {
      batch = await readEntries(reader);
      for (const child of batch) await collectFromEntry(child, '', out);
    } while (batch.length);
    if (!out.length) {
      store.error = 'В папке не найдено подходящих исходных файлов';
      return;
    }
    props.onGenerateLocal && (await props.onGenerateLocal(entry.name, out));
  } catch (err) {
    store.error = `Не удалось прочитать папку: ${err.message}`;
  } finally {
    reading.value = false;
  }
}
// Desktop: клик по блоку добавления открывает системный диалог выбора папки
async function onZoneClick() {
  if (!isTauri || reading.value) return;
  reading.value = true;
  try {
    const res = await pickLocalFolder();
    if (!res) return;
    if (!res.files || !res.files.length) {
      store.error = 'В папке не найдено подходящих исходных файлов';
      return;
    }
    props.onGenerateLocal && (await props.onGenerateLocal(res.name, res.files));
  } catch (err) {
    store.error = String(err);
  } finally {
    reading.value = false;
  }
}

// Desktop: отдельная кнопка — системный диалог выбора файла .xdp
async function onPickXdp() {
  if (!isTauri || reading.value) return;
  reading.value = true;
  try {
    const res = await pickXdpFile();
    if (res) await afterXdpImport(res);
  } catch (err) {
    store.error = err.message;
  } finally {
    reading.value = false;
  }
}

// Desktop: нативный drag-drop Tauri отдаёт пути на диске вместо File
async function onNativePaths(paths) {
  dragOver.value = false;
  if (!paths.length || reading.value) return;
  const p = paths[0];
  reading.value = true;
  try {
    if (p.toLowerCase().endsWith('.xdp')) {
      await afterXdpImport(await importXdpPath(p));
      return;
    }
    const res = await readLocalFolder(p);
    if (!res.files || !res.files.length) {
      store.error = 'В папке не найдено подходящих исходных файлов';
      return;
    }
    props.onGenerateLocal && (await props.onGenerateLocal(res.name, res.files));
  } catch (err) {
    store.error = err.message;
  } finally {
    reading.value = false;
  }
}

let unNative = null;
onMounted(() => {
  if (!isTauri) return;
  unNative = onNativeDrop({
    onEnter: () => (dragOver.value = true),
    onLeave: () => (dragOver.value = false),
    onDrop: onNativePaths,
  });
});
onUnmounted(() => unNative && unNative());
</script>

<template>
  <div class="mx-auto max-w-5xl px-6 py-16 sm:py-24" data-testid="home-page">
    <h1 class="text-center text-3xl sm:text-4xl font-bold tracking-tight text-slate-800 dark:text-slate-200">
      {{ t('homeTitle') }}
    </h1>

    <div class="mt-8 mx-auto max-w-2xl flex items-center gap-3">
      <div class="relative flex-1">
        <svg class="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500"
          viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          v-model="search"
          data-testid="home-search-input"
          type="text"
          :placeholder="t('homeSearchPlaceholder')"
          class="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-10 pr-4 py-3 text-sm placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200"
          @keyup.enter="submitSearch" />
      </div>
      <select
        v-model="docLangFilter"
        data-testid="docs-lang-filter"
        class="shrink-0 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-3 text-sm text-slate-600 dark:text-slate-300 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200">
        <option value="">{{ t('allLanguages') }}</option>
        <option v-for="l in DOC_LANGUAGES" :key="l.code" :value="l.code">{{ l.label }}</option>
      </select>
    </div>

    <div v-if="recentXdp.length" class="mt-8 mx-auto max-w-5xl" data-testid="recent-xdp">
      <p class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">{{ t('recentXdp') }}</p>
      <div class="flex flex-wrap gap-2">
        <div
          v-for="p in recentXdp"
          :key="p"
          class="group inline-flex items-center gap-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-3 pr-1 py-1 text-sm hover:border-blue-400 dark:hover:border-blue-500 transition-colors duration-200">
          <button
            :data-testid="`recent-xdp-${xdpName(p)}`"
            :title="p"
            :disabled="reading"
            class="flex items-center gap-1.5 text-slate-700 dark:text-slate-300 hover:text-blue-600 dark:hover:text-blue-400 disabled:opacity-50 transition-colors duration-200"
            @click="openRecent(p)">
            <svg class="w-3.5 h-3.5 text-slate-400 dark:text-slate-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 8v13H3V8" /><path d="M1 3h22v5H1z" /><path d="M10 12h4" />
            </svg>
            <span class="truncate max-w-[14rem]">{{ xdpName(p) }}</span>
          </button>
          <button
            :data-testid="`recent-xdp-remove-${xdpName(p)}`"
            :title="t('recentXdpRemove')"
            class="p-1 rounded text-slate-400 dark:text-slate-500 opacity-0 group-hover:opacity-100 hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
            @click.stop="forgetXdp(p)">
            <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <div class="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      <div
        data-testid="add-repo-dropzone"
        class="relative rounded-xl p-5 min-h-[150px] flex flex-col justify-between cursor-default text-white bg-gradient-to-br from-blue-600 to-sky-500 dark:from-blue-600 dark:to-sky-600 transition-all duration-200"
        :class="[dragOver ? 'ring-4 ring-blue-400/60 scale-[1.02]' : '', isTauri ? 'cursor-pointer hover:brightness-110' : '']"
        @dragover.prevent="dragOver = true"
        @dragleave.prevent="dragOver = false"
        @drop.prevent="onDrop"
        @click="onZoneClick">
        <button
          v-if="isTauri"
          type="button"
          data-testid="open-xdp-btn"
          class="absolute top-3 right-3 rounded-md bg-white/15 hover:bg-white/30 px-2.5 py-1 text-xs font-medium text-white transition-colors duration-200 disabled:opacity-50"
          :disabled="reading"
          :title="t('openXdpHint')"
          @click.stop="onPickXdp">
          {{ t('openXdp') }}
        </button>
        <div>
          <svg class="w-7 h-7 mb-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          <p class="font-semibold">{{ t('addRepo') }}</p>
        </div>
        <p class="text-sm text-blue-50/90">
          {{ reading ? t('loading') : dragOver ? t('dropRelease') : isTauri ? t('dropHintDesktop') : t('dropHintWeb') }}
        </p>
      </div>

      <div
        v-for="r in filteredRepos"
        :key="r.repoKey"
        :data-testid="`home-repo-card-${r.repoKey}`"
        class="group relative rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-800/50 p-5 min-h-[150px] flex flex-col justify-between cursor-pointer hover:border-blue-400 dark:hover:border-blue-500 transition-colors duration-200"
        @click="openRepo(r.repoKey)">
        <div class="min-w-0">
          <p class="font-semibold text-slate-800 dark:text-slate-200 truncate pr-6">{{ r.repoKey.split('@')[0] }}</p>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">
            {{ r.fileCount }} {{ t('filesWord') }} · {{ docLangLabel(r.language) }}
          </p>
        </div>
        <div class="flex items-center justify-between">
          <span class="flex items-center gap-2">
            <span
              :data-testid="`repo-lang-badge-${r.repoKey}`"
              :title="docLangLabel(r.language)"
              class="rounded-md bg-blue-50 dark:bg-blue-900/40 border border-blue-200 dark:border-blue-800 px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-blue-600 dark:text-blue-400">
              {{ docLangShort(r.language) }}
            </span>
            <span class="text-xs text-slate-400 dark:text-slate-500">
              {{ new Date(r.generatedAt).toLocaleDateString() }}
            </span>
          </span>
          <svg class="w-4 h-4 text-slate-400 dark:text-slate-500 group-hover:text-blue-600 dark:group-hover:text-blue-400 group-hover:translate-x-0.5 transition-all duration-200"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
          </svg>
        </div>
        <button
          :data-testid="`home-repo-delete-${r.repoKey}`"
          :title="t('deleteTooltip')"
          class="absolute top-3 right-3 p-1 rounded text-slate-400 dark:text-slate-500 opacity-0 group-hover:opacity-100 hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          @click.stop="removeRepo(r.repoKey)">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>

    <p v-if="!store.repos.length" class="mt-10 text-center text-sm text-slate-400 dark:text-slate-500">
      {{ t('emptyHistory') }}
    </p>

    <div
      v-if="importedKeys"
      data-testid="xdp-pick-modal"
      class="fixed inset-0 z-[90] bg-black/50 backdrop-blur-sm flex items-center justify-center p-4"
      @click.self="importedKeys = null">
      <div class="w-full max-w-md rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shadow-2xl p-6">
        <h2 class="text-lg font-semibold text-slate-800 dark:text-slate-200">{{ t('xdpPickTitle') }}</h2>
        <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">{{ t('xdpPickHint') }}</p>
        <div class="mt-4 flex flex-col gap-2 max-h-[50vh] overflow-y-auto pr-1">
          <button
            v-for="k in importedKeys"
            :key="k"
            :data-testid="`xdp-pick-${k}`"
            class="flex items-center justify-between gap-3 rounded-md border border-slate-200 dark:border-slate-600 px-3 py-2 text-sm text-left text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-700 dark:hover:text-blue-400 transition-colors duration-200"
            @click="openImported(k)">
            <span class="truncate">{{ docLangLabel(importedLang(k)) }}</span>
            <span class="shrink-0 rounded-md bg-blue-50 dark:bg-blue-900/40 border border-blue-200 dark:border-blue-800 px-1.5 py-0.5 text-[10px] font-semibold text-blue-600 dark:text-blue-400">
              {{ docLangShort(importedLang(k)) }}
            </span>
          </button>
        </div>
        <button
          data-testid="xdp-pick-cancel"
          class="mt-4 w-full rounded-md border border-slate-200 dark:border-slate-600 px-3 py-2 text-sm text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors duration-200"
          @click="importedKeys = null">
          {{ t('cancel') }}
        </button>
      </div>
    </div>
  </div>
</template>
