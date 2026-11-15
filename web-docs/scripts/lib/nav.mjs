// Та же глава на другом языке: сопоставление из sync (alt), иначе — главная репозитория
export function altPath(repo, code, fromPath) {
  const v = repo.langs[code];
  if (!v) return null;
  for (const l of Object.values(repo.langs)) {
    if (l.path === fromPath) return v.path;
    const c = l.chapters.find((x) => x.path === fromPath);
    if (c) return l.code === code ? c.path : (c.alt && c.alt[code]) || v.path;
  }
  return v.path;
}
