<script setup>
import { computed } from 'vue';
import { store } from '../store';
import { cancelGeneration } from '../api';

const IN_PROGRESS = ['queued', 'cloning', 'parsing', 'sectioning', 'generating', 'embedding', 'overview'];

const active = computed(() =>
  [...IN_PROGRESS, 'done', 'cancelled'].includes(store.progress.status)
);

const cancellable = computed(() => IN_PROGRESS.includes(store.progress.status));

async function cancel() {
  try {
    await cancelGeneration(store.progress.jobId);
  } catch (e) {
    store.error = e.message;
  }
}

const statusText = computed(() => {
  const p = store.progress;
  switch (p.status) {
    case 'queued': return 'В очереди…';
    case 'cloning': return `Клонирование репозитория: ${p.currentFile}`;
    case 'parsing': return 'Разбор структуры файлов…';
    case 'sectioning': return 'AI планирует разделы wiki…';
    case 'generating': return `Генерация раздела: ${p.currentFile}`;
    case 'embedding': return `Создание векторных эмбеддингов: ${p.currentFile}`;
    case 'overview': return 'Создание общего обзора репозитория…';
    case 'cancelled': return 'Генерация отменена';
    case 'done': return `Готово! Обработано файлов: ${p.processed}${p.cachedHits ? ` (из кэша: ${p.cachedHits})` : ''}`;
    default: return '';
  }
});

const percent = computed(() => {
  const p = store.progress;
  if (p.status === 'done') return 100;
  if (p.status === 'sectioning') return 8;
  if (p.status === 'embedding') return 94;
  if (p.status === 'overview') return 97;
  if (!p.total) return 5;
  return Math.max(5, Math.round((p.processed / p.total) * 100));
});
</script>

<template>
  <div
    v-if="active"
    data-testid="progress-container"
    class="border-b border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 px-4 py-2.5">
    <div class="flex items-center justify-between mb-1.5">
      <span data-testid="progress-status-text" class="text-xs font-medium text-slate-500 dark:text-slate-400 truncate pr-4">
        {{ statusText }}
      </span>
      <span v-if="store.progress.total" class="text-xs text-slate-400 dark:text-slate-500 shrink-0">
        {{ store.progress.processed }} / {{ store.progress.total }}
      </span>
      <button
        v-if="cancellable"
        data-testid="cancel-generation-button"
        class="ml-3 shrink-0 rounded-md border border-slate-200 dark:border-slate-700 px-2.5 py-0.5 text-xs font-medium text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-red-600 dark:hover:text-red-400 transition-colors duration-200"
        @click="cancel">
        Отменить
      </button>
    </div>
    <div class="h-1.5 rounded-full bg-slate-100 dark:bg-slate-800 overflow-hidden">
      <div
        data-testid="progress-bar-fill"
        class="h-full rounded-full bg-blue-600 dark:bg-blue-500 transition-[width] duration-500"
        :style="{ width: percent + '%' }" />
    </div>
  </div>
</template>
