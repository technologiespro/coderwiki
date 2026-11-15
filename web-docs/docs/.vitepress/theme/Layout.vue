<script setup lang="ts">
import DefaultTheme from 'vitepress/theme';
import { useData } from 'vitepress';
import { watchEffect } from 'vue';
import UiLangSwitch from './components/UiLangSwitch.vue';
import LangBadges from './components/LangBadges.vue';
import DocFooter from './components/DocFooter.vue';

const { dir, lang } = useData();
// Направление письма и язык документа следуют за локалью страницы (RTL для арабского)
watchEffect(() => {
  if (typeof document === 'undefined') return;
  document.documentElement.setAttribute('dir', dir.value || 'ltr');
  document.documentElement.setAttribute('lang', lang.value);
});
</script>

<template>
  <DefaultTheme.Layout>
    <template #nav-bar-content-after><UiLangSwitch /></template>
    <template #doc-before><LangBadges /></template>
    <template #doc-after><DocFooter /></template>
  </DefaultTheme.Layout>
</template>
