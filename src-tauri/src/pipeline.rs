// Пайплайн генерации wiki: план разделов → страницы разделов → эмбеддинги → обзор
use crate::lm;
use crate::repo;
use crate::state::{get_json, get_str, set_json, set_str};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

pub const MERMAID_RULES: &str = "In every mermaid diagram: node IDs MUST be short latin identifiers without spaces, dots, quotes or special characters (a1, store, apiRouter). Human-readable text goes ONLY inside the node label in double quotes, e.g. a1[\"Vue application\"]. NEVER write a quoted string as a node ID (WRONG: \"Vue App\"[\"...\"]; CORRECT: vueApp[\"Vue App\"]). Never use emojis, slashes, parentheses or backticks inside labels. Edge labels also in double quotes: a1 -->|\"loads\"| b2.";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub status: String,
    pub current_file: String,
    pub processed: usize,
    pub total: usize,
    pub partial: String,
    pub cached_hits: usize,
    pub error: String,
    pub job_id: String,
}

pub fn emit(app: &tauri::AppHandle, p: &Progress) {
    let _ = app.emit("gen-progress", p);
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenParams {
    pub repo_url: Option<String>,
    pub name: Option<String>,
    pub files: Option<Vec<LocalFile>>,
    pub language: Option<String>,
    pub model_id: String,
    pub lm_url: Option<String>,
    pub embedding_model: Option<String>,
    pub max_files: Option<usize>,
    pub max_tokens: Option<usize>,
}

#[derive(Deserialize, Clone)]
pub struct LocalFile {
    pub path: String,
    pub content: String,
}

pub fn slice_by_tokens(text: &str, tokens: usize) -> String {
    let max_chars = tokens.saturating_mul(7) / 2; // ~3.5 символа на токен
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

pub fn first_line(content: &str) -> String {
    content
        .lines()
        .find(|l| l.trim().len() > 2)
        .unwrap_or("")
        .trim()
        .chars()
        .take(80)
        .collect()
}

fn slugify(t: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in t.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.chars().count() >= 50 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "section".into() } else { out }
}

fn sha(data: &str) -> String {
    let mut h = Sha256::new();
    h.update(data.as_bytes());
    format!("{:x}", h.finalize())
}

fn parse_section_plan(raw: &str, files: &[String]) -> Option<Vec<(String, Vec<String>)>> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    let arr: Value = serde_json::from_str(&raw[start..=end]).ok()?;
    let arr = arr.as_array()?;
    let known: std::collections::HashSet<&String> = files.iter().collect();
    let mut used: std::collections::HashSet<String> = Default::default();
    let mut plan: Vec<(String, Vec<String>)> = Vec::new();
    for s in arr {
        let title = s["title"].as_str()?.trim().chars().take(120).collect::<String>();
        let fl: Vec<String> = s["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(String::from))
                    .filter(|f| known.contains(f) && !used.contains(f))
                    .collect()
            })
            .unwrap_or_default();
        for f in &fl {
            used.insert(f.clone());
        }
        if !fl.is_empty() {
            plan.push((title, fl));
        }
    }
    let rest: Vec<String> = files.iter().filter(|f| !used.contains(*f)).cloned().collect();
    if !rest.is_empty() && !plan.is_empty() {
        plan.push(("Прочие файлы".into(), rest));
    }
    if plan.is_empty() { None } else { Some(plan) }
}

fn fallback_sections(files: &[String]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for f in files {
        let top = if let Some(i) = f.find('/') { f[..i].to_string() } else { "Корень проекта".to_string() };
        let title = if top == "Корень проекта" { top.clone() } else { format!("Модуль {}", top) };
        if let Some(g) = groups.iter_mut().find(|(t, _)| *t == title) {
            g.1.push(f.clone());
        } else {
            groups.push((title, vec![f.clone()]));
        }
    }
    groups
}

fn build_section_context(sec_files: &[String], contents: &HashMap<String, String>, budget_tokens: usize) -> String {
    let per_file = (budget_tokens / sec_files.len().max(1)).max(400);
    let mut ctx = String::new();
    for f in sec_files {
        let code = slice_by_tokens(contents.get(f).map(String::as_str).unwrap_or(""), per_file);
        ctx.push_str(&format!("### {}\n```\n{}\n```\n\n", f, code));
    }
    ctx
}

pub fn chunk_code(content: &str) -> Vec<(String, usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut s = 0;
    while s < lines.len() {
        let e = (s + 60).min(lines.len());
        let text: String = lines[s..e].join("\n");
        if text.len() >= 30 {
            chunks.push((text.chars().take(4000).collect(), s + 1, e));
        }
        s = e;
    }
    chunks
}

#[allow(clippy::too_many_lines)]
pub async fn run_generate(
    app: tauri::AppHandle,
    db: sled::Db,
    data_dir: std::path::PathBuf,
    cancel: Arc<AtomicBool>,
    params: GenParams,
) {
    cancel.store(false, Ordering::SeqCst);
    let mut p = Progress { status: "queued".into(), job_id: "desktop".into(), ..Default::default() };
    emit(&app, &p);
    match run_inner(&app, &db, &data_dir, &cancel, &params, &mut p).await {
        Ok(()) => {}
        Err(e) => {
            p.status = "error".into();
            p.error = e;
            emit(&app, &p);
        }
    }
}

async fn run_inner(
    app: &tauri::AppHandle,
    db: &sled::Db,
    data_dir: &std::path::PathBuf,
    cancel: &Arc<AtomicBool>,
    params: &GenParams,
    p: &mut Progress,
) -> Result<(), String> {
    let language = params.language.clone().unwrap_or_else(|| "russian".into());
    let max_files = params.max_files.unwrap_or(50);
    let max_tokens = params.max_tokens.unwrap_or(8000);
    let lm_url = params.lm_url.as_deref();
    let model = params.model_id.as_str();

    // Клонирование или локальные файлы
    let (repo_key, files_with_content): (String, Vec<(String, String)>) = if let Some(files) = &params.files {
        let key = format!("local/{}", params.name.clone().unwrap_or_else(|| "folder".into()));
        let mut list: Vec<(String, String)> = files
            .iter()
            .filter(|f| !f.content.is_empty() && f.content.len() <= 200 * 1024)
            .map(|f| (f.path.replace('\\', "/"), f.content.clone()))
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        list.truncate(max_files);
        (key, list)
    } else {
        let url = params.repo_url.clone().ok_or("repoUrl обязателен")?;
        p.status = "cloning".into();
        p.current_file = url.clone();
        emit(app, p);
        let repos_dir = data_dir.join("repos");
        let (key, dir) = tauri::async_runtime::spawn_blocking(move || repo::clone_repo(&url, &repos_dir))
            .await
            .map_err(|e| e.to_string())??;
        p.status = "parsing".into();
        emit(app, p);
        let list = repo::collect_files(&dir, max_files);
        (key, list)
    };
    if files_with_content.is_empty() {
        return Err("Не найдено подходящих текстовых файлов".into());
    }

    let files: Vec<String> = files_with_content.iter().map(|(f, _)| f.clone()).collect();
    let mut contents: HashMap<String, String> = HashMap::new();
    let mut changed: std::collections::HashSet<String> = Default::default();
    for (f, c) in &files_with_content {
        let key = format!("src:{}:{}", repo_key, f);
        if get_str(db, &key).as_deref() != Some(c.as_str()) {
            changed.insert(f.clone());
        }
        set_str(db, &key, c);
        contents.insert(f.clone(), c.clone());
    }

    // Этап 1: план разделов
    p.status = "sectioning".into();
    p.current_file = "Планирование разделов wiki".into();
    emit(app, p);
    let summaries: String = files
        .iter()
        .map(|f| format!("{}: {}", f, first_line(contents.get(f).unwrap())))
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(20000)
        .collect();
    let plan_key = format!("cache:{}", sha(&format!("plan::{}::{}::{}", files.join(","), language, model)));
    let mut plan: Option<Vec<(String, Vec<String>)>> = get_json(db, &plan_key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some((
                        s["title"].as_str()?.to_string(),
                        s["files"].as_array()?.iter().filter_map(|f| f.as_str().map(String::from)).collect(),
                    ))
                })
                .collect()
        })
    });
    if plan.is_none() {
        let messages = json!([
            { "role": "system", "content": format!("You are an expert software architect organizing a documentation wiki (like DeepWiki). Group the repository files into 4-10 thematic documentation sections (chapters of a wiki), ordered from high-level (overview, entry points, architecture) to specific (utilities, configs). Every file must belong to EXACTLY ONE section. Section titles must be short, human-readable and strictly in {}. Respond with ONLY a valid JSON array, no markdown fences, no comments, no extra text: [{{\"title\":\"...\",\"files\":[\"path/one\",\"path/two\"]}}]", language) },
            { "role": "user", "content": format!("Repository: {}\n\nFiles (path: first line hint):\n{}", repo_key, summaries) }
        ]);
        let raw = lm::chat_once(lm_url, model, &messages).await.unwrap_or_default();
        let parsed = parse_section_plan(&raw, &files).unwrap_or_else(|| fallback_sections(&files));
        let as_json: Value = Value::Array(
            parsed.iter().map(|(t, fl)| json!({ "title": t, "files": fl })).collect(),
        );
        set_json(db, &plan_key, &as_json);
        plan = Some(parsed);
    }
    let plan = plan.unwrap();

    // Этап 2: генерация страниц разделов
    p.status = "generating".into();
    p.total = plan.len();
    p.processed = 0;
    emit(app, p);
    let mut sections: Vec<Value> = Vec::new();
    for (i, (title, sec_files)) in plan.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("Отменено пользователем".into());
        }
        p.current_file = title.clone();
        p.partial = String::new();
        emit(app, p);
        let code_ctx = build_section_context(sec_files, &contents, max_tokens.saturating_sub(2500));
        let joined: String = sec_files.iter().map(|f| contents.get(f).cloned().unwrap_or_default()).collect::<Vec<_>>().join("\u{0}");
        let cache_key = format!("cache:{}", sha(&format!("section::{}::{}::{}::{}", title, joined, language, model)));
        let md_text = if let Some(cached) = get_str(db, &cache_key) {
            p.cached_hits += 1;
            cached
        } else {
            let messages = json!([
                { "role": "system", "content": format!("You are an expert software architect writing one section of a documentation wiki for the \"{repo_key}\" repository. Write a comprehensive, well-structured documentation page for the section \"{title}\" based on the provided source code. Structure: 1) start with a heading \"# {title}\"; 2) several paragraphs explaining the purpose and role of this part of the system; 3) EXACTLY ONE ```mermaid code block (graph TD syntax) showing relationships between the components of this section. {rules} 4) a Markdown table of key modules with columns Module | Purpose | Key file; 5) detailed subsections (## headings) describing the important components, their APIs, data flow and interactions. Reference every file path in backticks like `src/app.js`. Output ONLY valid Markdown, strictly in {language}.", repo_key = repo_key, title = title, rules = MERMAID_RULES, language = language) },
                { "role": "user", "content": format!("Section: {}\n\nSource code of the section files:\n\n{}", title, code_ctx) }
            ]);
            let app2 = app.clone();
            let mut snapshot = p.clone();
            let mut last = std::time::Instant::now();
            let text = lm::chat_stream(lm_url, model, &messages, |_d| {
                if last.elapsed().as_millis() > 400 {
                    last = std::time::Instant::now();
                    snapshot.partial.clear();
                    emit(&app2, &snapshot);
                }
            })
            .await?;
            set_str(db, &cache_key, &text);
            text
        };
        sections.push(json!({
            "id": format!("{:02}-{}", i + 1, slugify(title)),
            "title": title,
            "files": sec_files,
            "md": md_text,
        }));
        p.processed += 1;
        p.partial = String::new();
        emit(app, p);
    }

    // Этап 3: эмбеддинги (через LM Studio embeddings API, если указана модель)
    if let Some(em) = params.embedding_model.as_deref().filter(|m| !m.is_empty()) {
        p.status = "embedding".into();
        emit(app, p);
        for f in &files {
            if cancel.load(Ordering::SeqCst) {
                return Err("Отменено пользователем".into());
            }
            let has_emb = db.scan_prefix(format!("emb:{}:{}:", repo_key, f)).next().is_some();
            if has_emb && !changed.contains(f) {
                continue;
            }
            crate::state::del_prefix(db, &format!("emb:{}:{}:", repo_key, f));
            p.current_file = f.clone();
            emit(app, p);
            let chunks = chunk_code(contents.get(f).unwrap());
            if chunks.is_empty() {
                continue;
            }
            let texts: Vec<String> = chunks.iter().map(|(t, _, _)| t.clone()).collect();
            match lm::embeddings(lm_url, em, &texts).await {
                Ok(vectors) => {
                    for (i, (text, s, e)) in chunks.iter().enumerate() {
                        if let Some(v) = vectors.get(i).filter(|v| !v.is_empty()) {
                            set_json(
                                db,
                                &format!("emb:{}:{}:{}", repo_key, f, i),
                                &json!({ "text": text, "startLine": s, "endLine": e, "vector": v }),
                            );
                        }
                    }
                }
                Err(_) => break, // эмбеддинги необязательны
            }
        }
    }

    // Этап 4: обзор
    p.status = "overview".into();
    p.current_file = String::new();
    emit(app, p);
    let excerpts: String = sections
        .iter()
        .map(|s| format!("### {}\n{}", s["title"].as_str().unwrap_or(""), s["md"].as_str().unwrap_or("").chars().take(500).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n\n")
        .chars()
        .take(24000)
        .collect();
    let ov_messages = json!([
        { "role": "system", "content": format!("You are an expert software architect. Based on the documentation section excerpts provided, write a comprehensive high-level overview of the repository. Structure it as follows: 1) A short opening paragraph explaining what the project is and its purpose; 2) \"Purpose and audience\" section describing target users and use cases; 3) \"Key features\" section as a Markdown table with columns Feature | Description | Key files; 4) \"Architecture\" section with EXACTLY ONE ```mermaid code block (graph TD syntax) showing the relationships between the main modules. {} 5) \"Key modules\" section briefly describing the most important parts. Reference file paths in backticks. Output ONLY valid Markdown. Write the overview strictly in {}.", MERMAID_RULES, language) },
        { "role": "user", "content": format!("Repository: {}\n\nSection excerpts:\n{}", repo_key, excerpts) }
    ]);
    let overview = lm::chat_once(lm_url, model, &ov_messages).await.unwrap_or_default();

    // Зависимости, сводки папок, diff изменений
    let deps = repo::extract_deps(&files, &contents);
    let mut dir_summaries = serde_json::Map::new();
    for f in &files {
        let dir = f.rfind('/').map(|i| f[..i].to_string()).unwrap_or_else(|| ".".into());
        let line = format!("- {}: {}\n", f, first_line(contents.get(f).unwrap()));
        dir_summaries
            .entry(dir)
            .and_modify(|v| {
                if let Value::String(s) = v {
                    s.push_str(&line);
                }
            })
            .or_insert(Value::String(line));
    }
    let prev = get_json(db, &format!("repo:{}", repo_key));
    let changes = prev.as_ref().and_then(|pr| {
        let prev_files: Vec<String> = pr["files"]
            .as_array()
            .map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if prev_files.is_empty() {
            return None;
        }
        let added: Vec<&String> = files.iter().filter(|f| !prev_files.contains(f)).collect();
        let modified: Vec<&String> = files.iter().filter(|f| prev_files.contains(f) && changed.contains(*f)).collect();
        let removed: Vec<&String> = prev_files.iter().filter(|f| !files.contains(f)).collect();
        Some(json!({ "added": added, "modified": modified, "removed": removed, "updatedAt": now_iso() }))
    });

    let record = json!({
        "repoKey": repo_key,
        "generatedAt": now_iso(),
        "language": language,
        "modelId": model,
        "overview": overview,
        "files": files,
        "sections": sections,
        "tree": repo::build_tree(&files),
        "deps": deps,
        "dirSummaries": Value::Object(dir_summaries),
        "changes": changes,
    });
    set_json(db, &format!("repo:{}", repo_key), &record);

    // .md на диск в data/docs/
    let out_dir = crate::state::repo_docs_dir(data_dir, &repo_key);
    let _ = std::fs::remove_dir_all(&out_dir);
    let _ = std::fs::create_dir_all(&out_dir);
    for s in &sections {
        let _ = std::fs::write(
            out_dir.join(format!("{}.md", s["id"].as_str().unwrap_or("section"))),
            s["md"].as_str().unwrap_or(""),
        );
    }
    if !overview.is_empty() {
        let _ = std::fs::write(out_dir.join("_OVERVIEW.md"), &overview);
    }

    p.status = "done".into();
    p.current_file = String::new();
    emit(app, p);
    Ok(())
}

pub fn now_iso() -> String {
    let d = SystemTimeIso::now();
    d.0
}

// ISO-8601 без внешних крейтов (UTC)
struct SystemTimeIso(String);
impl SystemTimeIso {
    fn now() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let days = secs / 86400;
        let (h, m, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
        // григорианский календарь из дней с 1970-01-01
        let mut year = 1970i64;
        let mut rem = days as i64;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let len = if leap { 366 } else { 365 };
            if rem < len {
                break;
            }
            rem -= len;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut month = 0usize;
        while rem >= months[month] {
            rem -= months[month];
            month += 1;
        }
        Self(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
            year, month + 1, rem + 1, h, m, s
        ))
    }
}
