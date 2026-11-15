// Разбор Markdown: заголовок, описание, переписывание ссылок, frontmatter

export function firstH1(md) {
  const m = md.match(/^#\s+(.+?)\s*#*\s*$/m);
  return m ? stripInline(m[1]) : '';
}

// Первый «обычный» абзац — без заголовков, таблиц, кода, списков и цитат
export function firstParagraph(md) {
  const lines = md.split(/\r?\n/);
  let inCode = false;
  let buf = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith('```') || line.startsWith('~~~')) {
      inCode = !inCode;
      continue;
    }
    if (inCode) continue;
    if (!line) {
      if (buf.length) break;
      continue;
    }
    if (/^(#|\||>|[-*+]\s|\d+\.\s|!\[|<|:::|---)/.test(line)) {
      if (buf.length) break;
      continue;
    }
    buf.push(line);
  }
  return stripInline(buf.join(' '));
}

// Снять inline-разметку: ссылки, код, выделение, html
export function stripInline(s) {
  return s
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, '')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/(\*\*|__)(.*?)\1/g, '$2')
    .replace(/(\*|_)(.*?)\1/g, '$2')
    .replace(/\s+/g, ' ')
    .trim();
}

// Описание до 160 символов, обрезка по границе слова
export function description(md, max = 160) {
  const p = firstParagraph(md);
  if (p.length <= max) return p;
  const cut = p.slice(0, max - 1);
  const sp = cut.lastIndexOf(' ');
  return (sp > max * 0.6 ? cut.slice(0, sp) : cut).replace(/[,;:\-–—]$/, '') + '…';
}

export function headings(md) {
  const out = [];
  let inCode = false;
  for (const raw of md.split(/\r?\n/)) {
    if (/^\s*(```|~~~)/.test(raw)) inCode = !inCode;
    if (inCode) continue;
    const m = raw.match(/^#{1,3}\s+(.+?)\s*#*\s*$/);
    if (m) out.push(stripInline(m[1]));
  }
  return out;
}

// Переписать внутренние ссылки между главами; resolve(oldName) → новый путь или null
export function rewriteLinks(md, resolve, onBroken) {
  const link = /(!?)\[([^\]]*)\]\(([^)\s]+)((?:\s+"[^"]*")?)\)/g;
  return md.replace(link, (whole, bang, text, target, title) => {
    if (bang) return whole;
    if (/^(https?:|mailto:|tel:|#|\/)/i.test(target)) return whole;
    const [rawPath, anchor] = target.split('#');
    let p;
    try {
      p = decodeURIComponent(rawPath);
    } catch {
      p = rawPath;
    }
    p = p.replace(/^\.\//, '');
    if (!p) return whole;
    const resolved = resolve(p);
    if (!resolved) {
      if (/\.md$/i.test(p) || !/\.[a-z0-9]{1,5}$/i.test(p)) onBroken(target);
      return whole;
    }
    return `[${text}](${resolved}${anchor ? '#' + anchor : ''}${title})`;
  });
}

export function frontmatter(obj) {
  const lines = Object.entries(obj)
    .filter(([, v]) => v !== undefined && v !== null && v !== '')
    .map(([k, v]) => `${k}: ${JSON.stringify(v)}`);
  return `---\n${lines.join('\n')}\n---\n\n`;
}

// Убрать существующий frontmatter из исходного файла, если он есть
export function stripFrontmatter(md) {
  return md.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
}
