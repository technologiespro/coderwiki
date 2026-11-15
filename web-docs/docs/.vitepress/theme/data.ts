// Клиентские данные манифеста: репозитории, доступные языки, переход между языковыми версиями
import { computed, ref, onMounted } from 'vue';
import { useData } from 'vitepress';
import manifest from '../generated-manifest.json';
import { strings } from '../i18n';
import { langByCode } from '../../../scripts/lib/langs.mjs';
import { altPath as altPathImpl } from '../../../scripts/lib/nav.mjs';

export type Chapter = { num: number | null; slug: string; title: string; path: string; alt?: Record<string, string> };
export type RepoLang = {
  code: string;
  label: string;
  short: string;
  path: string;
  title: string;
  description: string;
  chapters: Chapter[];
  chapterCount: number;
};
export type Repo = { key: string; owner: string; repo: string; updatedAt: string; tags: string[]; langs: Record<string, RepoLang> };

export const repos: Repo[] = manifest.repos as Repo[];
export const generatedAt: string = manifest.generatedAt;
export const uiLanguages: string[] = [...new Set<string>(['en', 'ru', ...(manifest.languages as string[])])];

const STORAGE_KEY = 'cw-ui-lang';
const stored = ref<string>('en');

function detectBrowserLang(): string {
  const prefs = (navigator.languages || [navigator.language || 'en']).map((l) => l.toLowerCase());
  for (const p of prefs) {
    const hit = uiLanguages.find((c) => p === c || p.startsWith(c + '-'));
    if (hit) return hit;
  }
  return 'en';
}

// Язык интерфейса: на страницах документации — язык страницы, на главной — выбор пользователя
export function useUiLang() {
  const { frontmatter } = useData();
  onMounted(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    stored.value = saved && uiLanguages.includes(saved) ? saved : detectBrowserLang();
  });
  const uiLang = computed<string>(() => (frontmatter.value.cwLang as string) || stored.value);
  const t = computed(() => strings(uiLang.value));
  function setUiLang(code: string) {
    stored.value = code;
    localStorage.setItem(STORAGE_KEY, code);
  }
  return { uiLang, t, setUiLang };
}

// Чистая функция: вызывается из computed, поэтому без useData() внутри
export function repoByKey(key: unknown): Repo | undefined {
  return repos.find((r) => r.key === key);
}

export function altPath(repo: Repo, code: string, fromPath: string): string | null {
  return altPathImpl(repo, code, fromPath);
}

export function langLabel(code: string) {
  return langByCode(code).label;
}

export function langShort(code: string) {
  return langByCode(code).short;
}
