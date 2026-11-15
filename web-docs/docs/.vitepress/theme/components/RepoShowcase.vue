<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { withBase } from 'vitepress';
import { repos, useUiLang, langLabel, langShort, type Repo } from '../data';

// Общий для всех витрин (/, /ru/, /en/ …): выбранный тег не теряется при смене языка
const activeTag = ref('');

const { uiLang, t } = useUiLang();

// Склонение «глава/главы/глав» по правилам языка интерфейса
function chaptersWord(n: number) {
  const forms = t.value.chaptersForms;
  return forms ? forms[new Intl.PluralRules(uiLang.value).select(n)] || t.value.chapters : t.value.chapters;
}
const query = ref('');

// Все теги витрины с количеством репозиториев, по убыванию популярности
const allTags = computed(() => {
  const counts = new Map<string, number>();
  for (const r of repos) for (const tg of r.tags || []) counts.set(tg, (counts.get(tg) || 0) + 1);
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([name, count]) => ({ name, count }));
});

// Один тег за раз: повторный клик снимает фильтр
function toggleTag(tag: string) {
  activeTag.value = activeTag.value === tag ? '' : tag;
}

const slug = (tag: string) => tag.toLowerCase().replace(/\+/g, 'plus').replace(/#/g, 'sharp').replace(/[^a-z0-9]+/g, '-');

// Выбранный тег живёт в ?tag= — ссылкой на отфильтрованную витрину можно поделиться
onMounted(() => {
  const fromUrl = new URLSearchParams(location.search).get('tag');
  activeTag.value = fromUrl && allTags.value.some((x) => x.name === fromUrl) ? fromUrl : '';
});
watch(activeTag, (tag) => {
  const url = new URL(location.href);
  if (tag) url.searchParams.set('tag', tag);
  else url.searchParams.delete('tag');
  history.replaceState(history.state, '', url);
});

// Карточка показывает описание на языке интерфейса, иначе на английском, иначе на первом доступном
function pick(r: Repo) {
  return r.langs[uiLang.value] || r.langs.en || r.langs[Object.keys(r.langs)[0]];
}

const cards = computed(() =>
  repos.map((r) => {
    const v = pick(r);
    return {
      key: r.key,
      owner: r.owner,
      repo: r.repo,
      description: v.description,
      chapterCount: v.chapterCount,
      tags: r.tags || [],
      href: withBase(v.path),
      langs: Object.keys(r.langs).map((code) => ({ code, short: langShort(code), label: langLabel(code), href: withBase(r.langs[code].path) })),
      haystack: `${r.key} ${(r.tags || []).join(' ')} ${Object.values(r.langs).map((l) => l.description).join(' ')}`.toLowerCase(),
    };
  })
);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  return cards.value.filter((c) => (!q || c.haystack.includes(q)) && (!activeTag.value || c.tags.includes(activeTag.value)));
});
</script>

<template>
  <section class="cw-home" data-testid="home-showcase">
    <header class="cw-hero">
      <p class="cw-hero__eyebrow">CoderWiki Docs</p>
      <h1 class="cw-hero__title">{{ t.heroTitle }}</h1>
      <p class="cw-hero__text">{{ t.heroText }}</p>
      <label class="cw-filter">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input v-model="query" type="search" :placeholder="t.filterPlaceholder" data-testid="repo-filter-input" />
      </label>
      <div v-if="allTags.length" class="cw-tags" role="toolbar" :aria-label="t.tags" data-testid="tag-filter-bar">
        <button type="button" class="cw-tag" :class="{ 'is-active': !activeTag }" :aria-pressed="!activeTag" data-testid="tag-filter-all" @click="activeTag = ''">{{ t.allTags }}</button>
        <button
          v-for="tg in allTags"
          :key="tg.name"
          type="button"
          class="cw-tag"
          :class="{ 'is-active': activeTag === tg.name }"
          :aria-pressed="activeTag === tg.name"
          :data-testid="`tag-filter-${slug(tg.name)}`"
          @click="toggleTag(tg.name)"
        >
          {{ tg.name }}<span class="cw-tag__count">{{ tg.count }}</span>
        </button>
      </div>
    </header>

    <p v-if="!cards.length" class="cw-empty" data-testid="home-empty">{{ t.noRepos }}</p>
    <p v-else-if="!filtered.length" class="cw-empty" data-testid="home-no-matches">{{ t.noMatches }}</p>

    <div v-else class="cw-grid" data-testid="repo-grid">
      <article v-for="c in filtered" :key="c.key" class="cw-card" :data-testid="`repo-card-${c.owner}-${c.repo}`">
        <a class="cw-card__main" :href="c.href">
          <p class="cw-card__owner">{{ c.owner }} /</p>
          <h2 class="cw-card__name">{{ c.repo }}</h2>
          <p class="cw-card__desc">{{ c.description }}</p>
        </a>
        <div v-if="c.tags.length" class="cw-card__tags" :data-testid="`repo-tags-${c.owner}-${c.repo}`">
          <button
            v-for="tg in c.tags"
            :key="tg"
            type="button"
            class="cw-card__tag"
            :class="{ 'is-active': activeTag === tg }"
            :data-testid="`repo-tag-${c.repo}-${slug(tg)}`"
            @click="toggleTag(tg)"
          >#{{ tg }}</button>
        </div>
        <footer class="cw-card__footer">
          <span class="cw-card__meta">{{ c.chapterCount }} {{ chaptersWord(c.chapterCount) }}</span>
          <span class="cw-card__langs">
            <a v-for="l in c.langs" :key="l.code" class="cw-card__lang" :href="l.href" :title="l.label" :hreflang="l.code" :data-testid="`repo-lang-${c.repo}-${l.code}`">{{ l.short }}</a>
          </span>
        </footer>
      </article>
    </div>
  </section>
</template>

<style scoped>
.cw-home {
  max-width: 1152px;
  margin: 0 auto;
  padding: 48px 24px 96px;
}
.cw-hero {
  max-width: 720px;
  margin-bottom: 48px;
}
.cw-hero__eyebrow {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--vp-c-brand-1);
}
.cw-hero__title {
  margin: 8px 0 12px;
  font-size: clamp(32px, 5vw, 52px);
  line-height: 1.1;
  font-weight: 800;
  letter-spacing: -0.02em;
}
.cw-hero__text {
  font-size: 17px;
  line-height: 1.6;
  color: var(--vp-c-text-2);
}
.cw-filter {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 28px;
  padding: 0 16px;
  height: 48px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  transition: border-color 0.2s, box-shadow 0.2s;
}
.cw-filter:focus-within {
  border-color: var(--vp-c-brand-1);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--vp-c-brand-1) 20%, transparent);
}
.cw-filter svg {
  width: 18px;
  height: 18px;
  color: var(--vp-c-text-3);
  flex: none;
}
.cw-filter input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  font-size: 15px;
  color: var(--vp-c-text-1);
}
.cw-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.cw-tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border-radius: 999px;
  border: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-2);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.2s, color 0.2s, border-color 0.2s, transform 0.15s;
}
.cw-tag:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}
.cw-tag:active {
  transform: scale(0.96);
}
.cw-tag.is-active {
  background: var(--vp-c-brand-1);
  border-color: var(--vp-c-brand-1);
  color: #fff;
}
.cw-tag__count {
  font-size: 11px;
  font-weight: 700;
  opacity: 0.7;
}
.cw-card__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 0 22px 6px;
}
.cw-card__tag {
  height: 22px;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.35);
  background: transparent;
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.2s, border-color 0.2s;
}
.cw-card__tag:hover,
.cw-card__tag.is-active {
  background: rgba(255, 255, 255, 0.25);
  border-color: rgba(255, 255, 255, 0.7);
}
.cw-empty {
  padding: 48px 0;
  color: var(--vp-c-text-2);
}
.cw-grid {
  display: grid;
  gap: 20px;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
}
.cw-card {
  display: flex;
  flex-direction: column;
  border-radius: 16px;
  color: #fff;
  background: linear-gradient(135deg, #2563eb 0%, #3b82f6 55%, #60a5fa 100%);
  box-shadow: 0 10px 30px -12px rgba(37, 99, 235, 0.55);
  overflow: hidden;
  transition: transform 0.25s ease, box-shadow 0.25s ease;
  animation: cw-rise 0.5s ease both;
}
.cw-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 18px 40px -14px rgba(37, 99, 235, 0.7);
}
.cw-card__main {
  display: block;
  padding: 22px 22px 14px;
  color: inherit;
  text-decoration: none;
}
.cw-card__owner {
  font-size: 13px;
  opacity: 0.8;
}
.cw-card__name {
  margin: 2px 0 10px;
  font-size: 22px;
  font-weight: 700;
  line-height: 1.2;
  border: none;
  padding: 0;
  letter-spacing: -0.01em;
}
.cw-card__desc {
  font-size: 14px;
  line-height: 1.55;
  opacity: 0.92;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.cw-card__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: auto;
  padding: 12px 22px 18px;
  font-size: 13px;
}
.cw-card__meta {
  opacity: 0.85;
}
.cw-card__langs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.cw-card__lang {
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 9px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.18);
  color: #fff;
  font-weight: 600;
  font-size: 12px;
  text-decoration: none;
  transition: background-color 0.2s;
}
.cw-card__lang:hover {
  background: rgba(255, 255, 255, 0.35);
}
@keyframes cw-rise {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
</style>
