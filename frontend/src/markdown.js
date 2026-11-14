import { Marked } from 'marked';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/common';
import 'highlight.js/styles/github-dark.css';
import { store } from './store';

export const md = new Marked(
  markedHighlight({
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      if (lang === 'mermaid') return code;
      if (lang && hljs.getLanguage(lang)) {
        return hljs.highlight(code, { language: lang }).value;
      }
      return hljs.highlightAuto(code).value;
    },
  })
);

export function sanitizeMermaid(code) {
  const quote = (label) => `"${label.replace(/"/g, "'").trim()}"`;
  let out = code
    .split('\n')
    .filter((l) => l.trim() !== '')
    .map((l) => l.replace(/<br\s*\/?>/gi, ' '))
    .join('\n');
  // Кавычные ID узлов невалидны в mermaid: "Vue App"["..."] → n1_vue_app["..."]
  const ids = new Map();
  const slug = (name) => {
    if (!ids.has(name)) {
      ids.set(
        name,
        `n${ids.size + 1}_${name.toLowerCase().replace(/[^a-z0-9]+/gi, '_').replace(/^_+|_+$/g, '').slice(0, 30)}`
      );
    }
    return ids.get(name);
  };
  out = out.replace(/"([^"\n]+)"\s*(?=[[({])/g, (m, name) => slug(name));
  out = out.replace(/(^|[\s;])"([^"\n]+)"(?=\s*(-->|---|-\.|==|--|<--|o--|$))/gm, (m, pre, name) => pre + slug(name));
  out = out.replace(/\[([^\][{}()"]+)\]/g, (m, label) =>
    /^[A-Za-z0-9_\s.-]+$/.test(label) ? m : `[${quote(label)}]`
  );
  out = out.replace(/\(([^()[\]{}"]+)\)/g, (m, label) =>
    /^[A-Za-z0-9_\s.-]+$/.test(label) ? m : `(${quote(label)})`
  );
  out = out.replace(/\{([^{}[\]()"]+)\}/g, (m, label) =>
    /^[A-Za-z0-9_\s.-]+$/.test(label) ? m : `{${quote(label)}}`
  );
  return out;
}

// Блоки без тега mermaid, но с mermaid-синтаксисом внутри
export function looksLikeMermaid(text) {
  return /^\s*(graph\s+(TD|TB|LR|RL|BT)\b|flowchart\s+|sequenceDiagram|classDiagram|erDiagram|stateDiagram)/.test(text);
}

export function linkifyPaths(rootEl) {
  if (!store.docs || !rootEl) return;
  const fileSet = new Set(store.docs.files || Object.keys(store.docs.docs || {}));
  for (const c of rootEl.querySelectorAll('code')) {
    if (c.closest('pre')) continue;
    const t = c.textContent.trim();
    if (fileSet.has(t)) {
      c.classList.add('file-link');
      c.setAttribute('data-testid', `file-link-${t}`);
      c.onclick = () => {
        store.selectedFile = t;
      };
    }
  }
}

export function exportSvgToPng(svgEl, filename, bg) {
  if (!svgEl) return;
  const clone = svgEl.cloneNode(true);
  const vb = svgEl.viewBox && svgEl.viewBox.baseVal;
  const w = vb && vb.width ? vb.width : svgEl.getBoundingClientRect().width || 800;
  const h = vb && vb.height ? vb.height : svgEl.getBoundingClientRect().height || 600;
  clone.setAttribute('width', w);
  clone.setAttribute('height', h);
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  const url = URL.createObjectURL(
    new Blob([new XMLSerializer().serializeToString(clone)], { type: 'image/svg+xml;charset=utf-8' })
  );
  const img = new Image();
  img.onload = () => {
    const scale = 2;
    const canvas = document.createElement('canvas');
    canvas.width = w * scale;
    canvas.height = h * scale;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.scale(scale, scale);
    ctx.drawImage(img, 0, 0, w, h);
    URL.revokeObjectURL(url);
    canvas.toBlob((blob) => {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = filename;
      a.click();
      URL.revokeObjectURL(a.href);
    });
  };
  img.src = url;
}
