<script setup>
import { ref, computed } from 'vue';
import { store } from '../store';
import { loadDocs, deleteRepo, loadRepos } from '../actions';
import { importXdpFile, isTauri, pickLocalFolder } from '../api';

const props = defineProps({
  onOpenUrl: Function,
  onGenerateLocal: Function,
});

const search = ref('');
const dragOver = ref(false);
const reading = ref(false);

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
  if (!q || q.includes('github.com')) return store.repos;
  return store.repos.filter((r) => r.repoKey.toLowerCase().includes(q));
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

async function onDrop(e) {
  dragOver.value = false;
  const entry = e.dataTransfer.items?.[0]?.webkitGetAsEntry?.();
  if (entry && entry.isFile && entry.name.toLowerCase().endsWith('.xdp')) {
    reading.value = true;
    try {
      const { repoKey } = await importXdpFile(e.dataTransfer.files[0]);
      await loadRepos();
      await loadDocs(repoKey);
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
</script>

<template>
  <div class="mx-auto max-w-5xl px-6 py-16 sm:py-24" data-testid="home-page">
    <h1 class="text-center text-3xl sm:text-4xl font-bold tracking-tight text-slate-800 dark:text-slate-200">
      Какой репозиторий вы хотите изучить?
    </h1>

    <div class="mt-8 mx-auto max-w-xl relative">
      <svg class="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-slate-500"
        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        v-model="search"
        data-testid="home-search-input"
        type="text"
        placeholder="Поиск по истории или вставьте ссылку на GitHub…"
        class="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-10 pr-4 py-3 text-sm placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200"
        @keyup.enter="submitSearch" />
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
        <div>
          <svg class="w-7 h-7 mb-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          <p class="font-semibold">Добавить репозиторий</p>
        </div>
        <p class="text-sm text-blue-50/90">
          {{ reading ? 'Загрузка…' : dragOver ? 'Отпустите, чтобы загрузить' : isTauri ? 'Нажмите, чтобы выбрать папку с кодом, или перетащите папку / файл .xdp' : 'Вставьте ссылку на GitHub выше, перетащите папку с кодом или файл .xdp' }}
        </p>
      </div>

      <div
        v-for="r in filteredRepos"
        :key="r.repoKey"
        :data-testid="`home-repo-card-${r.repoKey}`"
        class="group relative rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-800/50 p-5 min-h-[150px] flex flex-col justify-between cursor-pointer hover:border-blue-400 dark:hover:border-blue-500 transition-colors duration-200"
        @click="openRepo(r.repoKey)">
        <div class="min-w-0">
          <p class="font-semibold text-slate-800 dark:text-slate-200 truncate pr-6">{{ r.repoKey }}</p>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">
            {{ r.fileCount }} файлов · {{ r.language === 'russian' ? 'русский' : 'английский' }}
          </p>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-xs text-slate-400 dark:text-slate-500">
            {{ new Date(r.generatedAt).toLocaleDateString() }}
          </span>
          <svg class="w-4 h-4 text-slate-400 dark:text-slate-500 group-hover:text-blue-600 dark:group-hover:text-blue-400 group-hover:translate-x-0.5 transition-all duration-200"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
          </svg>
        </div>
        <button
          :data-testid="`home-repo-delete-${r.repoKey}`"
          title="Удалить"
          class="absolute top-3 right-3 p-1 rounded text-slate-400 dark:text-slate-500 opacity-0 group-hover:opacity-100 hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          @click.stop="removeRepo(r.repoKey)">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>

    <p v-if="!store.repos.length" class="mt-10 text-center text-sm text-slate-400 dark:text-slate-500">
      Здесь появятся сгенерированные wiki — добавьте первый репозиторий
    </p>
  </div>
</template>
