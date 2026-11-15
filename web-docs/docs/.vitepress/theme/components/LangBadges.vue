<script setup lang="ts">
import { computed } from 'vue';
import { useData, withBase } from 'vitepress';
import { useUiLang, repoByKey, altPath, langLabel, langShort } from '../data';

const { frontmatter } = useData();
const { t } = useUiLang();
const repo = computed(() => repoByKey(frontmatter.value.cwRepo));

// Языковые версии текущей главы: сопоставленная по смыслу глава или главная репозитория
const badges = computed(() => {
  if (!repo.value) return [];
  return Object.keys(repo.value.langs).map((code) => ({
    code,
    short: langShort(code),
    label: langLabel(code),
    href: withBase(altPath(repo.value!, code, frontmatter.value.cwPath) || repo.value!.path),
    current: code === frontmatter.value.cwLang,
  }));
});
</script>

<template>
  <div v-if="badges.length" class="cw-badges" data-testid="article-lang-badges">
    <span class="cw-badges__label">{{ t.availableIn }}:</span>
    <template v-for="b in badges" :key="b.code">
      <span v-if="b.current" class="cw-badge cw-badge--current" :title="b.label" :data-testid="`article-lang-${b.code}`">{{ b.short }}</span>
      <a v-else class="cw-badge" :href="b.href" :title="b.label" :hreflang="b.code" :data-testid="`article-lang-${b.code}`">{{ b.short }}</a>
    </template>
  </div>
</template>

<style scoped>
.cw-badges {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-bottom: 20px;
  font-size: 12px;
}
.cw-badges__label {
  color: var(--vp-c-text-3);
  margin-inline-end: 4px;
}
.cw-badge {
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 9px;
  border-radius: 999px;
  border: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-2);
  font-weight: 600;
  letter-spacing: 0.02em;
  text-decoration: none;
  transition: background-color 0.2s, color 0.2s, border-color 0.2s;
}
a.cw-badge:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}
.cw-badge--current {
  background: var(--vp-c-brand-1);
  border-color: var(--vp-c-brand-1);
  color: #fff;
}
</style>
