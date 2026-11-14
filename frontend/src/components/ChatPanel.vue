<script setup>
import { ref, watch, nextTick } from 'vue';
import { store } from '../store';
import { t } from '../i18n';
import { streamChat, sendFeedback } from '../api';
import { md, linkifyPaths } from '../markdown';

const input = ref('');
const listEl = ref(null);
const chatMode = ref('code');

async function rate(m, vote) {
  if (m.voted) return;
  m.voted = vote;
  try {
    await sendFeedback(store.docs.repoKey, vote);
  } catch {
    /* необязательно */
  }
}

watch(
  () => (store.docs ? store.docs.repoKey : null),
  () => {
    store.chat.messages = [];
  }
);

async function scrollDown() {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}

function exportChat() {
  if (!store.chat.messages.length || !store.docs) return;
  let mdText = `# Чат с репозиторием ${store.docs.repoKey}\n\n_${new Date().toLocaleString()}_\n\n---\n\n`;
  for (const m of store.chat.messages) {
    if (m.role === 'user') {
      mdText += `**Вопрос:** ${m.content}\n\n`;
    } else {
      mdText += `**Ответ:**\n\n${m.content}\n\n`;
      if (m.sources && m.sources.length) {
        mdText += `_Источники: ${m.sources.map((s) => s.path + (s.lines ? `#${s.lines[0]}-${s.lines[1]}` : '')).join(', ')}_\n\n`;
      }
      mdText += '---\n\n';
    }
  }
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([mdText], { type: 'text/markdown' }));
  a.download = `chat-${store.docs.repoKey.replace('/', '__')}.md`;
  a.click();
  URL.revokeObjectURL(a.href);
}

async function send() {
  const q = input.value.trim();
  if (!q || store.chat.loading || !store.docs) return;
  input.value = '';
  const history = store.chat.messages.map((m) => ({ role: m.role, content: m.content }));
  store.chat.messages.push({ role: 'user', content: q });
  const answer = { role: 'assistant', content: '', sources: [], mode: '', truncated: false, voted: '' };
  store.chat.messages.push(answer);
  store.chat.loading = true;
  scrollDown();
  try {
    await streamChat(
      {
        repo: store.docs.repoKey,
        question: q,
        history: history.slice(-6),
        modelId: store.selectedModel || store.docs.modelId,
        lmUrl: store.lmUrl || undefined,
        embeddingModel: store.embeddingModel || undefined,
        maxTokens: store.maxTokens || undefined,
        mode: chatMode.value === 'arch' ? 'arch' : undefined,
      },
      {
        onSources: (payload) => {
          answer.sources = payload.sources || [];
          answer.mode = payload.mode || '';
          answer.truncated = !!payload.truncated;
        },
        onDelta: (delta) => {
          answer.content += delta;
          scrollDown();
        },
      }
    );
  } catch (e) {
    answer.content += `\n\n**Ошибка:** ${e.message}`;
  } finally {
    store.chat.loading = false;
    await nextTick();
    linkifyPaths(listEl.value);
    scrollDown();
  }
}
</script>

<template>
  <div v-if="store.docs && !store.readOnly">
    <button
      v-if="!store.chat.open"
      data-testid="chat-open-button"
      :title="t('chatTitle')"
      class="fixed bottom-6 right-6 z-40 rounded-full bg-blue-600 dark:bg-blue-500 text-white p-4 shadow-lg hover:bg-blue-700 dark:hover:bg-blue-600 transition-colors duration-200"
      @click="store.chat.open = true">
      <svg class="w-6 h-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
      </svg>
    </button>

    <div
      v-else
      data-testid="chat-panel"
      class="fixed right-0 bottom-0 top-[61px] z-40 w-full sm:w-[420px] flex flex-col border-l border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 shadow-xl">
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-200 dark:border-slate-800">
        <div class="min-w-0">
          <p class="text-sm font-semibold text-slate-800 dark:text-slate-200">{{ t('chatTitle') }}</p>
          <p class="text-xs text-slate-400 dark:text-slate-500 truncate">{{ store.docs.repoKey }}</p>
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <button
            data-testid="chat-export-button"
            title="Скачать диалог как Markdown"
            :disabled="!store.chat.messages.length"
            class="p-2 rounded-md text-slate-400 dark:text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800 disabled:opacity-40 transition-colors duration-200"
            @click="exportChat">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" />
            </svg>
          </button>
          <button
            data-testid="chat-clear-button"
            title="Очистить историю"
            class="p-2 rounded-md text-slate-400 dark:text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
            @click="store.chat.messages = []">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            </svg>
          </button>
          <button
            data-testid="chat-close-button"
            title="Закрыть"
            class="p-2 rounded-md text-slate-400 dark:text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors duration-200"
            @click="store.chat.open = false">
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      <div ref="listEl" class="flex-1 overflow-y-auto p-4 space-y-4" data-testid="chat-messages">
        <div class="flex rounded-md border border-slate-200 dark:border-slate-700 overflow-hidden text-xs w-fit" data-testid="chat-mode-toggle">
          <button
            data-testid="chat-mode-code"
            :class="chatMode === 'code' ? 'bg-blue-600 dark:bg-blue-500 text-white' : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
            class="px-3 py-1.5 font-medium transition-colors duration-200"
            @click="chatMode = 'code'">{{ t('chatModeCode') }}</button>
          <button
            data-testid="chat-mode-arch"
            :class="chatMode === 'arch' ? 'bg-blue-600 dark:bg-blue-500 text-white' : 'bg-white dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'"
            class="px-3 py-1.5 font-medium transition-colors duration-200"
            @click="chatMode = 'arch'">{{ t('chatModeArch') }}</button>
        </div>
        <p v-if="!store.chat.messages.length" class="text-sm text-slate-400 dark:text-slate-500">
          Задайте вопрос по коду — AI ответит на основе сгенерированной документации репозитория.
        </p>
        <template v-for="(m, i) in store.chat.messages" :key="i">
          <div
            v-if="m.role === 'user'"
            class="ml-10 rounded-lg bg-blue-600 dark:bg-blue-500 text-white px-3 py-2 text-sm whitespace-pre-wrap"
            data-testid="chat-user-message">
            {{ m.content }}
          </div>
          <div v-else data-testid="chat-assistant-message">
            <div
              class="prose prose-sm prose-slate dark:prose-invert max-w-none"
              v-html="md.parse(m.content || '…')" />
            <div v-if="m.sources && m.sources.length" class="flex flex-wrap items-center gap-x-2 gap-y-1 mt-2" data-testid="chat-sources">
              <span class="text-xs text-slate-400 dark:text-slate-500">Источники:</span>
              <button
                v-for="s in m.sources"
                :key="s.path + (s.lines ? s.lines.join('-') : '')"
                :data-testid="`chat-source-${s.path}`"
                :title="s.lines ? `Строки ${s.lines[0]}–${s.lines[1]}` : s.path"
                class="text-xs font-mono text-blue-600 dark:text-blue-400 hover:underline"
                @click="store.selectedFile = s.path">
                {{ s.path }}{{ s.lines ? `#${s.lines[0]}-${s.lines[1]}` : '' }}
              </button>
            </div>
            <p
              v-if="m.mode === 'basic' && !store.chat.loading"
              class="mt-1.5 text-xs text-amber-600 dark:text-amber-400"
              data-testid="chat-basic-search-warning">
              Embedding-модель не найдена — использован базовый поиск по ключевым словам
            </p>
            <p
              v-if="m.truncated && !store.chat.loading"
              class="mt-1 text-xs text-slate-400 dark:text-slate-500"
              data-testid="chat-truncated-warning">
              ⚠ Контекст оптимизирован под лимиты модели
            </p>
            <div v-if="!store.chat.loading && m.content" class="flex items-center gap-1 mt-1.5" data-testid="chat-feedback">
              <button
                data-testid="chat-thumbs-up"
                :class="m.voted === 'up' ? 'text-green-600 dark:text-green-400' : 'text-slate-300 dark:text-slate-600 hover:text-green-600 dark:hover:text-green-400'"
                class="p-1 transition-colors duration-200" title="Полезный ответ" @click="rate(m, 'up')">
                <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3" /></svg>
              </button>
              <button
                data-testid="chat-thumbs-down"
                :class="m.voted === 'down' ? 'text-red-600 dark:text-red-400' : 'text-slate-300 dark:text-slate-600 hover:text-red-600 dark:hover:text-red-400'"
                class="p-1 transition-colors duration-200" title="Плохой ответ — скорректировать параметры поиска" @click="rate(m, 'down')">
                <svg class="w-3.5 h-3.5 rotate-180" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3" /></svg>
              </button>
              <span v-if="m.voted === 'down'" class="text-xs text-slate-400 dark:text-slate-500">параметры поиска скорректированы</span>
            </div>
          </div>
        </template>
      </div>

      <div class="border-t border-slate-200 dark:border-slate-800 p-3 flex gap-2">
        <input
          v-model="input"
          data-testid="chat-input"
          :disabled="store.chat.loading"
          type="text"
          :placeholder="t('chatPlaceholder')"
          class="flex-1 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-3 py-2 text-sm placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500/50 dark:focus:ring-blue-400/50 transition-colors duration-200 disabled:opacity-60"
          @keyup.enter="send" />
        <button
          data-testid="chat-send-button"
          :disabled="store.chat.loading || !input.trim()"
          class="rounded-md bg-blue-600 dark:bg-blue-500 text-white px-3 py-2 hover:bg-blue-700 dark:hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-200"
          @click="send">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>
