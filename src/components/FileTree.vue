<script setup>
import { ref, computed } from 'vue';
import { store } from '../store';
import { loadDocs, deleteRepo } from '../actions';
import TreeNode from './TreeNode.vue';

const query = ref('');

const results = computed(() => {
  if (!query.value.trim() || !store.docs) return null;
  const q = query.value.toLowerCase();
  const sections = [];
  const contents = [];
  for (const s of store.docs.sections || []) {
    if (s.title.toLowerCase().includes(q)) {
      sections.push(s);
    } else {
      const idx = s.md.toLowerCase().indexOf(q);
      if (idx !== -1) {
        const start = Math.max(0, idx - 30);
        contents.push({
          id: s.id,
          title: s.title,
          snippet: `${start > 0 ? '…' : ''}${s.md.slice(start, idx + q.length + 45).replace(/[\n#`*>|]/g, ' ').trim()}…`,
        });
      }
    }
  }
  const names = (store.docs.files || []).filter((f) => f.toLowerCase().includes(q));
  return { sections, names, contents };
});

async function openRepo(repoKey) {
  try {
    await loadDocs(repoKey);
    query.value = '';
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
</script>

<template>
  <div data-testid="file-tree">
    <div v-if="store.repos.length && !store.readOnly" class="mb-4" data-testid="repo-history">
      <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">История</p>
      <div
        v-for="r in store.repos"
        :key="r.repoKey"
        class="group flex items-center gap-1 rounded-md transition-colors duration-200"
        :class="store.docs && store.docs.repoKey === r.repoKey
          ? 'bg-blue-50 dark:bg-blue-900/30'
          : 'hover:bg-slate-100 dark:hover:bg-slate-800'">
        <button
          :data-testid="`history-item-${r.repoKey}`"
          class="flex-1 min-w-0 flex items-center justify-between gap-2 px-2 py-1.5 text-sm text-left"
          :class="store.docs && store.docs.repoKey === r.repoKey
            ? 'text-blue-700 dark:text-blue-400 font-medium'
            : 'text-slate-600 dark:text-slate-400'"
          @click="openRepo(r.repoKey)">
          <span class="truncate">{{ r.repoKey }}</span>
          <span class="shrink-0 text-xs text-slate-400 dark:text-slate-500">{{ r.fileCount }}</span>
        </button>
        <button
          :data-testid="`history-delete-${r.repoKey}`"
          title="Удалить из истории"
          class="shrink-0 p-1 mr-1 rounded text-slate-400 dark:text-slate-500 opacity-0 group-hover:opacity-100 hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          @click.stop="removeRepo(r.repoKey)">
          <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
      <div class="border-b border-slate-200 dark:border-slate-800 mt-3" />
    </div>
    <div v-if="store.docs" class="mb-3 px-2">
      <p class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">Репозиторий</p>
      <button
        data-testid="repo-root-button"
        class="mt-1 text-sm font-medium text-slate-700 dark:text-slate-300 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200 truncate block max-w-full"
        @click="store.selectedFile = null; store.selectedSection = null">
        {{ store.docs.repoKey }}
      </button>
    </div>
    <div v-if="store.docs && store.docs.sections && store.docs.sections.length" class="mb-4" data-testid="sections-nav">
      <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">Разделы</p>
      <button
        v-for="s in store.docs.sections"
        :key="s.id"
        :data-testid="`section-item-${s.id}`"
        class="w-full flex items-center justify-between gap-2 rounded-md px-2 py-1.5 text-sm text-left transition-colors duration-200"
        :class="store.selectedSection === s.id
          ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium'
          : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'"
        @click="store.selectedSection = s.id; store.selectedFile = null">
        <span class="truncate">{{ s.title }}</span>
        <span class="shrink-0 text-xs text-slate-400 dark:text-slate-500">{{ s.files.length }}</span>
      </button>
      <div class="border-b border-slate-200 dark:border-slate-800 mt-3" />
    </div>
    <template v-if="store.docs">
      <div class="px-2 mb-3">
        <input
          v-model="query"
          data-testid="file-search-input"
          type="text"
          placeholder="Поиск файлов…"
          class="w-full rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-sm placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200" />
      </div>
      <template v-if="results">
        <button
          v-for="s in results.sections"
          :key="`sec-${s.id}`"
          :data-testid="`search-section-${s.id}`"
          class="w-full rounded-md px-2 py-1.5 text-sm text-left truncate transition-colors duration-200"
          :class="store.selectedSection === s.id
            ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium'
            : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'"
          @click="store.selectedSection = s.id">
          {{ s.title }}
        </button>
        <button
          v-for="p in results.names"
          :key="p"
          :data-testid="`search-result-${p}`"
          class="w-full rounded-md px-2 py-1.5 text-sm text-left truncate transition-colors duration-200"
          :class="store.selectedFile === p
            ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium'
            : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'"
          @click="store.selectedFile = p">
          {{ p }}
        </button>
        <p
          v-if="results.contents.length"
          class="px-2 pt-3 pb-1 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">
          В тексте документации
        </p>
        <button
          v-for="r in results.contents"
          :key="r.id"
          :data-testid="`content-result-${r.id}`"
          class="w-full rounded-md px-2 py-1.5 text-left transition-colors duration-200 hover:bg-slate-100 dark:hover:bg-slate-800"
          @click="store.selectedSection = r.id">
          <span class="block truncate text-sm text-slate-600 dark:text-slate-400">{{ r.title }}</span>
          <span class="block truncate text-xs text-slate-400 dark:text-slate-500">{{ r.snippet }}</span>
        </button>
        <p
          v-if="!results.sections.length && !results.names.length && !results.contents.length"
          class="px-2 text-sm text-slate-400 dark:text-slate-500"
          data-testid="search-no-results">
          Ничего не найдено
        </p>
      </template>
      <template v-else>
        <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">Файлы</p>
        <TreeNode v-for="node in store.docs.tree" :key="node.path" :node="node" />
      </template>
    </template>
    <p v-else class="px-2 text-sm text-slate-400 dark:text-slate-500">
      Файлы появятся здесь после генерации документации
    </p>
  </div>
</template>
