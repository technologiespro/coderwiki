<script setup>
import { ref } from 'vue';
import { store, toggleTheme } from '../store';
import { fetchModels } from '../api';

const props = defineProps({
  onGenerate: Function,
  onReloadModels: Function,
});

const showSettings = ref(false);

// Проверка соединения с LM Studio по текущему URL из поля настроек
const connTest = ref({ status: 'idle', text: '' });
async function testConnection() {
  if (connTest.value.status === 'testing') return;
  connTest.value = { status: 'testing', text: 'Проверяем соединение…' };
  const started = Date.now();
  try {
    const { models } = await fetchModels(store.lmUrl);
    const ms = Date.now() - started;
    if (!models || !models.length) {
      connTest.value = { status: 'warn', text: `Сервер отвечает (${ms} мс), но модели не загружены — загрузите модель в LM Studio` };
    } else {
      const emb = models.filter((m) => m.id.toLowerCase().includes('embed')).length;
      connTest.value = {
        status: 'ok',
        text: `Соединение установлено за ${ms} мс · моделей: ${models.length}${emb ? ` (embedding: ${emb})` : ' (embedding-моделей нет — RAG-чат будет в keyword-режиме)'}`,
      };
    }
  } catch (e) {
    connTest.value = { status: 'error', text: `Нет соединения: ${e.message}` };
  }
}

function saveSettings() {
  localStorage.setItem('lmUrl', store.lmUrl);
  localStorage.setItem('embeddingModel', store.embeddingModel);
  localStorage.setItem('maxFiles', store.maxFiles);
  localStorage.setItem('maxTokens', store.maxTokens);
  showSettings.value = false;
  props.onReloadModels && props.onReloadModels();
}

const isGenerating = () =>
  ['queued', 'cloning', 'parsing', 'generating'].includes(store.progress.status);
</script>

<template>
  <header
    class="sticky top-0 z-40 border-b border-slate-200 dark:border-slate-800 bg-white/80 dark:bg-slate-900/80 backdrop-blur-sm"
    data-testid="app-header">
    <div class="flex items-center gap-3 px-4 py-3 flex-wrap">
      <button
        v-if="store.docs && !store.readOnly"
        data-testid="mobile-sidebar-toggle"
        class="md:hidden rounded-md p-2 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
        title="Файлы"
        @click="store.sidebarOpen = true">
        <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="3" y1="6" x2="21" y2="6" /><line x1="3" y1="12" x2="21" y2="12" /><line x1="3" y1="18" x2="21" y2="18" />
        </svg>
      </button>
      <button class="flex items-center gap-2 mr-2" data-testid="logo-home-button" title="На главную"
        @click="() => { if (store.readOnly) return; store.docs = null; store.selectedFile = null; store.chat.open = false; }">
        <svg class="w-6 h-6 text-blue-600 dark:text-blue-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" /><path d="M20 3v4" /><path d="M22 5h-4" />
        </svg>
        <span class="text-lg font-semibold text-slate-800 dark:text-slate-200">SmartWiki</span>
      </button>

      <template v-if="store.readOnly">
        <span
          data-testid="readonly-badge"
          class="rounded-full border border-slate-200 dark:border-slate-700 px-3 py-1 text-xs font-medium text-slate-500 dark:text-slate-400">
          Только чтение{{ store.docs ? ` · ${store.docs.repoKey}` : '' }}
        </span>
        <div class="flex-1" />
      </template>

      <template v-else>
      <input
        v-model="store.repoUrl"
        data-testid="repo-url-input"
        type="text"
        placeholder="https://github.com/owner/repo"
        class="flex-1 min-w-[220px] rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-2 text-sm placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200"
        @keyup.enter="props.onGenerate && props.onGenerate()" />

      <div class="flex rounded-md border border-slate-200 dark:border-slate-700 overflow-hidden text-sm" data-testid="language-toggle">
        <button
          data-testid="language-russian-button"
          :class="store.language === 'russian'
            ? 'bg-blue-600 dark:bg-blue-500 text-white'
            : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
          class="px-3 py-2 font-medium transition-colors duration-200"
          @click="store.language = 'russian'">RU</button>
        <button
          data-testid="language-english-button"
          :class="store.language === 'english'
            ? 'bg-blue-600 dark:bg-blue-500 text-white'
            : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
          class="px-3 py-2 font-medium transition-colors duration-200"
          @click="store.language = 'english'">EN</button>
      </div>

      <button
        data-testid="lm-status-indicator"
        :title="store.lmStatusText"
        class="relative flex items-center justify-center w-8 h-9 rounded-md hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
        @click="showSettings = true">
        <span
          class="w-2.5 h-2.5 rounded-full transition-colors duration-300"
          :class="{
            'bg-slate-400 animate-pulse': store.lmStatus === 'checking',
            'bg-green-500': store.lmStatus === 'ok',
            'bg-amber-500': store.lmStatus === 'warn',
            'bg-red-500': store.lmStatus === 'error',
          }" />
      </button>

      <select
        v-model="store.selectedModel"
        data-testid="model-select"
        class="rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-2 text-sm max-w-[220px] focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200">
        <option v-if="!store.models.length" value="" disabled>
          {{ store.modelsError ? 'Модели недоступны' : 'Загрузка моделей…' }}
        </option>
        <option v-for="m in store.models" :key="m.id" :value="m.id">{{ m.id }}</option>
      </select>

      <button
        data-testid="generate-button"
        :disabled="isGenerating()"
        class="rounded-md bg-blue-600 dark:bg-blue-500 text-white px-4 py-2 text-sm font-medium hover:bg-blue-700 dark:hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-200"
        @click="props.onGenerate && props.onGenerate()">
        {{ isGenerating() ? 'Генерация…' : 'Generate' }}
      </button>

      <div class="relative">
        <button
          data-testid="settings-button"
          class="rounded-md p-2 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
          title="Настройки LM Studio"
          @click="showSettings = !showSettings">
          <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </button>
        <div
          v-if="showSettings"
          data-testid="settings-panel"
          class="absolute right-0 top-12 w-80 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shadow-lg p-4 z-50">
          <label class="block text-xs font-medium text-slate-500 dark:text-slate-400 mb-1.5">URL LM Studio сервера</label>
          <input
            v-model="store.lmUrl"
            data-testid="lm-url-input"
            type="text"
            placeholder="http://192.168.31.110:1234"
            class="w-full rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-900 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50" />
          <p class="text-xs text-slate-400 dark:text-slate-500 mt-1.5">Пусто — используется значение из .env бэкенда</p>
          <label class="block text-xs font-medium text-slate-500 dark:text-slate-400 mt-3 mb-1.5">Embedding-модель (RAG-чат)</label>
          <select
            v-model="store.embeddingModel"
            data-testid="embedding-model-select"
            class="w-full rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-900 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50">
            <option value="">Авто (первая с «embed» в имени)</option>
            <option v-for="m in store.models" :key="m.id" :value="m.id">{{ m.id }}</option>
          </select>
          <div class="grid grid-cols-2 gap-2 mt-3">
            <div>
              <label class="block text-xs font-medium text-slate-500 dark:text-slate-400 mb-1.5">Макс. файлов</label>
              <input
                v-model="store.maxFiles"
                data-testid="max-files-input"
                type="number" min="1" placeholder="50"
                class="w-full rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-900 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50" />
            </div>
            <div>
              <label class="block text-xs font-medium text-slate-500 dark:text-slate-400 mb-1.5">Контекст (токенов)</label>
              <input
                v-model="store.maxTokens"
                data-testid="max-tokens-input"
                type="number" min="1000" step="1000" placeholder="8000"
                class="w-full rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-900 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50" />
            </div>
          </div>
          <div class="mt-3 flex gap-2">
            <button
              data-testid="test-connection-button"
              :disabled="connTest.status === 'testing'"
              class="flex-1 rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-900 px-3 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 disabled:opacity-60 transition-colors duration-200 inline-flex items-center justify-center gap-2"
              @click="testConnection">
              <svg v-if="connTest.status === 'testing'" class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <path d="M21 12a9 9 0 1 1-6.219-8.56" />
              </svg>
              Проверить соединение
            </button>
          </div>
          <p
            v-if="connTest.status !== 'idle'"
            data-testid="connection-status"
            class="mt-2 text-xs rounded-md px-2.5 py-2"
            :class="{
              'bg-slate-100 dark:bg-slate-700/60 text-slate-500 dark:text-slate-400': connTest.status === 'testing',
              'bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-400': connTest.status === 'ok',
              'bg-amber-50 dark:bg-amber-900/30 text-amber-700 dark:text-amber-400': connTest.status === 'warn',
              'bg-red-50 dark:bg-red-900/30 text-red-600 dark:text-red-400': connTest.status === 'error',
            }">
            {{ connTest.text }}
          </p>
          <button
            data-testid="save-settings-button"
            class="mt-3 w-full rounded-md bg-blue-600 dark:bg-blue-500 text-white px-3 py-2 text-sm font-medium hover:bg-blue-700 dark:hover:bg-blue-600 transition-colors duration-200"
            @click="saveSettings">Сохранить и обновить модели</button>
        </div>
      </div>
      </template>

      <button
        data-testid="theme-toggle-button"
        class="rounded-md p-2 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
        title="Переключить тему"
        @click="toggleTheme">
        <svg v-if="store.theme === 'dark'" class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5" /><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
        </svg>
        <svg v-else class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      </button>
    </div>
  </header>
</template>
