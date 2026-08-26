// Tauri-команды: полный аналог REST API web-версии
use crate::pipeline::{GenParams, MERMAID_RULES};
use crate::state::{del_prefix, get_json, get_str, repo_docs_dir, set_json, set_str, AppState};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub async fn fetch_models(lm_url: Option<String>) -> Result<Value, String> {
    // Тот же формат ответа, что у web-эндпоинта /api/models: { models: [{id}, ...] }
    let raw = crate::lm::fetch_models(lm_url.as_deref()).await?;
    let models = raw["data"].as_array().cloned().unwrap_or_default();
    Ok(json!({ "models": models }))
}

#[tauri::command]
pub fn list_repos(state: State<'_, AppState>) -> Value {
    let mut repos: Vec<Value> = crate::state::scan(&state.db, "repo:")
        .into_iter()
        .filter_map(|(k, v)| {
            let rec: Value = serde_json::from_slice(&v).ok()?;
            Some(json!({
                "repoKey": k.strip_prefix("repo:").unwrap_or(&k),
                "language": rec["language"],
                "modelId": rec["modelId"],
                "generatedAt": rec["generatedAt"],
                "fileCount": rec["files"].as_array().map(|a| a.len()).unwrap_or(0),
                "sectionCount": rec["sections"].as_array().map(|a| a.len()).unwrap_or(0),
            }))
        })
        .collect();
    repos.sort_by(|a, b| b["generatedAt"].as_str().unwrap_or("").cmp(a["generatedAt"].as_str().unwrap_or("")));
    json!({ "repos": repos })
}

#[tauri::command]
pub fn get_docs(state: State<'_, AppState>, repo: String) -> Result<Value, String> {
    get_json(&state.db, &format!("repo:{}", repo)).ok_or_else(|| "Документация не найдена".into())
}

#[tauri::command]
pub fn delete_repo(state: State<'_, AppState>, repo: String) -> Value {
    let _ = state.db.remove(format!("repo:{}", repo));
    del_prefix(&state.db, &format!("src:{}:", repo));
    del_prefix(&state.db, &format!("emb:{}:", repo));
    let _ = std::fs::remove_dir_all(repo_docs_dir(&state.data_dir, &repo));
    json!({ "ok": true })
}

#[tauri::command]
pub fn start_generate(app: tauri::AppHandle, state: State<'_, AppState>, payload: GenParams) -> Value {
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    let cancel = state.cancel.clone();
    tauri::async_runtime::spawn(async move {
        crate::pipeline::run_generate(app, db, data_dir, cancel, payload).await;
    });
    json!({ "jobId": "desktop" })
}

#[tauri::command]
pub fn start_generate_local(app: tauri::AppHandle, state: State<'_, AppState>, payload: GenParams) -> Value {
    start_generate(app, state, payload)
}

#[tauri::command]
pub fn cancel_generate(state: State<'_, AppState>) -> Value {
    state.cancel.store(true, Ordering::SeqCst);
    json!({ "ok": true })
}

#[tauri::command]
pub async fn chat(app: tauri::AppHandle, payload: crate::chat::ChatPayload) -> Result<String, String> {
    let state = app.state::<AppState>();
    let db = state.db.clone();
    crate::chat::answer(app.clone(), db, payload).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionPayload {
    pub repo: String,
    pub section_id: String,
    pub model_id: Option<String>,
    pub lm_url: Option<String>,
    pub max_tokens: Option<usize>,
}

#[tauri::command]
pub async fn generate_section(app: tauri::AppHandle, payload: SectionPayload) -> Result<String, String> {
    let state = app.state::<AppState>();
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    let mut record = get_json(&db, &format!("repo:{}", payload.repo)).ok_or("Репозиторий не найден")?;
    let language = record["language"].as_str().unwrap_or("russian").to_string();
    let model = payload.model_id.clone().or_else(|| record["modelId"].as_str().map(String::from)).ok_or("modelId обязателен")?;
    let (title, files): (String, Vec<String>) = {
        let sec = record["sections"]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["id"] == json!(payload.section_id)))
            .ok_or("Раздел не найден — перегенерируйте репозиторий")?;
        (
            sec["title"].as_str().unwrap_or("").to_string(),
            sec["files"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).collect()).unwrap_or_default(),
        )
    };
    let max_tokens = payload.max_tokens.unwrap_or(8000);
    let per_file = (max_tokens.saturating_sub(2500) / files.len().max(1)).max(400);
    let mut code_ctx = String::new();
    for f in &files {
        if let Some(src) = get_str(&db, &format!("src:{}:{}", payload.repo, f)) {
            code_ctx.push_str(&format!("### {}\n```\n{}\n```\n\n", f, crate::pipeline::slice_by_tokens(&src, per_file)));
        }
    }
    if code_ctx.is_empty() {
        return Err("Исходный код раздела не найден — перегенерируйте репозиторий целиком".into());
    }
    let messages = json!([
        { "role": "system", "content": format!("You are an expert software architect writing one section of a documentation wiki for the \"{repo}\" repository. Write a comprehensive, well-structured documentation page for the section \"{title}\" based on the provided source code. Structure: 1) start with a heading \"# {title}\"; 2) several paragraphs explaining the purpose and role of this part of the system; 3) EXACTLY ONE ```mermaid code block (graph TD syntax) showing relationships between the components of this section. {rules} 4) a Markdown table of key modules with columns Module | Purpose | Key file; 5) detailed subsections (## headings). Reference every file path in backticks. Output ONLY valid Markdown, strictly in {language}.", repo = payload.repo, title = title, rules = MERMAID_RULES, language = language) },
        { "role": "user", "content": format!("Section: {}\n\nSource code of the section files:\n\n{}", title, code_ctx) }
    ]);
    let app2 = app.clone();
    let markdown = crate::lm::chat_stream(payload.lm_url.as_deref(), &model, &messages, |d| {
        let _ = app2.emit("section-delta", d.to_string());
    })
    .await?;
    if let Some(arr) = record["sections"].as_array_mut() {
        for s in arr.iter_mut() {
            if s["id"] == json!(payload.section_id) {
                s["md"] = json!(markdown);
            }
        }
    }
    set_json(&db, &format!("repo:{}", payload.repo), &record);
    let out_dir = repo_docs_dir(&data_dir, &payload.repo);
    let _ = std::fs::create_dir_all(&out_dir);
    let _ = std::fs::write(out_dir.join(format!("{}.md", payload.section_id)), &markdown);
    Ok(markdown)
}

#[tauri::command]
pub async fn export_xdp(app: tauri::AppHandle, repo: String) -> Result<Option<String>, String> {
    let state = app.state::<AppState>();
    let db = state.db.clone();
    let repo2 = repo.clone();
    let app2 = app.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        crate::xdp::build_xdp(&db, &repo2, |line| {
            let _ = app2.emit("xdp-progress", line);
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let default_name = format!("{}.xdp", repo.replace('/', "__"));
    let path = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("XDP container", &["xdp"])
            .save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    match path {
        Some(p) => {
            std::fs::write(&p, &bytes).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn export_zip(app: tauri::AppHandle, repo: String) -> Result<Option<String>, String> {
    let state = app.state::<AppState>();
    let record = get_json(&state.db, &format!("repo:{}", repo)).ok_or("Репозиторий не найден")?;
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        use std::io::Write;
        if let Some(ov) = record["overview"].as_str() {
            zip.start_file("_OVERVIEW.md", opts).map_err(|e| e.to_string())?;
            zip.write_all(ov.as_bytes()).map_err(|e| e.to_string())?;
        }
        if let Some(sections) = record["sections"].as_array() {
            for s in sections {
                zip.start_file(format!("{}.md", s["id"].as_str().unwrap_or("section")), opts).map_err(|e| e.to_string())?;
                zip.write_all(s["md"].as_str().unwrap_or("").as_bytes()).map_err(|e| e.to_string())?;
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    let default_name = format!("{}-docs.zip", repo.replace('/', "__"));
    let path = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new().set_file_name(&default_name).add_filter("ZIP", &["zip"]).save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    match path {
        Some(p) => {
            std::fs::write(&p, &buf).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn import_xdp(app: tauri::AppHandle, data_b64: String) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|e| format!("base64: {}", e))?;
    let repo_key = crate::xdp::import_xdp(&state.db, &state.data_dir, &bytes)?;
    Ok(json!({ "ok": true, "repoKey": repo_key }))
}

#[tauri::command]
pub async fn pick_local_folder() -> Result<Option<Value>, String> {
    let picked = tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    match picked {
        Some(dir) => {
            let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "folder".into());
            let files = tauri::async_runtime::spawn_blocking(move || crate::repo::collect_files(&dir, 400))
                .await
                .map_err(|e| e.to_string())?;
            let arr: Vec<Value> = files.into_iter().map(|(p, c)| json!({ "path": p, "content": c })).collect();
            Ok(Some(json!({ "name": name, "files": arr })))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn send_feedback(state: State<'_, AppState>, repo: String, vote: String) -> Value {
    let key = format!("feedback:{}", repo);
    let mut fb = get_json(&state.db, &key).unwrap_or_else(|| json!({ "up": 0, "down": 0 }));
    let field = if vote == "up" { "up" } else { "down" };
    fb[field] = json!(fb[field].as_i64().unwrap_or(0) + 1);
    set_json(&state.db, &key, &fb);
    let _ = set_str(&state.db, "noop", "");
    json!({ "ok": true })
}
