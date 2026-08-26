// Чат по репозиторию: RAG (векторный поиск в SLED), keyword-fallback, агентские инструменты
use crate::lm;
use crate::pipeline::slice_by_tokens;
use crate::state::{get_str, scan};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Emitter;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatPayload {
    pub repo: String,
    pub question: String,
    pub model_id: Option<String>,
    pub lm_url: Option<String>,
    pub embedding_model: Option<String>,
    pub history: Option<Vec<Value>>,
    pub mode: Option<String>,
    pub max_tokens: Option<usize>,
}

const PAGE_TOKENS: usize = 2500;

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f32, 0f32, 0f32);
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

struct Hit {
    path: String,
    start: u64,
    end: u64,
    text: String,
    score: f32,
}

async fn vector_search(db: &sled::Db, repo: &str, question: &str, lm_url: Option<&str>, em: &str) -> Vec<Hit> {
    let qv = match lm::embeddings(lm_url, em, &[question.to_string()]).await {
        Ok(mut v) if !v.is_empty() && !v[0].is_empty() => v.remove(0),
        _ => return Vec::new(),
    };
    let prefix = format!("emb:{}:", repo);
    let mut hits: Vec<Hit> = scan(db, &prefix)
        .into_iter()
        .filter_map(|(k, v)| {
            let val: Value = serde_json::from_slice(&v).ok()?;
            let vector: Vec<f32> = val["vector"].as_array()?.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            let rest = &k[prefix.len()..];
            let path = rest.rsplit_once(':').map(|(p, _)| p.to_string()).unwrap_or_else(|| rest.to_string());
            Some(Hit {
                path,
                start: val["startLine"].as_u64().unwrap_or(1),
                end: val["endLine"].as_u64().unwrap_or(1),
                text: val["text"].as_str().unwrap_or("").to_string(),
                score: cosine(&qv, &vector),
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(12);
    hits
}

// Скоринг разделов по ключевым словам (fallback без эмбеддингов)
fn keyword_context(record: &Value, question: &str) -> (String, Vec<Value>) {
    let words: Vec<String> = question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect();
    let empty = vec![];
    let sections = record["sections"].as_array().unwrap_or(&empty);
    let mut scored: Vec<(i64, &Value)> = sections
        .iter()
        .map(|s| {
            let name = s["title"].as_str().unwrap_or("").to_lowercase();
            let text = format!("{}\n{}", name, s["md"].as_str().unwrap_or("")).to_lowercase();
            let mut score = 0i64;
            for w in &words {
                score += text.matches(w.as_str()).count() as i64;
                if name.contains(w.as_str()) {
                    score += 5;
                }
            }
            (score, s)
        })
        .filter(|(sc, _)| *sc > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(4);
    let mut context = String::new();
    if let Some(ov) = record["overview"].as_str() {
        context.push_str(&format!("## Repository overview\n{}\n\n", ov.chars().take(3000).collect::<String>()));
    }
    let mut sources: Vec<Value> = Vec::new();
    for (_, s) in &scored {
        context.push_str(&format!(
            "## {}\n{}\n\n",
            s["title"].as_str().unwrap_or(""),
            s["md"].as_str().unwrap_or("").chars().take(2500).collect::<String>()
        ));
        if let Some(fl) = s["files"].as_array() {
            for f in fl.iter().take(2) {
                sources.push(json!({ "path": f }));
            }
        }
    }
    if scored.is_empty() {
        let files: Vec<&str> = record["files"].as_array().map(|a| a.iter().filter_map(|f| f.as_str()).collect()).unwrap_or_default();
        context.push_str(&format!("## Files in repository\n{}\n", files.join("\n")));
    }
    sources.truncate(6);
    (context.chars().take(16000).collect(), sources)
}

fn tree_text(record: &Value) -> String {
    record["files"]
        .as_array()
        .map(|a| a.iter().filter_map(|f| f.as_str()).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

fn page_of(doc: &str, page: usize, p: &str) -> String {
    let per_page = PAGE_TOKENS * 7 / 2;
    let chars: Vec<char> = doc.chars().collect();
    let pages = chars.len().div_ceil(per_page).max(1);
    let page = page.clamp(1, pages);
    let start = (page - 1) * per_page;
    let end = (start + per_page).min(chars.len());
    format!(
        "[{} — страница {}/{}]\n{}",
        p,
        page,
        pages,
        chars[start..end].iter().collect::<String>()
    )
}

// Инструменты агента читают ИСХОДНЫЙ КОД из SLED (src:)
fn exec_tool(db: &sled::Db, record: &Value, repo: &str, action: &str, raw_path: &str, page: usize) -> String {
    let p = raw_path.trim_start_matches('/');
    let all: Vec<&str> = record["files"].as_array().map(|a| a.iter().filter_map(|f| f.as_str()).collect()).unwrap_or_default();
    if action == "READ_FILE" || action == "READ_FILE_CHUNK" {
        match get_str(db, &format!("src:{}:{}", repo, p)) {
            Some(doc) => {
                if action == "READ_FILE_CHUNK" {
                    page_of(&doc, page.max(1), p)
                } else if doc.chars().count() > PAGE_TOKENS * 7 / 2 {
                    page_of(&doc, 1, p)
                } else {
                    doc
                }
            }
            None => format!("Файл не найден: {}. Доступные файлы:\n{}", p, all.iter().take(40).cloned().collect::<Vec<_>>().join("\n")),
        }
    } else {
        let members: Vec<&&str> = all.iter().filter(|f| **f == p || f.starts_with(&format!("{}/", p))).collect();
        if members.is_empty() {
            return format!("Папка не найдена: {}", p);
        }
        let mut lines = Vec::new();
        for f in members.iter().take(60) {
            let head = get_str(db, &format!("src:{}:{}", repo, f))
                .map(|s| crate::pipeline::first_line(&s))
                .unwrap_or_default();
            lines.push(format!("- {}: {}", f, head));
        }
        slice_by_tokens(&lines.join("\n"), 2500)
    }
}

fn find_tool_call(text: &str) -> Option<(String, String, usize)> {
    for action in ["READ_FILE_CHUNK", "READ_DIR", "READ_FILE"] {
        if let Some(idx) = text.rfind(action) {
            let rest = &text[idx + action.len()..];
            let mut it = rest.split_whitespace();
            if let Some(path) = it.next() {
                let path = path.trim_matches(|c| c == '`' || c == '"' || c == '\'' || c == ',' || c == ')');
                if path.is_empty() {
                    continue;
                }
                let page = it.next().and_then(|x| x.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok()).unwrap_or(1);
                return Some((action.to_string(), path.to_string(), page));
            }
        }
    }
    None
}

pub async fn answer(app: tauri::AppHandle, db: sled::Db, payload: ChatPayload) -> Result<String, String> {
    let record = crate::state::get_json(&db, &format!("repo:{}", payload.repo)).ok_or("Репозиторий не найден")?;
    let repo = payload.repo.clone();
    let model = payload.model_id.clone().or_else(|| record["modelId"].as_str().map(String::from)).ok_or("modelId обязателен")?;
    let lm_url = payload.lm_url.as_deref();
    let language = record["language"].as_str().unwrap_or("russian").to_string();
    let arch = payload.mode.as_deref() == Some("arch");

    // Контекст: arch → обзор+структура; иначе векторный RAG, fallback keyword
    let (context, sources, mode) = if arch {
        let mut ctx = String::new();
        if let Some(ov) = record["overview"].as_str() {
            ctx.push_str(&format!("## Repository overview\n{}\n\n", ov.chars().take(5000).collect::<String>()));
        }
        if let Some(ds) = record["dirSummaries"].as_object() {
            ctx.push_str("## Directory summaries\n");
            for (d, v) in ds.iter().take(30) {
                ctx.push_str(&format!("### {}\n{}\n", d, v.as_str().unwrap_or("")));
            }
        }
        ctx.push_str(&format!("\n## File tree\n{}\n", tree_text(&record)));
        (ctx.chars().take(16000).collect::<String>(), Vec::new(), "arch")
    } else {
        let hits = match payload.embedding_model.as_deref().filter(|m| !m.is_empty()) {
            Some(em) => vector_search(&db, &repo, &payload.question, lm_url, em).await,
            None => Vec::new(),
        };
        if !hits.is_empty() {
            let mut ctx = String::new();
            if let Some(ov) = record["overview"].as_str() {
                ctx.push_str(&format!("## Repository overview\n{}\n\n", ov.chars().take(2000).collect::<String>()));
            }
            let mut sources: Vec<Value> = Vec::new();
            for h in &hits {
                ctx.push_str(&format!("## `{}` (строки {}-{})\n```\n{}\n```\n\n", h.path, h.start, h.end, h.text));
                if !sources.iter().any(|s| s["path"] == json!(h.path)) {
                    sources.push(json!({ "path": h.path, "lines": [h.start, h.end] }));
                }
            }
            sources.truncate(6);
            (ctx.chars().take(16000).collect::<String>(), sources, "vector")
        } else {
            let (ctx, sources) = keyword_context(&record, &payload.question);
            (ctx, sources, "basic")
        }
    };

    let _ = app.emit("chat-sources", json!({ "sources": sources, "mode": mode, "truncated": false }));

    let system = format!(
        "You are SmartWiki assistant answering questions about the \"{}\" repository. Use the provided context. If the context is not enough, you may call tools by replying with EXACTLY one line: READ_DIR <path> — list files of a directory; READ_FILE <path> — read a source file; READ_FILE_CHUNK <path> <page> — read a page of a big file. After a tool response continue answering. Reference file paths in backticks. Answer strictly in {}, in Markdown.\n\nRepository file tree:\n{}\n\nContext:\n{}",
        repo,
        language,
        slice_by_tokens(&tree_text(&record), 1500),
        context
    );

    let mut messages = vec![json!({ "role": "system", "content": system })];
    for h in payload.history.unwrap_or_default().into_iter().rev().take(6).rev() {
        messages.push(h);
    }
    messages.push(json!({ "role": "user", "content": payload.question }));

    let mut full_answer = String::new();
    for step in 0..4 {
        let app2 = app.clone();
        let text = lm::chat_stream(lm_url, &model, &Value::Array(messages.clone()), |d| {
            let _ = app2.emit("chat-delta", d.to_string());
        })
        .await?;
        full_answer = text.clone();
        match find_tool_call(&text) {
            Some((action, path, page)) if step < 3 => {
                let result = exec_tool(&db, &record, &repo, &action, &path, page);
                messages.push(json!({ "role": "assistant", "content": text }));
                messages.push(json!({ "role": "user", "content": format!("Tool response ({} {}):\n{}\n\nNow answer the original question.", action, path, result) }));
                let _ = app.emit("chat-delta", "\n\n".to_string());
            }
            _ => break,
        }
    }
    Ok(full_answer)
}
