<script setup>
import { onMounted, computed, provide, watch } from 'vue';
import { store, applyTheme } from './store';
import { fetchModels, startGeneration, startLocalGeneration, openProgress, fetchShared } from './api';
import { loadDocs, loadRepos } from './actions';
import AppHeader from './components/AppHeader.vue';
import ProgressBar from './components/ProgressBar.vue';
import FileTree from './components/FileTree.vue';
import MarkdownViewer from './components/MarkdownViewer.vue';
import ChatPanel from './components/ChatPanel.vue';
import HomePage from './components/HomePage.vue';

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
      store.lmStatusText = 'LM Studio отвечает, но модели не загружены';
    } else {
      store.lmStatus = 'ok';
      store.lmStatusText = `LM Studio на связи (${Date.now() - started} мс) · моделей: ${models.length}`;
    }
  } catch (e) {
    store.models = [];
    store.modelsError = e.message;
    store.lmStatus = 'error';
    store.lmStatusText = `LM Studio недоступен: ${e.message}`;
  }
}

function trackJob(jobId, repoKey) {
  store.progress = { status: 'queued', currentFile: '', processed: 0, total: 0, error: '', cachedHits: 0, partial: '' };
  openProgress(jobId, async (p) => {
    store.progress = p;
    if (p.status === 'done') {
      try {
        await loadDocs(repoKey);
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

async function generate() {
  store.error = '';
  if (!store.repoUrl.trim()) {
    store.error = 'Укажите URL GitHub репозитория';
    return;
  }
  if (!store.selectedModel) {
    store.error = 'Выберите AI модель (проверьте, что LM Studio запущен)';
    return;
  }
  localStorage.setItem('lastRepoUrl', store.repoUrl);
  localStorage.setItem('language', store.language);
  try {
    const { jobId, repoKey } = await startGeneration({
      repoUrl: store.repoUrl,
      language: store.language,
      modelId: store.selectedModel,
      lmUrl: store.lmUrl || undefined,
      embeddingModel: store.embeddingModel || undefined,
      maxFiles: store.maxFiles || undefined,
      maxTokens: store.maxTokens || undefined,
    });
    trackJob(jobId, repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

provide('generateRepo', generate);

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

async function generateLocal(name, files) {
  store.error = '';
  if (!store.selectedModel) {
    store.error = 'Выберите AI модель (проверьте, что LM Studio запущен)';
    return;
  }
  try {
    const { jobId, repoKey } = await startLocalGeneration({
      name,
      language: store.language,
      modelId: store.selectedModel,
      lmUrl: store.lmUrl || undefined,
      embeddingModel: store.embeddingModel || undefined,
      maxFiles: store.maxFiles || undefined,
      maxTokens: store.maxTokens || undefined,
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
        <span>{{ store.error }}</span>
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
  </div>
</template>
