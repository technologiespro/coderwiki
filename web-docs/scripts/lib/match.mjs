// Сопоставление глав между языками по общим признакам (без ИИ):
// исходные файлы, идентификаторы из кода, узлы mermaid-диаграмм и относительная позиция главы
const PATH_RE = /^[\w.@~-]+(\/[\w.@~-]+)*\.[a-z0-9]{1,8}$/i;
const IDENT_RE = /^[A-Za-z_][\w:.$-]*(\(\))?$/;
const MERMAID_SKIP = new Set(['graph', 'flowchart', 'subgraph', 'classdef', 'class', 'style', 'linkstyle', 'click', 'end', 'td', 'lr', 'tb', 'rl', 'bt']);

function addPath(set, p) {
  const v = p.toLowerCase().replace(/^\.?\//, '');
  set.add(v);
  set.add(v.split('/').pop());
}

export function features(md, extraFiles = []) {
  const files = new Set();
  const idents = new Set();
  const nodes = new Set();
  for (const f of extraFiles) addPath(files, f);
  const code = md.replace(/```mermaid[\s\S]*?```/g, (block) => {
    for (const m of block.matchAll(/\b([A-Za-z_]\w*)\s*(\[|\(|\{|>)/g)) {
      const id = m[1].toLowerCase();
      if (!MERMAID_SKIP.has(id)) nodes.add(id);
    }
    return '';
  });
  for (const m of code.matchAll(/`([^`\n]{2,120})`/g)) {
    const tok = m[1].trim();
    if (PATH_RE.test(tok) && (tok.includes('/') || /\.[a-z]{1,6}$/i.test(tok))) addPath(files, tok);
    else if (IDENT_RE.test(tok) && tok.length >= 3) idents.add(tok.replace(/\(\)$/, '').toLowerCase());
  }
  // Латиница в заголовках: только «технические» слова (цифры, CAPS, camelCase, snake_case)
  for (const h of code.matchAll(/^#{1,4}\s+(.+)$/gm)) {
    for (const w of h[1].matchAll(/[A-Za-z][\w-]{1,}/g)) {
      const t = w[0];
      if (/\d|_/.test(t) || /^[A-Z]{2,}/.test(t) || /[a-z][A-Z]/.test(t)) idents.add(t.toLowerCase());
    }
  }
  return { files, idents, nodes };
}

function jaccard(a, b) {
  if (!a.size && !b.size) return null;
  let inter = 0;
  for (const x of a) if (b.has(x)) inter++;
  return inter / (a.size + b.size - inter);
}

const WEIGHTS = [['files', 0.55], ['idents', 0.3], ['nodes', 0.15]];

function similarity(a, b) {
  let s = 0;
  let w = 0;
  for (const [k, wt] of WEIGHTS) {
    const j = jaccard(a[k], b[k]);
    if (j === null) continue;
    s += j * wt;
    w += wt;
  }
  return w ? s / w : 0;
}

const pos = (i, n) => (n > 1 ? i / (n - 1) : 0);
const MIN_CONTENT = 0.06;

// Пара языков: жадное взаимно-однозначное сопоставление по убыванию оценки
function matchPair(A, B) {
  const pairs = [];
  A.forEach((a, i) =>
    B.forEach((b, j) => {
      const content = similarity(a._f, b._f);
      if (content < MIN_CONTENT) return;
      pairs.push({ i, j, score: content * 0.8 + 0.2 * (1 - Math.abs(pos(i, A.length) - pos(j, B.length))) });
    })
  );
  pairs.sort((x, y) => y.score - x.score);
  const ab = new Map();
  const usedB = new Set();
  for (const p of pairs) {
    if (ab.has(p.i) || usedB.has(p.j)) continue;
    ab.set(p.i, { j: p.j, how: 'meaning' });
    usedB.add(p.j);
  }
  // Запасной вариант — та же позиция по номеру, если она свободна
  A.forEach((a, i) => {
    if (ab.has(i)) return;
    const j = B.findIndex((b, k) => !usedB.has(k) && a.num != null && b.num === a.num);
    if (j >= 0) {
      ab.set(i, { j, how: 'number' });
      usedB.add(j);
    }
  });
  return ab;
}

const MIN_NEAR = 0.03;

function nearest(from, to, code) {
  let n = 0;
  for (const a of from) {
    if (a.alt[code]) continue;
    let best = null;
    let bestScore = MIN_NEAR;
    for (const b of to) {
      const sc = similarity(a._f, b._f);
      if (sc >= bestScore) [best, bestScore] = [b, sc];
    }
    if (best) {
      a.alt[code] = best.path;
      n++;
    }
  }
  return n;
}

// Для каждой главы заполняет alt: { код языка → путь } ; без пары — обзор репозитория
export function linkChapters(langs) {
  const codes = Object.keys(langs);
  const stats = [];
  for (const v of Object.values(langs)) for (const c of v.chapters) c.alt = {};
  for (let x = 0; x < codes.length; x++) {
    for (let y = x + 1; y < codes.length; y++) {
      const A = langs[codes[x]];
      const B = langs[codes[y]];
      const ab = matchPair(A.chapters, B.chapters);
      const st = { pair: `${codes[x]}↔${codes[y]}`, meaning: 0, number: 0, none: 0, moved: 0 };
      A.chapters.forEach((a, i) => {
        const m = ab.get(i);
        if (!m) {
          st.none++;
          return;
        }
        const b = B.chapters[m.j];
        a.alt[codes[y]] = b.path;
        b.alt[codes[x]] = a.path;
        st[m.how]++;
        if (a.num !== b.num) st.moved++;
      });
      // Оставшимся без пары — ближайшая по смыслу глава (может совпасть с уже занятой)
      st.near = nearest(A.chapters, B.chapters, codes[y]) + nearest(B.chapters, A.chapters, codes[x]);
      for (const c of A.chapters) c.alt[codes[y]] ??= B.path;
      for (const c of B.chapters) c.alt[codes[x]] ??= A.path;
      stats.push(st);
    }
  }
  return stats;
}
