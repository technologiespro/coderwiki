<script setup lang="ts">
import { computed } from 'vue';
import { useData, useRouter, withBase } from 'vitepress';
import { useUiLang, uiLanguages, repoByKey, altPath, langLabel } from '../data';

const { frontmatter } = useData();
const router = useRouter();
const { uiLang, t, setUiLang } = useUiLang();
const repo = computed(() => repoByKey(frontmatter.value.cwRepo));

// На главной — язык интерфейса; на странице документации — языковые версии репозитория
const options = computed(() => {
  const codes = repo.value ? Object.keys(repo.value.langs) : uiLanguages;
  return codes.map((code) => ({ code, label: langLabel(code) }));
});

function onChange(e: Event) {
  const code = (e.target as HTMLSelectElement).value;
  setUiLang(code);
  if (repo.value) {
    const target = altPath(repo.value, code, frontmatter.value.cwPath);
    if (target) router.go(withBase(target));
    return;
  }
  // Витрина: переходим на главную выбранной локали (с сохранением ?tag=) — меню, поиск и футер тоже сменят язык
  router.go(withBase(`/${code}/`) + location.search);
}
</script>

<template>
  <label class="cw-lang-switch" :title="t.uiLanguage" data-testid="ui-lang-switch">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" /><path d="M2 12h20" /><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
    </svg>
    <select :value="uiLang" :aria-label="t.uiLanguage" data-testid="ui-lang-select" @change="onChange">
      <option v-for="o in options" :key="o.code" :value="o.code">{{ o.label }}</option>
    </select>
  </label>
</template>

<style scoped>
.cw-lang-switch {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-inline-start: 12px;
  padding: 0 10px;
  height: 32px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 999px;
  color: var(--vp-c-text-2);
  font-size: 13px;
  transition: border-color 0.2s, color 0.2s;
}
.cw-lang-switch:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-text-1);
}
.cw-lang-switch svg {
  width: 15px;
  height: 15px;
  flex: none;
}
.cw-lang-switch select {
  appearance: none;
  background: transparent;
  border: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
  padding-inline-end: 4px;
  max-width: 130px;
}
.cw-lang-switch select:focus {
  outline: none;
}
.cw-lang-switch select option {
  color: var(--vp-c-text-1);
  background: var(--vp-c-bg);
}
/* Мобильные: только глобус, невидимый select растянут поверх для нативного выбора */
@media (max-width: 767px) {
  .cw-lang-switch {
    position: relative;
    width: 32px;
    padding: 0;
    margin-inline-start: 6px;
    justify-content: center;
  }
  .cw-lang-switch select {
    position: absolute;
    inset: 0;
    width: 100%;
    max-width: none;
    opacity: 0;
    padding: 0;
  }
}
</style>
