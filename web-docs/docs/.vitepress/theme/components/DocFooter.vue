<script setup lang="ts">
import { useUiLang, generatedAt } from '../data';
const { t } = useUiLang();
const date = generatedAt.slice(0, 10);
</script>

<template>
  <p class="cw-doc-footer" data-testid="doc-footer">
    <span>{{ t.generatedBy }}</span>
    <span class="cw-doc-footer__sep">·</span>
    <span>{{ t.lastBuild }}: <time :datetime="generatedAt">{{ date }}</time></span>
  </p>
</template>

<style scoped>
.cw-doc-footer {
  margin-top: 32px;
  padding-top: 16px;
  border-top: 1px solid var(--vp-c-divider);
  font-size: 12px;
  color: var(--vp-c-text-3);
}
.cw-doc-footer__sep {
  margin: 0 8px;
}
</style>
