import { slugify as tr } from 'transliteration';

// Имя файла главы → чистый ASCII-слаг; если транслитерация дала мусор — запасной вариант
export function makeSlug(baseName, num, title) {
  const primary = clean(tr(baseName, { lowercase: true, separator: '-' }));
  if (hasLetters(primary)) return primary;
  const fromTitle = title ? clean(tr(title, { lowercase: true, separator: '-' })) : '';
  const prefix = num != null ? String(num).padStart(2, '0') : '';
  if (hasLetters(fromTitle)) return prefix ? `${prefix}-${fromTitle}` : fromTitle;
  return prefix ? `chapter-${prefix}` : 'page';
}

function clean(s) {
  return s
    .replace(/[^a-z0-9-]+/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80)
    .replace(/-+$/g, '');
}

// Слаг считается осмысленным, если в нём есть хотя бы 2 латинские буквы
function hasLetters(s) {
  return (s.match(/[a-z]/g) || []).length >= 2;
}

// Уникализация слагов в пределах одного репозитория
export function uniqueSlug(slug, used) {
  let s = slug;
  let i = 2;
  while (used.has(s)) s = `${slug}-${i++}`;
  used.add(s);
  return s;
}
