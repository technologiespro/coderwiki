export const MAX_CONTEXT_TOKENS = parseInt(process.env.MAX_CONTEXT_TOKENS, 10) || 8000;

export const estimateTokens = (t) => Math.ceil(String(t || '').length / 4);

export const getMaxTokens = (override) => {
  const v = parseInt(override, 10);
  return v > 500 ? v : MAX_CONTEXT_TOKENS;
};

export function sliceByTokens(text, maxTokens) {
  return String(text).slice(0, maxTokens * 4);
}

export function splitByTokens(text, maxTokens) {
  const maxChars = maxTokens * 4;
  const lines = String(text).split('\n');
  const parts = [];
  let cur = '';
  for (const line of lines) {
    if (cur.length + line.length + 1 > maxChars && cur) {
      parts.push(cur);
      cur = '';
    }
    cur += (cur ? '\n' : '') + line;
  }
  if (cur.trim()) parts.push(cur);
  return parts;
}

export function slidingHistory(history, maxTokens = 1500) {
  if (!history.length) return { history, truncated: false };
  const kept = [];
  let used = 0;
  for (let i = history.length - 1; i >= 0; i--) {
    const t = estimateTokens(history[i].content);
    if (used + t > maxTokens) break;
    used += t;
    kept.unshift(history[i]);
  }
  const truncated = kept.length < history.length;
  if (truncated && history[0] && !kept.includes(history[0])) {
    kept.unshift({ role: history[0].role, content: sliceByTokens(history[0].content, 200) });
  }
  return { history: kept, truncated };
}
