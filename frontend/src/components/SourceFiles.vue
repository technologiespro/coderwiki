<script setup>
import { ref, computed } from 'vue';
import { store } from '../store';
import { t } from '../i18n';

// eslint-disable-next-line no-undef -- макрос компилятора Vue
const props = defineProps({ files: { type: Array, default: () => [] } });
const open = ref(false);

const githubBase = computed(() => {
  if (!store.docs || !String(store.docs.repoUrl || '').includes('github.com')) return '';
  return `https://github.com/${store.docs.repoKey}/blob/HEAD`;
});
</script>

<template>
  <div
    v-if="files.length"
    class="mb-6 rounded-lg border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-800/40 overflow-hidden"
    data-testid="source-files-panel">
    <button
      data-testid="source-files-toggle"
      class="w-full flex items-center justify-between px-4 py-2.5 text-sm font-medium text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
      @click="open = !open">
      <span>{{ t('sourceFiles') }}</span>
      <svg
        class="w-4 h-4 transition-transform duration-200"
        :class="open ? 'rotate-90' : ''"
        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="9 18 15 12 9 6" />
      </svg>
    </button>
    <div v-if="open" class="px-4 pb-3 pt-1" data-testid="source-files-list">
      <p class="text-xs text-slate-400 dark:text-slate-500 mb-2">
        Следующие файлы были использованы в качестве контекста для этой страницы:
      </p>
      <div class="space-y-1">
        <div v-for="f in files" :key="f" class="flex items-center gap-2">
          <a
            v-if="githubBase"
            :href="`${githubBase}/${f}`"
            target="_blank"
            rel="noopener"
            title="Открыть на GitHub"
            class="shrink-0 text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 transition-colors duration-200">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
            </svg>
          </a>
          <button
            :data-testid="`source-file-${f}`"
            class="text-sm font-mono text-blue-600 dark:text-blue-400 hover:underline truncate"
            @click="store.selectedFile = f">
            {{ f }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
