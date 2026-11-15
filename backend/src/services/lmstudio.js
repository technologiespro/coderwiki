function baseUrl(url) {
  const u = (url && url.trim()) || process.env.LMSTUDIO_URL;
  return u.replace(/\/+$/, '');
}

function lmError(message) {
  const err = new Error(message);
  err.status = 502;
  return err;
}

export async function fetchModels(lmUrl) {
  let res;
  try {
    res = await fetch(`${baseUrl(lmUrl)}/v1/models`, { signal: AbortSignal.timeout(10000) });
  } catch (e) {
    throw lmError(`LM Studio недоступен (${baseUrl(lmUrl)}): ${e.message}`);
  }
  if (!res.ok) throw lmError(`LM Studio вернул ошибку ${res.status}`);
  const data = await res.json();
  return data.data || [];
}

export async function getEmbeddings({ lmUrl, model, input, signal }) {
  const timeout = AbortSignal.timeout(120000);
  let res;
  try {
    res = await fetch(`${baseUrl(lmUrl)}/v1/embeddings`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model, input }),
      signal: signal ? AbortSignal.any([timeout, signal]) : timeout,
    });
  } catch (e) {
    throw lmError(`LM Studio embeddings недоступен: ${e.message}`);
  }
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw lmError(`LM Studio embeddings ошибка ${res.status}: ${text.slice(0, 200)}`);
  }
  const data = await res.json();
  return (data.data || []).sort((a, b) => a.index - b.index).map((d) => d.embedding);
}

export async function chatStream({ lmUrl, modelId, messages, onChunk, signal }) {
  const body = { model: modelId, messages, temperature: 0.3, stream: true };
  const timeout = AbortSignal.timeout(600000);
  let res;
  try {
    res = await fetch(`${baseUrl(lmUrl)}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: signal ? AbortSignal.any([timeout, signal]) : timeout,
    });
  } catch (e) {
    throw lmError(`LM Studio недоступен (${baseUrl(lmUrl)}): ${e.message}`);
  }
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw lmError(`LM Studio ошибка ${res.status}: ${text.slice(0, 300)}`);
  }
  const ctype = res.headers.get('content-type') || '';
  if (!ctype.includes('text/event-stream')) {
    const data = await res.json();
    const content = data.choices?.[0]?.message?.content;
    if (!content) throw lmError('LM Studio вернул пустой ответ');
    return content;
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let full = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop();
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed.startsWith('data:')) continue;
      const payload = trimmed.slice(5).trim();
      if (payload === '[DONE]') continue;
      try {
        const json = JSON.parse(payload);
        const delta = json.choices?.[0]?.delta?.content || '';
        if (delta) {
          full += delta;
          onChunk && onChunk(full, delta);
        }
      } catch {
        /* неполный JSON-чанк */
      }
    }
  }
  if (!full) throw lmError('LM Studio вернул пустой ответ');
  return full;
}

import { estimateTokens, splitByTokens, getMaxTokens } from '../utils/tokenManager.js';

export async function generateDocSafe(opts) {
  const limit = getMaxTokens(opts.maxTokens) - 1000;
  if (estimateTokens(opts.fileContent) <= limit) return generateDoc(opts);
  const parts = splitByTokens(opts.fileContent, limit);
  const results = [];
  for (let i = 0; i < parts.length; i++) {
    results.push(
      await generateDoc({
        ...opts,
        filePath: `${opts.filePath} (part ${i + 1}/${parts.length})`,
        fileContent: parts[i],
      })
    );
  }
  return results.join('\n\n---\n\n');
}

const MERMAID_RULES =
  'In every mermaid diagram: node IDs MUST be short latin identifiers without spaces, dots, quotes or special characters (a1, store, apiRouter). Human-readable text goes ONLY inside the node label in double quotes, e.g. a1["Vue application"]. NEVER write a quoted string as a node ID (WRONG: "Vue App"["..."]; CORRECT: vueApp["Vue App"]). Never use emojis, slashes, parentheses or backticks inside labels. Edge labels also in double quotes: a1 -->|"loads"| b2.';

export async function generateDoc({ lmUrl, modelId, language, filePath, fileContent, onChunk, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are an expert software architect. Analyze the provided code and write comprehensive technical documentation. If the file has meaningful internal structure or relationships, include ONE small mermaid diagram in a \`\`\`mermaid code block (graph TD syntax) illustrating its key components or data flow; skip the diagram for trivial files. ${MERMAID_RULES} Output ONLY valid Markdown. Write the documentation strictly in ${language}.`,
      },
      { role: 'user', content: `File path: ${filePath}\n\nCode:\n${fileContent}` },
    ],
  });
}

// План разделов wiki: LLM группирует файлы в тематические разделы (JSON)
export async function planSections({ lmUrl, modelId, language, repoKey, fileSummaries, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are an expert software architect organizing a documentation wiki (like DeepWiki). Organize the repository files into a TWO-LEVEL wiki structure: 3-7 chapters, each containing 1-4 pages. The FIRST chapter must be the project overview chapter: its first page is a high-level overview and, whenever a README or manifest/config file exists, its second page is a getting-started page (installation, configuration, usage examples) grouping the README and manifest files. Order chapters from high-level (overview, architecture) to specific (utilities, configs). Every other file must belong to EXACTLY ONE page. Chapter and page titles must be short, human-readable and strictly in ${language}. Respond with ONLY a valid JSON array, no markdown fences, no comments, no extra text: [{"chapter":"...","sections":[{"title":"...","files":["path/one","path/two"]}]}]`,
      },
      { role: 'user', content: `Repository: ${repoKey}\n\nFiles (path: first line hint):\n${fileSummaries}` },
    ],
  });
}

// Страница раздела wiki по исходному коду его файлов
export async function generateSectionDoc({ lmUrl, modelId, language, repoKey, title, codeContext, isFirst, projectHint, onChunk, signal }) {
  // Первая страница wiki должна начинаться с назначения проекта
  const firstRule = isFirst
    ? ' This is the FIRST page of the wiki: BEFORE any implementation or configuration details, open with 2-3 paragraphs that clearly identify WHAT this project is, what problem it solves, its purpose and intended users — infer this from the README, manifests and code.'
    : '';
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are an expert software architect writing one section of a documentation wiki for the "${repoKey}" repository. Write a comprehensive, well-structured documentation page for the section "${title}" based on the provided source code.${firstRule} Structure: 1) start with a heading "# ${title}"; 2) several paragraphs explaining the purpose and role of this part of the system; 3) EXACTLY ONE \`\`\`mermaid code block (graph TD syntax) showing relationships between the components of this section. ${MERMAID_RULES} 4) a Markdown table of key modules with columns Module | Purpose | Key file; 5) detailed subsections (## headings) describing the important components, their APIs, data flow and interactions. Reference every file path in backticks like \`src/app.js\`. Output ONLY valid Markdown, strictly in ${language}.`,
      },
      {
        role: 'user',
        content: `Section: ${title}${projectHint ? `\n\nProject README excerpt (use it to identify the project purpose):\n${projectHint}` : ''}\n\nSource code of the section files:\n\n${codeContext}`,
      },
    ],
  });
}

// Распределение новых файлов по существующим разделам (диф-обновление wiki)
export async function assignNewFiles({ lmUrl, modelId, sections, newFiles, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    signal,
    messages: [
      {
        role: 'system',
        content: 'You are organizing a documentation wiki. Assign each NEW file to exactly one of the EXISTING sections. Respond with ONLY a valid JSON object mapping file path to section title, no markdown fences, no comments: {"path/one":"Section Title"}',
      },
      {
        role: 'user',
        content: `Existing sections:\n${sections.map((s) => `- ${s.title}: ${s.files.slice(0, 8).join(', ')}`).join('\n')}\n\nNew files:\n${newFiles.join('\n')}`,
      },
    ],
  });
}

export async function generateOverview({ lmUrl, modelId, language, repoKey, excerpts, onChunk, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are an expert software architect. Based on the documentation section excerpts provided, write a comprehensive high-level overview of the repository. Structure it as follows: 1) A short opening paragraph explaining what the project is and its purpose; 2) "Purpose and audience" section describing target users and use cases; 3) "Key features" section as a Markdown table with columns Feature | Description | Key files; 4) "Architecture" section with EXACTLY ONE \`\`\`mermaid code block (graph TD syntax) showing the relationships between the main modules. ${MERMAID_RULES} 5) "Key modules" section briefly describing the most important parts. Reference file paths in backticks. Output ONLY valid Markdown. Write the overview strictly in ${language}.`,
      },
      { role: 'user', content: `Repository: ${repoKey}\n\nPer-file documentation excerpts:\n\n${excerpts}` },
    ],
  });
}

// Страница «Установка и использование»: практический гайд по README и манифестам (как deepwiki.com)
export async function generateInstallDoc({ lmUrl, modelId, language, repoKey, title, codeContext, onChunk, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are writing the getting-started page of a documentation wiki for the "${repoKey}" repository. Based PRIMARILY on the README and manifest/config files provided, write a practical installation and usage guide. Structure: 1) start with a heading "# ${title}"; 2) "Prerequisites" — required runtimes, tools and versions; 3) "Installation" — exact step-by-step commands (cloning, dependency installation) in \`\`\`bash code blocks; 4) "Configuration" — environment variables, config files and build scripts if any; 5) "Usage examples" — reproduce the real usage and code examples from the README accurately in code blocks; when the README has none, derive minimal examples from the public API; 6) "Running tests" if test scripts exist. Use ONLY facts found in the provided files — never invent commands, package names or versions. Reference file paths in backticks. Output ONLY valid Markdown, strictly in ${language}.`,
      },
      { role: 'user', content: `Section: ${title}\n\nSource files (README, manifests, configs):\n\n${codeContext}` },
    ],
  });
}

export async function generateIdeas({ lmUrl, modelId, language, repoKey, title, excerpts, onChunk, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are an experienced staff engineer and product strategist reviewing the "${repoKey}" repository. Based on the documentation section excerpts, write an "improvement ideas" page. Structure: 1) start with a heading "# ${title}"; 2) "Architecture and code" — refactoring and tech-debt suggestions; 3) "Performance and security" — potential optimizations and hardening; 4) "New features" as a Markdown table with columns Idea | Value | Complexity (S/M/L); 5) "Quick wins" — small improvements achievable in under a day. Be specific to THIS project, reference file paths in backticks. Output ONLY valid Markdown, strictly in ${language}.`,
      },
      { role: 'user', content: `Repository: ${repoKey}\n\nDocumentation section excerpts:\n\n${excerpts}` },
    ],
  });
}

export async function generateAnswer({ lmUrl, modelId, language, repoKey, context, question, history = [], onChunk, signal }) {
  return chatStream({
    lmUrl,
    modelId,
    onChunk,
    signal,
    messages: [
      {
        role: 'system',
        content: `You are a helpful expert assistant for the "${repoKey}" repository. Answer the user's questions using ONLY the provided documentation context. Reference relevant file paths in backticks. If the answer is not in the context, say so honestly. Answer in Markdown, strictly in ${language}.`,
      },
      ...history,
      { role: 'user', content: `Documentation context:\n\n${context}\n\nQuestion: ${question}` },
    ],
  });
}
