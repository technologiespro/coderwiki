import { reactive } from 'vue';

export const store = reactive({
  theme: localStorage.getItem('theme') || 'dark',
  lmUrl: localStorage.getItem('lmUrl') || '',
  embeddingModel: localStorage.getItem('embeddingModel') || '',
  maxFiles: localStorage.getItem('maxFiles') || '',
  maxTokens: localStorage.getItem('maxTokens') || '',
  improvementIdeas: localStorage.getItem('improvementIdeas') === '1',
  // Сэмплирование LLM: пусто — рекомендованные значения для документации (0.2 / 0.9 / 40 / 0.1)
  temperature: localStorage.getItem('temperature') || '',
  topP: localStorage.getItem('topP') || '',
  topK: localStorage.getItem('topK') || '',
  frequencyPenalty: localStorage.getItem('frequencyPenalty') || '',
  githubToken: localStorage.getItem('githubToken') || '',
  repoUrl: localStorage.getItem('lastRepoUrl') || '',
  language: localStorage.getItem('language') || 'russian',
  models: [],
  modelsError: '',
  lmStatus: 'checking',
  lmStatusText: 'Проверяем связь с LM Studio…',
  selectedModel: '',
  repos: [],
  progress: { status: 'idle', currentFile: '', processed: 0, total: 0, error: '', cachedHits: 0, partial: '' },
  docs: null,
  selectedFile: null,
  selectedSection: null,
  liveSection: null,
  readOnly: false,
  sidebarOpen: false,
  chat: { open: false, messages: [], loading: false },
  error: '',
});

export function applyTheme() {
  document.documentElement.classList.toggle('dark', store.theme === 'dark');
  localStorage.setItem('theme', store.theme);
}

export function toggleTheme() {
  store.theme = store.theme === 'dark' ? 'light' : 'dark';
  applyTheme();
}
