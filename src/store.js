import { reactive } from 'vue';

export const store = reactive({
  theme: localStorage.getItem('theme') || 'dark',
  lmUrl: localStorage.getItem('lmUrl') || '',
  embeddingModel: localStorage.getItem('embeddingModel') || '',
  maxFiles: localStorage.getItem('maxFiles') || '',
  maxTokens: localStorage.getItem('maxTokens') || '',
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
