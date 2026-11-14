<script setup>
import { ref, computed, inject, watch } from 'vue';
import { store } from '../store';
import { t, docLangShort, docLangLabel, DOC_LANGUAGES } from '../i18n';
import { fetchLiveSection } from '../api';

const generateLang = inject('generateLang', null);
const showAddLang = ref(false);

const busyStatuses = ['queued', 'cloning', 'parsing', 'sectioning', 'generating', 'embedding', 'overview'];
const genBusy = computed(() => busyStatuses.includes(store.progress.status));

// Языковые версии текущего репозитория (общая база owner/name)
const langVersions = computed(() => {
  if (!store.docs) return [];
  const base = store.docs.repoKey.split('@')[0];
  return store.repos.filter((r) => r.repoKey.split('@')[0] === base);
});

// Языки, для которых версии ещё нет — предлагаются в дропдауне «+»
const missingLangs = computed(() =>
  DOC_LANGUAGES.filter((l) => !langVersions.value.some((v) => v.language === l.code))
);

// Язык, который сейчас догенерируется для открытого репозитория
const generatingLang = computed(() => {
  if (!genBusy.value || !store.progress.repoKey || !store.docs) return null;
  const [b, lang] = store.progress.repoKey.split('@');
  if (b !== store.docs.repoKey.split('@')[0] || !lang) return null;
  return langVersions.value.some((v) => v.language === lang) ? null : lang;
});

function addLang(code) {
  showAddLang.value = false;
  if (generateLang) generateLang(code);
}

// Группировка разделов по главам для древовидного сайдбара
function groupSections(list) {
  const out = [];
  for (const s of list || []) {
    const g = s.group || null;
    const last = out[out.length - 1];
    if (last && last.group === g) last.items.push(s);
    else out.push({ group: g, items: [s] });
  }
  return out;
}
const groupedSections = computed(() => groupSections(store.docs && store.docs.sections));
const groupedLive = computed(() => groupSections(store.progress.sections));

// Свёрнутые главы: { [owner/name]: [название главы, ...] } — сохраняются между сессиями
const COLLAPSED_KEY = 'collapsedChapters';
function loadCollapsed() {
  try {
    const v = JSON.parse(localStorage.getItem(COLLAPSED_KEY) || '{}');
    return v && typeof v === 'object' ? v : {};
  } catch {
    return {};
  }
}
const collapsed = ref(loadCollapsed());
const repoBase = () => (store.docs ? store.docs.repoKey.split('@')[0] : '');
function isCollapsed(group) {
  return !!group && (collapsed.value[repoBase()] || []).includes(group);
}
function setCollapsed(group, value) {
  const base = repoBase();
  const list = (collapsed.value[base] || []).filter((g) => g !== group);
  if (value) list.push(group);
  collapsed.value = { ...collapsed.value, [base]: list };
  localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed.value));
}
function toggleGroup(group) {
  setCollapsed(group, !isCollapsed(group));
}
// Раздел, открытый из поиска или по ссылке, не должен остаться в свёрнутой главе
function revealSection(id) {
  const s = id && store.docs && (store.docs.sections || []).find((x) => x.id === id);
  if (s && s.group && isCollapsed(s.group)) setCollapsed(s.group, false);
  store.selectedSection = id;
}
watch(() => store.selectedSection, revealSection);

function openSection(id) {
  store.selectedSection = id;
  store.selectedFile = null;
}

// Клик по заголовку главы — открыть первую страницу главы и развернуть её
function openGroup(g) {
  if (isCollapsed(g.group)) setCollapsed(g.group, false);
  openSection(g.items[0].id);
}

async function switchLang(v) {
  if (v.repoKey === store.docs.repoKey) return;
  try {
    await loadDocs(v.repoKey);
  } catch (e) {
    store.error = e.message;
  }
}

// Открыть уже готовый раздел во время генерации остальных
async function openLive(s) {
  try {
    const { md } = await fetchLiveSection(store.progress.repoKey, s.id);
    store.liveSection = { id: s.id, title: s.title, md };
    store.sidebarOpen = false;
  } catch (e) {
    store.error = e.message;
  }
}
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
      <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">{{ t('history') }}</p>
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
      <p class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">{{ t('repository') }}</p>
      <button
        data-testid="repo-root-button"
        class="mt-1 text-sm font-medium text-slate-700 dark:text-slate-300 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200 truncate block max-w-full"
        @click="store.selectedFile = null; store.selectedSection = null">
        {{ store.docs.repoKey.split('@')[0] }}
      </button>
      <div v-if="!store.readOnly && langVersions.length" class="mt-1.5 flex flex-wrap items-center gap-1" data-testid="wiki-lang-switcher">
        <button
          v-for="v in langVersions"
          :key="v.repoKey"
          :data-testid="`wiki-lang-${v.language}`"
          :title="docLangLabel(v.language)"
          class="rounded-md border px-1.5 py-0.5 text-[10px] font-semibold tracking-wide transition-colors duration-200"
          :class="v.repoKey === store.docs.repoKey
            ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400'
            : 'border-slate-200 dark:border-slate-700 text-slate-500 dark:text-slate-400 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400'"
          @click="switchLang(v)">
          {{ docLangShort(v.language) }}
        </button>
        <span
          v-if="generatingLang"
          data-testid="wiki-lang-generating"
          :title="`${docLangLabel(generatingLang)} — ${t('generating')}`"
          class="inline-flex items-center gap-1 rounded-md border border-blue-400/60 dark:border-blue-500/60 px-1.5 py-0.5 text-[10px] font-semibold text-blue-600 dark:text-blue-400">
          <svg class="w-2.5 h-2.5 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
          {{ docLangShort(generatingLang) }}
        </span>
        <div v-if="missingLangs.length" class="relative">
          <button
            data-testid="wiki-lang-add-button"
            :title="t('addLangTitle')"
            :disabled="genBusy"
            class="rounded-md border border-dashed border-slate-300 dark:border-slate-600 px-1.5 py-0.5 text-[10px] font-bold leading-none text-slate-400 dark:text-slate-500 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 disabled:opacity-40 disabled:cursor-not-allowed transition-colors duration-200"
            @click="showAddLang = !showAddLang">
            +
          </button>
          <div v-if="showAddLang" class="fixed inset-0 z-30" data-testid="add-lang-backdrop" @click="showAddLang = false" />
          <div
            v-if="showAddLang"
            data-testid="add-lang-dropdown"
            class="absolute left-0 top-full mt-1 z-40 w-48 max-h-64 overflow-y-auto rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shadow-xl p-1">
            <p class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">{{ t('addLangHeader') }}</p>
            <button
              v-for="l in missingLangs"
              :key="l.code"
              :data-testid="`add-lang-${l.code}`"
              class="w-full flex items-center justify-between gap-2 rounded-md px-2 py-1.5 text-sm text-left text-slate-600 dark:text-slate-300 hover:bg-blue-50 dark:hover:bg-blue-900/30 hover:text-blue-700 dark:hover:text-blue-400 transition-colors duration-200"
              @click="addLang(l.code)">
              <span class="truncate">{{ l.label }}</span>
              <span class="shrink-0 text-[10px] font-semibold text-slate-400 dark:text-slate-500">{{ l.short }}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
    <div v-if="store.docs && store.docs.sections && store.docs.sections.length" class="mb-4" data-testid="sections-nav">
      <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">{{ t('sections') }}</p>
      <template v-for="(g, gi) in groupedSections" :key="gi">
        <div
          v-if="g.group"
          class="flex items-center mt-1 rounded-md hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200">
          <button
            :data-testid="`section-group-toggle-${gi}`"
            :title="isCollapsed(g.group) ? t('expandChapter') : t('collapseChapter')"
            :aria-expanded="!isCollapsed(g.group)"
            class="shrink-0 p-1.5 rounded text-slate-400 dark:text-slate-500 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
            @click.stop="toggleGroup(g.group)">
            <svg
              class="w-3.5 h-3.5 transition-transform duration-200"
              :class="isCollapsed(g.group) ? '' : 'rotate-90'"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </button>
          <button
            :data-testid="`section-group-${gi}`"
            class="flex-1 min-w-0 flex items-center justify-between gap-2 py-1.5 pr-2 text-sm font-semibold text-left text-slate-700 dark:text-slate-200"
            @click="openGroup(g)">
            <span class="truncate">{{ g.group }}</span>
            <span v-if="isCollapsed(g.group)" class="shrink-0 text-xs font-normal text-slate-400 dark:text-slate-500">{{ g.items.length }}</span>
          </button>
        </div>
        <div v-show="!isCollapsed(g.group)" :data-testid="`section-group-items-${gi}`" :class="g.group ? 'ml-3 border-l border-slate-200 dark:border-slate-800' : ''">
          <button
            v-for="s in g.items"
            :key="s.id"
            :data-testid="`section-item-${s.id}`"
            class="w-full flex items-center justify-between gap-2 rounded-md py-1.5 text-sm text-left transition-colors duration-200"
            :class="[
              g.group ? 'pl-3 pr-2 rounded-l-none' : 'px-2',
              store.selectedSection === s.id
                ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium'
                : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800',
            ]"
            @click="openSection(s.id)">
            <span class="truncate">{{ s.title }}</span>
            <span class="shrink-0 text-xs text-slate-400 dark:text-slate-500">{{ s.files.length }}</span>
          </button>
        </div>
      </template>
      <div class="border-b border-slate-200 dark:border-slate-800 mt-3" />
    </div>
    <template v-if="store.docs">
      <div class="px-2 mb-3">
        <input
          v-model="query"
          data-testid="file-search-input"
          type="text"
          :placeholder="t('searchFiles')"
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
          @click="revealSection(s.id)">
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
          {{ t('inDocsText') }}
        </p>
        <button
          v-for="r in results.contents"
          :key="r.id"
          :data-testid="`content-result-${r.id}`"
          class="w-full rounded-md px-2 py-1.5 text-left transition-colors duration-200 hover:bg-slate-100 dark:hover:bg-slate-800"
          @click="revealSection(r.id)">
          <span class="block truncate text-sm text-slate-600 dark:text-slate-400">{{ r.title }}</span>
          <span class="block truncate text-xs text-slate-400 dark:text-slate-500">{{ r.snippet }}</span>
        </button>
        <p
          v-if="!results.sections.length && !results.names.length && !results.contents.length"
          class="px-2 text-sm text-slate-400 dark:text-slate-500"
          data-testid="search-no-results">
          {{ t('nothingFound') }}
        </p>
      </template>
      <template v-else>
        <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">{{ t('filesHeader') }}</p>
        <TreeNode v-for="node in store.docs.tree" :key="node.path" :node="node" />
      </template>
    </template>
    <div v-else-if="store.progress.sections && store.progress.sections.length" data-testid="live-sections">
      <p class="px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1">{{ t('sections') }}</p>
      <template v-for="(g, gi) in groupedLive" :key="gi">
        <p v-if="g.group" class="px-2 py-1.5 mt-1 text-sm font-semibold truncate text-slate-700 dark:text-slate-200">{{ g.group }}</p>
        <div :class="g.group ? 'ml-3 border-l border-slate-200 dark:border-slate-800' : ''">
          <component
            :is="s.done ? 'button' : 'div'"
            v-for="s in g.items"
            :key="s.id"
            :data-testid="`live-section-${s.id}`"
            class="w-full flex items-center gap-2 py-1.5 text-sm text-left rounded-md transition-colors duration-200"
            :class="[
              g.group ? 'pl-3 pr-2 rounded-l-none' : 'px-2',
              s.done ? 'text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer' : 'text-slate-400 dark:text-slate-600',
              store.liveSection && store.liveSection.id === s.id ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium' : '',
            ]"
            @click="s.done && openLive(s)">
            <svg v-if="s.done" class="w-3.5 h-3.5 shrink-0 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12" />
            </svg>
            <svg v-else-if="store.progress.currentFile === s.title" class="w-3.5 h-3.5 shrink-0 animate-spin text-blue-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
            <span v-else class="w-3.5 h-3.5 shrink-0 flex items-center justify-center"><span class="w-1.5 h-1.5 rounded-full bg-slate-300 dark:bg-slate-700" /></span>
            <span class="truncate">{{ s.title }}</span>
          </component>
        </div>
      </template>
    </div>
    <p v-else class="px-2 text-sm text-slate-400 dark:text-slate-500">
      {{ t('emptyHistory') }}
    </p>
  </div>
</template>
