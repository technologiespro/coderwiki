<script setup>
import { computed, ref, reactive, watch, nextTick, onMounted, onUnmounted, inject } from 'vue';
import mermaid from 'mermaid';
import { store } from '../store';
import { API, streamRegenerateSection, createShare, isTauri, exportXdpDesktop, exportZipDesktop } from '../api';
import { md, linkifyPaths, exportSvgToPng, sanitizeMermaid, looksLikeMermaid } from '../markdown';
import SourceFiles from './SourceFiles.vue';

const regenerating = ref(false);
const toc = ref([]);
const activeHeading = ref('');
const generateRepo = inject('generateRepo', null);

const isGithub = computed(() => store.docs && String(store.docs.repoUrl || '').includes('github.com'));
const updating = computed(() =>
  ['queued', 'cloning', 'parsing', 'sectioning', 'generating', 'embedding', 'overview'].includes(store.progress.status)
);

function updateWiki() {
  if (!generateRepo || !store.docs || updating.value) return;
  store.repoUrl = store.docs.repoUrl;
  generateRepo();
}

const shareCopied = ref(false);
async function shareWiki() {
  try {
    const { token } = await createShare(store.docs.repoKey);
    const link = `${window.location.origin}/?share=${token}`;
    try {
      await navigator.clipboard.writeText(link);
    } catch {
      window.prompt('Скопируйте публичную ссылку:', link);
    }
    shareCopied.value = true;
    setTimeout(() => (shareCopied.value = false), 2500);
  } catch (e) {
    store.error = e.message;
  }
}

const changes = computed(() => {
  const c = store.docs && store.docs.changes;
  if (!c) return null;
  return c.added.length + c.modified.length + c.removed.length ? c : null;
});

const rootTab = ref('overview');
function printPage() {
  window.print();
}

const archHtml = computed(() => {
  if (!store.docs) return '';
  const deps = store.docs.deps || [];
  if (!deps.length) return md.parse('_Зависимости между файлами не обнаружены (нет import/require связей)._');
  const id = (p) => `n_${p.replace(/[^\w]/g, '_')}`;
  const lines = ['graph LR'];
  const seen = new Set();
  for (const [a, b] of deps) {
    for (const p of [a, b]) {
      if (!seen.has(p)) {
        seen.add(p);
        lines.push(`  ${id(p)}["${p}"]`);
      }
    }
    lines.push(`  ${id(a)} --> ${id(b)}`);
  }
  return md.parse('```mermaid\n' + lines.join('\n') + '\n```');
});

// Экспорт .xdp с прогрессом обучения словаря (по поколениям генетического алгоритма)
const xdpJob = reactive({ status: 'idle', gen: 0, gens: 0, line: '', error: '' });
async function exportXdp() {
  if (xdpJob.status === 'packing' || !store.docs) return;
  Object.assign(xdpJob, { status: 'packing', gen: 0, gens: 0, line: '', error: '' });
  if (isTauri) {
    // Desktop: Rust-упаковка + системный диалог сохранения
    try {
      await exportXdpDesktop(store.docs.repoKey, (line) => {
        const m = line.match(/^gen (\d+)\/(\d+)/);
        if (m) {
          xdpJob.gen = Number(m[1]);
          xdpJob.gens = Number(m[2]);
        }
        xdpJob.line = line;
      });
      xdpJob.status = 'idle';
    } catch (e) {
      Object.assign(xdpJob, { status: 'error', error: String(e) });
    }
    return;
  }
  try {
    const r = await fetch(`${API}/export-xdp-job`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ repo: store.docs.repoKey }),
    });
    const { jobId, error } = await r.json();
    if (!jobId) throw new Error(error || 'Не удалось запустить упаковку');
    const es = new EventSource(`${API}/export-xdp-progress?jobId=${jobId}`);
    es.onmessage = (e) => {
      const d = JSON.parse(e.data);
      xdpJob.gen = d.gen;
      xdpJob.gens = d.gens;
      xdpJob.line = d.lines && d.lines.length ? d.lines[d.lines.length - 1] : '';
      if (d.status === 'done') {
        es.close();
        xdpJob.status = 'idle';
        const a = document.createElement('a');
        a.href = `${API}/export-xdp-file?jobId=${jobId}`;
        a.download = '';
        document.body.appendChild(a);
        a.click();
        a.remove();
      } else if (d.status === 'error') {
        es.close();
        Object.assign(xdpJob, { status: 'error', error: d.error || 'Ошибка упаковки' });
      }
    };
    es.onerror = () => {
      es.close();
      if (xdpJob.status === 'packing') Object.assign(xdpJob, { status: 'error', error: 'Соединение с сервером потеряно' });
    };
  } catch (e) {
    Object.assign(xdpJob, { status: 'error', error: e.message });
  }
}

// В desktop-версии ZIP сохраняется через системный диалог
function onZipClick(e) {
  if (!isTauri) return;
  e.preventDefault();
  exportZipDesktop(store.docs.repoKey).catch((err) => {
    store.error = String(err);
  });
}

let ticking = false;
function onScroll() {
  if (ticking) return;
  ticking = true;
  requestAnimationFrame(() => {
    ticking = false;
    let current = '';
    for (const item of toc.value) {
      const el = document.getElementById(item.id);
      if (el && el.getBoundingClientRect().top <= 110) current = item.id;
    }
    activeHeading.value = current || (toc.value[0] ? toc.value[0].id : '');
  });
}
onMounted(() => window.addEventListener('scroll', onScroll, { passive: true }));
onUnmounted(() => window.removeEventListener('scroll', onScroll));

async function regenerate() {
  if (!currentSection.value || regenerating.value) return;
  regenerating.value = true;
  const sec = currentSection.value;
  const old = sec.md;
  sec.md = '';
  try {
    await streamRegenerateSection(
      {
        repo: store.docs.repoKey,
        sectionId: sec.id,
        modelId: store.selectedModel || store.docs.modelId,
        lmUrl: store.lmUrl || undefined,
        maxTokens: store.maxTokens || undefined,
      },
      (delta) => {
        sec.md += delta;
      }
    );
  } catch (e) {
    sec.md = old;
    store.error = e.message;
  } finally {
    regenerating.value = false;
  }
}

function buildToc() {
  const items = [];
  let n = 0;
  container.value.querySelectorAll('article h1, article h2, article h3').forEach((h) => {
    if (!h.id) h.id = `h-${++n}-${h.textContent.trim().toLowerCase().replace(/[^\wа-яё]+/gi, '-').slice(0, 40)}`;
    items.push({ id: h.id, text: h.textContent.trim(), level: Number(h.tagName[1]) });
  });
  toc.value = items;
  onScroll();
}

function scrollToHeading(id) {
  const el = document.getElementById(id);
  if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

const container = ref(null);

const currentSection = computed(() => {
  if (!store.docs || !store.docs.sections || !store.selectedSection) return null;
  return store.docs.sections.find((s) => s.id === store.selectedSection) || null;
});

const html = computed(() => (currentSection.value && currentSection.value.md ? md.parse(currentSection.value.md) : ''));

const overviewHtml = computed(() =>
  store.docs && store.docs.overview ? md.parse(store.docs.overview) : ''
);

const liveHtml = computed(() =>
  ['sectioning', 'generating', 'overview'].includes(store.progress.status) && store.progress.partial
    ? md.parse(store.progress.partial)
    : ''
);

const fileList = computed(() =>
  store.docs ? store.docs.files || Object.keys(store.docs.docs || {}) : []
);

const downloadUrl = computed(() =>
  store.docs ? `${API}/download?repo=${encodeURIComponent(store.docs.repoKey)}` : ''
);

// Лайтбокс диаграмм: клик по схеме открывает её в большом размере (как в DeepWiki)
const lightbox = reactive({ open: false, svg: '', zoom: 1, baseW: 800 });
function openLightbox(svgEl) {
  if (!svgEl) return;
  const vb = svgEl.viewBox && svgEl.viewBox.baseVal;
  const w = vb && vb.width ? vb.width : svgEl.getBoundingClientRect().width || 800;
  const clone = svgEl.cloneNode(true);
  clone.removeAttribute('height');
  clone.style.maxWidth = 'none';
  lightbox.baseW = w;
  lightbox.svg = clone.outerHTML;
  // стартовый зум: вписать в 85% ширины экрана, но не более 2.2x от natural
  lightbox.zoom = Math.min(2.2, Math.max(0.9, (window.innerWidth * 0.85) / w));
  lightbox.open = true;
}
function onKeydown(e) {
  if (e.key === 'Escape') lightbox.open = false;
}
// Панорамирование мышью и зум колёсиком в лайтбоксе
const lbPanel = ref(null);
const lbPanning = ref(false);
let panStart = null;
function lbMouseDown(e) {
  if (e.button !== 0 || !lbPanel.value) return;
  panStart = { x: e.clientX, y: e.clientY, sl: lbPanel.value.scrollLeft, st: lbPanel.value.scrollTop };
  lbPanning.value = true;
}
function lbMouseMove(e) {
  if (!panStart || !lbPanel.value) return;
  lbPanel.value.scrollLeft = panStart.sl - (e.clientX - panStart.x);
  lbPanel.value.scrollTop = panStart.st - (e.clientY - panStart.y);
}
function lbMouseUp() {
  panStart = null;
  lbPanning.value = false;
}
function lbWheel(e) {
  const el = lbPanel.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const mx = e.clientX - rect.left + el.scrollLeft;
  const my = e.clientY - rect.top + el.scrollTop;
  const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
  const nz = Math.min(4, Math.max(0.25, lightbox.zoom * factor));
  const ratio = nz / lightbox.zoom;
  lightbox.zoom = nz;
  // держим точку под курсором на месте
  nextTick(() => {
    el.scrollLeft = mx * ratio - (e.clientX - rect.left);
    el.scrollTop = my * ratio - (e.clientY - rect.top);
  });
}
onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  window.addEventListener('mousemove', lbMouseMove);
  window.addEventListener('mouseup', lbMouseUp);
});
onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  window.removeEventListener('mousemove', lbMouseMove);
  window.removeEventListener('mouseup', lbMouseUp);
});

let mmdId = 0;
async function postProcess() {
  await nextTick();
  if (!container.value) return;
  linkifyPaths(container.value);
  buildToc();
  mermaid.initialize({
    startOnLoad: false,
    theme: store.theme === 'dark' ? 'dark' : 'neutral',
    securityLevel: 'loose',
    flowchart: { htmlLabels: false },
  });
  // Помеченные ```mermaid блоки + непомеченные блоки с mermaid-синтаксисом
  const blocks = [...container.value.querySelectorAll('pre code')].filter(
    (b) => b.classList.contains('language-mermaid') || looksLikeMermaid(b.textContent)
  );
  for (const block of blocks) {
    const code = block.textContent;
    let svg = null;
    for (const candidate of [code, sanitizeMermaid(code)]) {
      try {
        svg = (await mermaid.render(`mmd-${++mmdId}`, candidate)).svg;
        break;
      } catch {
        document.getElementById(`dmmd-${mmdId}`)?.remove();
        document.getElementById(`mmd-${mmdId}`)?.remove();
      }
    }
    if (!svg) continue;
    {
      const div = document.createElement('div');
      div.className = 'mermaid-diagram relative my-6 flex justify-center overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-800/50 p-4 cursor-zoom-in';
      div.setAttribute('data-testid', 'mermaid-diagram');
      div.title = 'Нажмите, чтобы увеличить';
      div.innerHTML = svg;
      div.onclick = () => openLightbox(div.querySelector('svg'));
      const btn = document.createElement('button');
      btn.textContent = 'PNG';
      btn.title = 'Скачать диаграмму как PNG';
      btn.setAttribute('data-testid', 'export-diagram-png-button');
      btn.className = 'absolute top-2 right-2 rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-700 px-2 py-1 text-xs font-medium text-slate-500 dark:text-slate-300 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200';
      btn.onclick = (e) => {
        e.stopPropagation();
        exportSvgToPng(
          div.querySelector('svg'),
          `diagram-${(store.docs ? store.docs.repoKey : 'repo').replace('/', '__')}.png`,
          store.theme === 'dark' ? '#1e293b' : '#ffffff'
        );
      };
      div.appendChild(btn);
      block.closest('pre').replaceWith(div);
    }
  }
}

watch([html, overviewHtml, archHtml, rootTab, () => store.selectedSection, () => store.theme], postProcess, {
  flush: 'post',
  immediate: true,
});
</script>

<template>
  <div class="mx-auto flex max-w-6xl gap-10" data-testid="markdown-viewer">
    <div ref="container" class="min-w-0 flex-1 max-w-4xl mx-auto">
    <template v-if="liveHtml && !currentSection">
      <div class="flex items-center gap-2 mb-4" data-testid="live-generation-header">
        <span class="relative flex h-2.5 w-2.5">
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-500 opacity-60"></span>
          <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-blue-600 dark:bg-blue-400"></span>
        </span>
        <p class="text-xs font-mono text-slate-400 dark:text-slate-500">
          Генерация: {{ store.progress.currentFile }}
        </p>
      </div>
      <article class="prose prose-slate dark:prose-invert max-w-none" data-testid="live-markdown-content" v-html="liveHtml" />
    </template>

    <template v-else-if="store.docs && currentSection">
      <div class="flex items-center gap-2 mb-4">
        <p class="text-xs font-mono text-slate-400 dark:text-slate-500" data-testid="selected-section-title">
          {{ currentSection.title }}
        </p>
        <button
          v-if="!store.readOnly"
          data-testid="regenerate-section-button"
          :disabled="regenerating"
          title="Перегенерировать этот раздел"
          class="p-1 rounded-md text-slate-400 dark:text-slate-500 hover:text-blue-600 dark:hover:text-blue-400 hover:bg-slate-100 dark:hover:bg-slate-800 disabled:opacity-60 transition-colors duration-200"
          @click="regenerate">
          <svg class="w-3.5 h-3.5" :class="regenerating ? 'animate-spin' : ''"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="23 4 23 10 17 10" /><polyline points="1 20 1 14 7 14" />
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
          </svg>
        </button>
        <span v-if="regenerating" class="text-xs text-blue-600 dark:text-blue-400">генерация…</span>
      </div>
      <SourceFiles :files="currentSection.files" />
      <article class="prose prose-slate dark:prose-invert max-w-none" data-testid="markdown-content" v-html="html" />
    </template>

    <template v-else-if="store.docs">
      <div class="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 class="text-4xl sm:text-5xl font-bold text-slate-800 dark:text-slate-200 tracking-tight">
            {{ store.docs.repoKey }}
          </h1>
          <p class="mt-3 text-slate-500 dark:text-slate-400">
            Документация сгенерирована {{ new Date(store.docs.generatedAt).toLocaleString() }}
            · язык: {{ store.docs.language === 'russian' ? 'русский' : 'английский' }}
          </p>
        </div>
        <div class="flex items-center gap-2 shrink-0 mt-2 flex-wrap">
          <button
            v-if="!store.readOnly"
            data-testid="share-wiki-button"
            title="Скопировать публичную ссылку (только чтение)"
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
            @click="shareWiki">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="18" cy="5" r="3" /><circle cx="6" cy="12" r="3" /><circle cx="18" cy="19" r="3" />
              <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" /><line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
            </svg>
            {{ shareCopied ? 'Ссылка скопирована!' : 'Поделиться' }}
          </button>
          <button
            v-if="isGithub && !store.readOnly"
            data-testid="update-wiki-button"
            :disabled="updating"
            title="Обновить wiki: перегенерируются только изменённые файлы"
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 disabled:opacity-50 transition-colors duration-200"
            @click="updateWiki">
            <svg class="w-4 h-4" :class="updating ? 'animate-spin' : ''"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10" /><polyline points="1 20 1 14 7 14" />
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
            </svg>
            Обновить
          </button>
          <a
            :href="downloadUrl"
            data-testid="download-zip-button"
            class="inline-flex items-center gap-2 rounded-md bg-blue-600 dark:bg-blue-500 text-white px-4 py-2 text-sm font-medium hover:bg-blue-700 dark:hover:bg-blue-600 transition-colors duration-200 shrink-0"
            @click="onZipClick">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" />
            </svg>
            Скачать ZIP
          </a>
          <button
            data-testid="export-xdp-button"
            :disabled="xdpJob.status === 'packing'"
            title="Экспорт .xdp контейнера (ZSTD + RAG-индекс + генетический словарь)"
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200 disabled:opacity-60 disabled:cursor-wait"
            @click="exportXdp">
            <svg v-if="xdpJob.status === 'packing'" class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
            <span v-if="xdpJob.status === 'packing'">{{ xdpJob.gens ? `словарь ${xdpJob.gen}/${xdpJob.gens}` : 'упаковка…' }}</span>
            <span v-else>.xdp</span>
          </button>
          <button
            data-testid="print-wiki-button"
            title="Печать / сохранить в PDF"
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
            @click="printPage">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="6 9 6 2 18 2 18 9" /><path d="M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2" /><rect x="6" y="14" width="12" height="8" />
            </svg>
          </button>
        </div>
      </div>

      <div v-if="xdpJob.status === 'packing' || xdpJob.status === 'error'" class="mt-4" data-testid="xdp-progress-panel">
        <div v-if="xdpJob.status === 'packing'" class="flex items-center gap-3">
          <div class="h-1.5 w-48 rounded-full bg-slate-200 dark:bg-slate-700 overflow-hidden shrink-0">
            <div
              class="h-full bg-blue-600 dark:bg-blue-500 transition-[width] duration-300"
              :style="{ width: xdpJob.gens ? `${Math.round((xdpJob.gen / xdpJob.gens) * 100)}%` : '8%' }" />
          </div>
          <span class="text-xs text-slate-500 dark:text-slate-400 font-mono truncate" data-testid="xdp-progress-text">
            {{ xdpJob.gens ? `обучение ZSTD-словаря · поколение ${xdpJob.gen}/${xdpJob.gens}` : 'упаковка .xdp…' }}{{ xdpJob.line ? ` · ${xdpJob.line}` : '' }}
          </span>
        </div>
        <p v-else class="text-xs text-red-500" data-testid="xdp-error">{{ xdpJob.error }}</p>
      </div>

      <div class="mt-8 flex rounded-md border border-slate-200 dark:border-slate-700 overflow-hidden text-sm w-fit" data-testid="root-tabs">
        <button
          data-testid="tab-overview"
          :class="rootTab === 'overview' ? 'bg-blue-600 dark:bg-blue-500 text-white' : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
          class="px-4 py-2 font-medium transition-colors duration-200"
          @click="rootTab = 'overview'">Обзор</button>
        <button
          data-testid="tab-architecture"
          :class="rootTab === 'arch' ? 'bg-blue-600 dark:bg-blue-500 text-white' : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
          class="px-4 py-2 font-medium transition-colors duration-200"
          @click="rootTab = 'arch'">Архитектура</button>
      </div>

      <template v-if="rootTab === 'arch'">
        <article
          class="prose prose-slate dark:prose-invert max-w-none mt-6"
          data-testid="architecture-content"
          v-html="archHtml" />
      </template>
      <template v-else>

      <div class="mt-8">
        <SourceFiles :files="fileList" />
      </div>

      <div
        v-if="changes"
        data-testid="changes-panel"
        class="mt-2 mb-8 rounded-lg border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-800/40 p-4">
        <p class="text-sm font-semibold text-slate-700 dark:text-slate-300 mb-2">
          Изменения при последнем обновлении
          <span class="font-normal text-xs text-slate-400 dark:text-slate-500 ml-1">
            {{ new Date(changes.updatedAt).toLocaleString() }}
          </span>
        </p>
        <div class="space-y-1">
          <button
            v-for="f in changes.modified"
            :key="`m-${f}`"
            :data-testid="`change-modified-${f}`"
            class="flex items-center gap-2 text-sm font-mono text-slate-600 dark:text-slate-400 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
            @click="store.selectedFile = f">
            <span class="w-4 text-center font-bold text-amber-600 dark:text-amber-400">~</span>{{ f }}
          </button>
          <button
            v-for="f in changes.added"
            :key="`a-${f}`"
            :data-testid="`change-added-${f}`"
            class="flex items-center gap-2 text-sm font-mono text-slate-600 dark:text-slate-400 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
            @click="store.selectedFile = f">
            <span class="w-4 text-center font-bold text-green-600 dark:text-green-400">+</span>{{ f }}
          </button>
          <div
            v-for="f in changes.removed"
            :key="`r-${f}`"
            :data-testid="`change-removed-${f}`"
            class="flex items-center gap-2 text-sm font-mono text-slate-400 dark:text-slate-500 line-through">
            <span class="w-4 text-center font-bold text-red-500 dark:text-red-400">−</span>{{ f }}
          </div>
        </div>
      </div>

      <article
        v-if="overviewHtml"
        class="prose prose-slate dark:prose-invert max-w-none"
        data-testid="overview-content"
        v-html="overviewHtml" />

      <h2 class="text-lg font-semibold text-slate-800 dark:text-slate-200 mt-12 mb-4">Разделы</h2>
      <div class="grid gap-2" data-testid="docs-index">
        <button
          v-for="s in store.docs.sections || []"
          :key="s.id"
          :data-testid="`docs-index-${s.id}`"
          class="text-left rounded-md border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-800/50 px-4 py-3 text-sm text-slate-600 dark:text-slate-400 hover:border-blue-400 dark:hover:border-blue-500 hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200 flex items-center justify-between gap-3"
          @click="store.selectedSection = s.id">
          <span class="font-medium truncate">{{ s.title }}</span>
          <span class="shrink-0 text-xs text-slate-400 dark:text-slate-500">{{ s.files.length }} файлов</span>
        </button>
      </div>
      </template>
    </template>

    <template v-else>
      <div class="flex flex-col items-center justify-center text-center py-24" data-testid="empty-state">
        <svg class="w-16 h-16 text-slate-300 dark:text-slate-700 mb-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" /><path d="M20 3v4" /><path d="M22 5h-4" />
        </svg>
        <h1 class="text-4xl sm:text-5xl font-bold text-slate-800 dark:text-slate-200 tracking-tight">SmartWiki</h1>
        <p class="mt-4 text-slate-500 dark:text-slate-400 max-w-md">
          Вставьте ссылку на GitHub репозиторий, выберите язык и модель — AI сгенерирует wiki с тематическими разделами документации.
        </p>
      </div>
    </template>
    </div>

    <aside
      v-if="toc.length > 1"
      class="hidden xl:block w-56 shrink-0 sticky top-[85px] self-start max-h-[calc(100vh-110px)] overflow-y-auto"
      data-testid="page-toc">
      <p class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">
        На этой странице
      </p>
      <button
        v-for="item in toc"
        :key="item.id"
        :data-testid="`toc-item-${item.id}`"
        class="block w-full text-left text-sm py-1 truncate transition-colors duration-200 border-l-2 pl-2"
        :class="activeHeading === item.id
          ? 'text-blue-600 dark:text-blue-400 font-medium border-blue-600 dark:border-blue-400'
          : 'text-slate-500 dark:text-slate-400 border-transparent hover:text-blue-600 dark:hover:text-blue-400'"
        :style="{ marginLeft: `${(item.level - 1) * 12}px` }"
        @click="scrollToHeading(item.id)">
        {{ item.text }}
      </button>
    </aside>

    <Teleport to="body">
      <div
        v-if="lightbox.open"
        data-testid="diagram-lightbox"
        class="fixed inset-0 z-[100] bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 sm:p-8"
        @click.self="lightbox.open = false">
        <div class="absolute top-4 right-4 z-10 flex items-center gap-2">
          <button
            data-testid="lightbox-zoom-in"
            title="Увеличить"
            class="w-9 h-9 rounded-md bg-white/10 hover:bg-white/20 text-white text-lg font-bold transition-colors duration-200"
            @click="lightbox.zoom = Math.min(4, lightbox.zoom * 1.25)">+</button>
          <button
            data-testid="lightbox-zoom-out"
            title="Уменьшить"
            class="w-9 h-9 rounded-md bg-white/10 hover:bg-white/20 text-white text-lg font-bold transition-colors duration-200"
            @click="lightbox.zoom = Math.max(0.25, lightbox.zoom / 1.25)">−</button>
          <button
            data-testid="lightbox-close"
            title="Закрыть (Esc)"
            class="w-9 h-9 rounded-md bg-white/10 hover:bg-white/20 text-white text-lg transition-colors duration-200"
            @click="lightbox.open = false">✕</button>
        </div>
        <div
          ref="lbPanel"
          class="max-w-full max-h-full overflow-auto rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 p-6 shadow-2xl select-none"
          :class="lbPanning ? 'cursor-grabbing' : 'cursor-grab'"
          data-testid="lightbox-panel"
          @click.stop
          @mousedown.prevent="lbMouseDown"
          @wheel.prevent="lbWheel">
          <div
            class="lightbox-svg"
            :style="{ width: `${Math.round(lightbox.baseW * lightbox.zoom)}px`, maxWidth: 'none' }"
            v-html="lightbox.svg" />
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style>
.lightbox-svg svg {
  width: 100%;
  height: auto;
  max-width: none !important;
}
.mermaid-diagram:hover {
  border-color: rgb(96 165 250 / 0.6);
}
</style>
