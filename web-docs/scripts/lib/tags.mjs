// Автоматические теги репозитория: языки по расширениям файлов + технологии по упоминаниям в тексте
const EXT = {
  rs: 'Rust', ts: 'TypeScript', tsx: 'TypeScript', js: 'JavaScript', mjs: 'JavaScript', cjs: 'JavaScript', jsx: 'JavaScript',
  py: 'Python', go: 'Go', java: 'Java', kt: 'Kotlin', swift: 'Swift', rb: 'Ruby', php: 'PHP', cs: 'C#',
  cpp: 'C++', cc: 'C++', hpp: 'C++', c: 'C', h: 'C', vue: 'Vue', svelte: 'Svelte', sol: 'Solidity', dart: 'Dart',
  scala: 'Scala', ex: 'Elixir', exs: 'Elixir', lua: 'Lua', zig: 'Zig',
};
const FILES = { 'cargo.toml': 'Rust', 'package.json': 'Node.js', dockerfile: 'Docker', 'docker-compose.yml': 'Docker', 'go.mod': 'Go', 'pyproject.toml': 'Python', 'requirements.txt': 'Python', 'tauri.conf.json': 'Tauri' };
const MENTIONS = [
  ['React', /\bReact(\.js)?\b|\.jsx\b/], ['Vue', /\bvue(\.js)?\b/i], ['Angular', /\bangular\b/i], ['Svelte', /\bsvelte\b/i],
  ['Next.js', /\bnext\.js\b/i], ['Nuxt', /\bnuxt\b/i], ['Node.js', /\bnode\.js\b/i], ['Express', /\bexpress\.js\b|\bexpress\(\)/i],
  ['FastAPI', /\bfastapi\b/i], ['Django', /\bdjango\b/i], ['Flask', /\bflask\b/i], ['Spring', /\bspring boot\b/i],
  ['Tauri', /\btauri\b/i], ['Electron', /\belectron\b/i], ['Tokio', /\btokio\b/i], ['Axum', /\baxum\b/i], ['Actix', /\bactix\b/i],
  ['Docker', /\bdocker\b/i], ['Kubernetes', /\bkubernetes\b|\bk8s\b/i], ['Nginx', /\bnginx\b/i],
  ['PostgreSQL', /\bpostgres(ql)?\b/i], ['MySQL', /\bmysql\b/i], ['MongoDB', /\bmongo(db)?\b/i], ['Redis', /\bredis\b/i],
  ['SQLite', /\bsqlite\b/i], ['GraphQL', /\bgraphql\b/i], ['gRPC', /\bgrpc\b/i], ['WebSocket', /\bwebsockets?\b/i],
  ['REST API', /\brest(ful)? api\b|\brest-api\b/i], ['CLI', /\bcli\b/i], ['Prometheus', /\bprometheus\b/i],
  ['Blockchain', /blockchain|блокчейн/i], ['Cryptography', /cryptograph|криптограф/i], ['P2P', /\bp2p\b|peer-to-peer/i],
  ['Machine Learning', /machine learning|машинн\S* обучени/i], ['LLM', /\bllms?\b/i],
];
const MAX_TAGS = 6;

export function autoTags(text, files) {
  const score = new Map();
  const add = (tag, n) => score.set(tag, (score.get(tag) || 0) + n);
  for (const f of files) {
    const base = f.toLowerCase().split('/').pop();
    if (FILES[base]) add(FILES[base], 6);
    const ext = base.includes('.') ? base.split('.').pop() : '';
    if (EXT[ext]) add(EXT[ext], 3);
  }
  for (const [tag, re] of MENTIONS) {
    const n = (text.match(new RegExp(re.source, re.flags + 'g')) || []).length;
    if (n >= 2) add(tag, Math.min(n, 20));
  }
  return [...score.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, MAX_TAGS)
    .map(([tag]) => tag);
}
