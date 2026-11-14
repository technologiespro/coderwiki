<script setup>
import { onMounted, computed, provide, watch, ref } from 'vue';
import { store, applyTheme } from './store';
import { fetchModels, startGeneration, startLocalGeneration, startLangGeneration, openProgress, fetchShared } from './api';
import { loadDocs, loadRepos } from './actions';
import AppHeader from './components/AppHeader.vue';
import ProgressBar from './components/ProgressBar.vue';
import FileTree from './components/FileTree.vue';
import MarkdownViewer from './components/MarkdownViewer.vue';
import ChatPanel from './components/ChatPanel.vue';
import HomePage from './components/HomePage.vue';
import { t, DOC_LANGUAGES } from './i18n';

const showWorkspace = computed(
  () =>
    !!store.docs ||
    ['queued', 'cloning', 'parsing', 'sectioning', 'generating', 'embedding', 'overview'].includes(store.progress.status)
);

async function loadModels() {
  store.modelsError = '';
  store.lmStatus = 'checking';
  store.lmStatusText = 'Проверяем связь с LM Studio…';
  const started = Date.now();
  try {
    const { models } = await fetchModels(store.lmUrl);
    store.models = models;
    if (models.length && !models.find((m) => m.id === store.selectedModel)) {
      store.selectedModel = models[0].id;
    }
    if (!models.length) {
      store.lmStatus = 'warn';
      store.lmStatusText = t('lmStatusWarn');
    } else {
      store.lmStatus = 'ok';
      store.lmStatusText = `${t('lmStatusOk')} (${Date.now() - started} мс) · ${t('connModels')}: ${models.length}`;
    }
  } catch (e) {
    store.models = [];
    store.modelsError = e.message;
    store.lmStatus = 'error';
    store.lmStatusText = `${t('lmStatusError')}: ${e.message}`;
  }
}

function trackJob(jobId, repoKey) {
  store.progress = { status: 'queued', currentFile: '', processed: 0, total: 0, error: '', cachedHits: 0, partial: '' };
  openProgress(jobId, async (p) => {
    store.progress = p;
    if (p.status === 'done') {
      try {
        const keepOpen = store.liveSection && store.liveSection.id;
        store.liveSection = null;
        // repoKey берём из события прогресса: Tauri start_generate не возвращает его в ответе команды
        const doneKey = p.repoKey || repoKey;
        if (doneKey) {
          await loadDocs(doneKey);
        }
        if (keepOpen && store.docs && store.docs.sections && store.docs.sections.some((s) => s.id === keepOpen)) {
          store.selectedSection = keepOpen;
        }
        if (p.status === 'done') {
          // Скрыть панель прогресса через пару секунд после завершения
          setTimeout(() => {
            if (store.progress.status === 'done') store.progress = { ...store.progress, status: 'idle' };
          }, 2500);
        }
        loadRepos();
      } catch (e) {
        store.error = e.message;
      }
    }
    if (p.status === 'error') store.error = p.error;
    if (p.status === 'cancelled') {
      setTimeout(() => {
        if (store.progress.status === 'cancelled') store.progress = { ...store.progress, status: 'idle' };
      }, 2500);
    }
  });
}

// Модалка выбора языка документации: открывается по Generate, после выбора — генерация
const langModal = ref(false);

// Подсказка при ошибке доступа к приватному репозиторию (403 / отказ аутентификации git)
const accessDenied = computed(() =>
  /error:\s*403|returned error: 403|Authentication failed|could not read Username|Repository not found|Permission denied/i.test(store.error || '')
);
let pendingLocal = null;

function generate() {
  store.error = '';
  if (!store.repoUrl.trim()) {
    store.error = t('errNoUrl');
    return;
  }
  if (!store.selectedModel) {
    store.error = t('errNoModel');
    return;
  }
  pendingLocal = null;
  langModal.value = true;
}

async function confirmLanguage(code) {
  store.language = code;
  localStorage.setItem('language', code);
  langModal.value = false;
  if (pendingLocal) {
    const { name, files } = pendingLocal;
    pendingLocal = null;
    await runLocalGeneration(name, files);
  } else {
    await runGeneration();
  }
}

async function runGeneration() {
  store.liveSection = null;
  localStorage.setItem('lastRepoUrl', store.repoUrl);
  try {
    const { jobId, repoKey } = await startGeneration({
      repoUrl: store.repoUrl,
      language: store.language,
      modelId: store.selectedModel,
      lmUrl: store.lmUrl || undefined,
      embeddingModel: store.embeddingModel || undefined,
      maxFiles: store.maxFiles || undefined,
      maxTokens: store.maxTokens || undefined,
      improvementIdeas: store.improvementIdeas || undefined,
      githubToken: store.githubToken.trim() || undefined,
    });
    trackJob(jobId, repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

provide('generateRepo', generate);

// Догенерация недостающей языковой версии открытой wiki (кнопка «+» в сайдбаре)
async function generateLangVersion(code) {
  store.error = '';
  if (!store.docs) return;
  if (!store.selectedModel) {
    store.error = t('errNoModel');
    return;
  }
  try {
    const { jobId, repoKey } = await startLangGeneration({
      repo: store.docs.repoKey,
      language: code,
      modelId: store.selectedModel,
      lmUrl: store.lmUrl || undefined,
      embeddingModel: store.embeddingModel || undefined,
      maxTokens: store.maxTokens || undefined,
      improvementIdeas: store.improvementIdeas || undefined,
    });
    trackJob(jobId, repoKey);
  } catch (e) {
    store.error = e.message;
  }
}
provide('generateLang', generateLangVersion);

// Клик по файлу открывает раздел wiki, содержащий этот файл
watch(
  () => store.selectedFile,
  (f) => {
    store.sidebarOpen = false;
    if (f && store.docs && store.docs.sections) {
      const sec = store.docs.sections.find((s) => s.files.includes(f));
      if (sec) store.selectedSection = sec.id;
    }
  }
);
watch(
  () => store.selectedSection,
  () => {
    store.sidebarOpen = false;
  }
);

function generateLocal(name, files) {
  store.error = '';
  if (!store.selectedModel) {
    store.error = t('errNoModel');
    return;
  }
  pendingLocal = { name, files };
  langModal.value = true;
}

async function runLocalGeneration(name, files) {
  try {
    const { jobId, repoKey } = await startLocalGeneration({
      name,
      language: store.language,
      modelId: store.selectedModel,
      lmUrl: store.lmUrl || undefined,
      embeddingModel: store.embeddingModel || undefined,
      maxFiles: store.maxFiles || undefined,
      maxTokens: store.maxTokens || undefined,
      improvementIdeas: store.improvementIdeas || undefined,
      files,
    });
    trackJob(jobId, repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

function openFromUrl(url) {
  store.repoUrl = url;
  generate();
}

onMounted(async () => {
  applyTheme();
  const shareToken = new URLSearchParams(window.location.search).get('share');
  if (shareToken) {
    store.readOnly = true;
    try {
      store.docs = await fetchShared(shareToken);
    } catch (e) {
      store.error = e.message;
    }
    return;
  }
  loadModels();
  loadRepos();
  const last = localStorage.getItem('lastRepoKey');
  if (last) {
    try {
      await loadDocs(last);
    } catch {
      /* нет сохранённой документации */
    }
  }
});
</script>

<template>
  <div class="min-h-screen bg-slate-50 dark:bg-slate-900 text-slate-800 dark:text-slate-300 transition-colors duration-200">
    <AppHeader :on-generate="generate" :on-reload-models="loadModels" />

    <div v-if="store.error" data-testid="error-banner"
      class="mx-auto max-w-7xl mt-4 px-4">
      <div class="rounded-md border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 px-4 py-3 text-sm flex items-start justify-between gap-4">
        <div class="min-w-0">
          <span>{{ store.error }}</span>
          <p v-if="accessDenied" data-testid="error-403-hint" class="mt-2 text-red-800 dark:text-red-300">
            {{ store.githubToken.trim() ? t('hint403WithToken') : t('hint403NoToken') }}
          </p>
        </div>
        <button data-testid="error-dismiss-button" class="shrink-0 font-medium hover:underline" @click="store.error = ''">✕</button>
      </div>
    </div>

    <ProgressBar />

    <div v-if="showWorkspace" class="flex">
      <aside
        class="w-72 shrink-0 border-r border-slate-200 dark:border-slate-800 h-[calc(100vh-61px)] sticky top-[61px] overflow-y-auto px-3 py-4 hidden md:block"
        data-testid="sidebar">
        <FileTree />
      </aside>
      <main class="flex-1 min-w-0 px-6 py-8">
        <MarkdownViewer />
      </main>

      <div v-if="store.sidebarOpen" class="fixed inset-0 z-50 md:hidden" data-testid="mobile-sidebar">
        <div class="absolute inset-0 bg-black/50" data-testid="mobile-sidebar-backdrop" @click="store.sidebarOpen = false" />
        <div class="absolute left-0 top-0 bottom-0 w-72 bg-white dark:bg-slate-900 border-r border-slate-200 dark:border-slate-800 overflow-y-auto px-3 py-4 shadow-xl">
          <FileTree />
        </div>
      </div>
    </div>
    <HomePage v-else :on-open-url="openFromUrl" :on-generate-local="generateLocal" />

    <ChatPanel />

    <div
      v-if="langModal"
      data-testid="doc-lang-modal"
      class="fixed inset-0 z-[90] bg-black/50 backdrop-blur-sm flex items-center justify-center p-4"
      @click.self="langModal = false">
      <div class="relative w-full max-w-md rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shadow-2xl p-6">
        <button
          data-testid="doc-lang-close"
          :title="t('cancel')"
          class="absolute top-3 right-3 p-1.5 rounded-md text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors duration-200"
          @click="langModal = false">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
        <h2 class="text-lg font-semibold text-slate-800 dark:text-slate-200 pr-8">{{ t('chooseDocLang') }}</h2>
        <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">{{ t('chooseDocLangHint') }}</p>
        <div class="mt-4 grid grid-cols-2 gap-2 max-h-[50vh] overflow-y-auto pr-1">
          <button
            v-for="l in DOC_LANGUAGES"
            :key="l.code"
            :data-testid="`doc-lang-${l.code}`"
            class="rounded-md border px-3 py-2 text-sm text-left transition-colors duration-200"
            :class="store.language === l.code
              ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400 font-medium'
              : 'border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500'"
            @click="confirmLanguage(l.code)">
            {{ l.label }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
