// Языки CoderWiki: полное имя из папки → код локали, подпись, короткий бейдж, направление письма
export const LANGS = [
  { name: 'english', code: 'en', label: 'English', short: 'EN', dir: 'ltr', ogLocale: 'en_US', htmlLang: 'en' },
  { name: 'russian', code: 'ru', label: 'Русский', short: 'RU', dir: 'ltr', ogLocale: 'ru_RU', htmlLang: 'ru' },
  { name: 'chinese', code: 'zh', label: '中文', short: '中文', dir: 'ltr', ogLocale: 'zh_CN', htmlLang: 'zh-CN' },
  { name: 'japanese', code: 'ja', label: '日本語', short: '日本', dir: 'ltr', ogLocale: 'ja_JP', htmlLang: 'ja' },
  { name: 'spanish', code: 'es', label: 'Español', short: 'ES', dir: 'ltr', ogLocale: 'es_ES', htmlLang: 'es' },
  { name: 'german', code: 'de', label: 'Deutsch', short: 'DE', dir: 'ltr', ogLocale: 'de_DE', htmlLang: 'de' },
  { name: 'portuguese', code: 'pt', label: 'Português', short: 'PT', dir: 'ltr', ogLocale: 'pt_BR', htmlLang: 'pt' },
  { name: 'french', code: 'fr', label: 'Français', short: 'FR', dir: 'ltr', ogLocale: 'fr_FR', htmlLang: 'fr' },
  { name: 'arabic', code: 'ar', label: 'العربية', short: 'AR', dir: 'rtl', ogLocale: 'ar_AR', htmlLang: 'ar' },
  { name: 'hindi', code: 'hi', label: 'हिन्दी', short: 'HI', dir: 'ltr', ogLocale: 'hi_IN', htmlLang: 'hi' },
  { name: 'indonesian', code: 'id', label: 'Bahasa Indonesia', short: 'ID', dir: 'ltr', ogLocale: 'id_ID', htmlLang: 'id' },
  { name: 'vietnamese', code: 'vi', label: 'Tiếng Việt', short: 'VI', dir: 'ltr', ogLocale: 'vi_VN', htmlLang: 'vi' },
  { name: 'kazakh', code: 'kk', label: 'Қазақша', short: 'KZ', dir: 'ltr', ogLocale: 'kk_KZ', htmlLang: 'kk' },
  { name: 'swahili', code: 'sw', label: 'Kiswahili', short: 'SW', dir: 'ltr', ogLocale: 'sw_KE', htmlLang: 'sw' },
];

// Неизвестный язык не ломает сборку: код — первые две буквы имени
export function langByName(name) {
  const n = String(name || '').toLowerCase();
  return (
    LANGS.find((l) => l.name === n) || {
      name: n,
      code: n.slice(0, 2) || 'xx',
      label: n ? n[0].toUpperCase() + n.slice(1) : '?',
      short: (n.slice(0, 2) || '??').toUpperCase(),
      dir: 'ltr',
      ogLocale: n.slice(0, 2),
      htmlLang: n.slice(0, 2),
    }
  );
}

export function langByCode(code) {
  return LANGS.find((l) => l.code === code) || langByName(code);
}
