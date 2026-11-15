// Перевод готовой языковой версии wiki: структура глав, mermaid, код и пути сохраняются
use crate::pipeline::{emit, now_iso, slugify, GenParams, Progress};
use crate::quality::{est_tokens, Llm};
use crate::state::{get_json, set_json};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn lang_name(language: &str) -> String {
    let mut c = language.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn system_prompt(language: &str) -> String {
    format!(
        "You are a technical translator and localization expert. Translate the following software documentation into {lang}. \
Strictly preserve all Markdown formatting, heading levels, tables, code blocks, file paths, variable and function names, CLI commands and Mermaid.js diagrams \
(inside mermaid blocks translate ONLY the quoted labels — never node IDs, arrows or syntax). \
Translate only the explanatory text, headings and descriptions using professional software-engineering terminology in {lang}. \
Do not summarize, shorten or add anything. Output ONLY the translated Markdown — no notes, no preamble.",
        lang = lang_name(language)
    )
}

// Части по заголовкам «## » (вне блоков кода), склеенные жадно в пределах limit токенов
fn split_md(md: &str, limit: usize) -> Vec<String> {
    let mut blocks: Vec<String> = vec![String::new()];
    let mut in_code = false;
    for line in md.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
        }
        if !in_code && t.starts_with("## ") && !blocks.last().unwrap().trim().is_empty() {
            blocks.push(String::new());
        }
        blocks.last_mut().unwrap().push_str(line);
    }
    let mut parts: Vec<String> = Vec::new();
    for b in blocks.into_iter().filter(|b| !b.trim().is_empty()) {
        match parts.last_mut() {
            Some(last) if est_tokens(last) + est_tokens(&b) <= limit => last.push_str(&b),
            _ => parts.push(b),
        }
    }
    parts
}

async fn translate_md<F: FnMut(usize, usize)>(llm: &Llm<'_>, md: &str, language: &str, limit: usize, mut on_part: F) -> Result<String, String> {
    let parts = split_md(md, limit);
    let mut out = String::new();
    for (k, part) in parts.iter().enumerate() {
        on_part(k + 1, parts.len());
        let msgs = json!([
            { "role": "system", "content": system_prompt(language) },
            { "role": "user", "content": format!("Documentation to translate:\n\n{}", part) }
        ]);
        let text = llm.cached("translate-v1", msgs).await?;
        out.push_str(text.trim_end());
        out.push_str("\n\n");
    }
    Ok(out.trim_end().to_string() + "\n")
}

// Заголовки глав и разделов одним запросом; при сбое разбора — исходные строки
async fn translate_list(llm: &Llm<'_>, items: &[String], language: &str, cancel: &Arc<AtomicBool>) -> Result<Vec<String>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let msgs = json!([
        { "role": "system", "content": format!("Translate each string of the JSON array into {} — they are titles of software documentation sections. Keep identifiers, file names and product names unchanged. Respond with ONLY a JSON array of strings of the same length and order, no markdown fences, no comments.", lang_name(language)) },
        { "role": "user", "content": serde_json::to_string(items).unwrap_or_default() }
    ]);
    let raw = llm.cached("titles-v1", msgs).await.unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        return Err("Отменено пользователем".into());
    }
    let parsed: Option<Vec<String>> = raw
        .find('[')
        .and_then(|s| raw.rfind(']').map(|e| &raw[s..=e]))
        .and_then(|j| serde_json::from_str(j).ok())
        .filter(|v: &Vec<String>| v.len() == items.len() && v.iter().all(|t| !t.trim().is_empty()));
    Ok(parsed.unwrap_or_else(|| items.to_vec()))
}

pub async fn run(
    app: &tauri::AppHandle,
    db: &sled::Db,
    data_dir: &std::path::PathBuf,
    cancel: &Arc<AtomicBool>,
    params: &GenParams,
    source_key: &str,
    p: &mut Progress,
) -> Result<(), String> {
    let language = params.language.clone().unwrap_or_else(|| "english".into());
    let src = get_json(db, &format!("repo:{}", source_key)).ok_or("Исходная языковая версия не найдена")?;
    let sections: Vec<Value> = src["sections"].as_array().cloned().unwrap_or_default();
    if sections.is_empty() {
        return Err("В исходной версии нет разделов для перевода".into());
    }
    let repo_key = format!("{}@{}", crate::state::base_repo(source_key), language);
    p.repo_key = repo_key.clone();
    let llm = Llm {
        db,
        lm_url: params.lm_url.as_deref(),
        model: params.model_id.as_str(),
        sampling: params.sampling(),
        cancel,
        max_tokens: params.max_tokens.unwrap_or(8000),
    };
    // Перевод примерно равен оригиналу по длине: в контекст должны поместиться оба
    let limit = (llm.max_tokens.saturating_sub(1000) / 2).clamp(700, 4000);

    p.status = "sectioning".into();
    p.current_file = "Перевод заголовков".into();
    emit(app, p);
    let titles: Vec<String> = sections.iter().map(|s| s["title"].as_str().unwrap_or("").to_string()).collect();
    let mut group_names: Vec<String> = Vec::new();
    for s in &sections {
        if let Some(g) = s["group"].as_str() {
            if !group_names.iter().any(|x| x == g) {
                group_names.push(g.to_string());
            }
        }
    }
    let mut all = titles.clone();
    all.extend(group_names.iter().cloned());
    let translated = translate_list(&llm, &all, &language, cancel).await?;
    let new_titles = &translated[..titles.len()];
    let groups: HashMap<&str, &str> = group_names.iter().map(String::as_str).zip(translated[titles.len()..].iter().map(String::as_str)).collect();

    let out_dir = crate::state::repo_docs_dir(data_dir, &repo_key);
    let _ = std::fs::remove_dir_all(&out_dir);
    let _ = std::fs::create_dir_all(&out_dir);
    let ids: Vec<String> = new_titles.iter().enumerate().map(|(i, t)| format!("{:02}-{}", i + 1, slugify(t))).collect();
    let new_group = |s: &Value| s["group"].as_str().map(|g| groups.get(g).copied().unwrap_or(g).to_string());
    p.status = "generating".into();
    p.total = sections.len();
    p.processed = 0;
    p.sections = sections
        .iter()
        .enumerate()
        .map(|(i, s)| json!({ "id": ids[i], "title": new_titles[i], "group": new_group(s), "done": false }))
        .collect();
    emit(app, p);

    let mut out_sections: Vec<Value> = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("Отменено пользователем".into());
        }
        let title = &new_titles[i];
        p.current_file = title.clone();
        emit(app, p);
        let md = translate_md(&llm, s["md"].as_str().unwrap_or(""), &language, limit, |k, n| {
            if n > 1 {
                p.current_file = format!("{} ({}/{})", title, k, n);
                emit(app, p);
            }
        })
        .await?;
        let _ = std::fs::write(out_dir.join(format!("{}.md", ids[i])), &md);
        let mut sec = s.clone();
        sec["id"] = json!(ids[i]);
        sec["title"] = json!(title);
        sec["group"] = json!(new_group(s));
        sec["md"] = json!(md);
        out_sections.push(sec);
        p.processed += 1;
        p.sections[i]["done"] = json!(true);
        emit(app, p);
    }

    p.status = "overview".into();
    p.current_file = String::new();
    emit(app, p);
    let overview = match src["overview"].as_str().filter(|o| !o.trim().is_empty()) {
        Some(ov) => translate_md(&llm, ov, &language, limit, |_, _| {}).await?,
        None => String::new(),
    };

    let mut record = src.clone();
    record["repoKey"] = json!(repo_key);
    record["generatedAt"] = json!(now_iso());
    record["language"] = json!(language);
    record["modelId"] = json!(params.model_id);
    record["overview"] = json!(overview);
    record["sections"] = Value::Array(out_sections);
    record["translatedFrom"] = json!(source_key);
    record["changes"] = Value::Null;
    set_json(db, &format!("repo:{}", repo_key), &record);
    if !overview.is_empty() {
        let _ = std::fs::write(out_dir.join("_OVERVIEW.md"), &overview);
    }
    p.status = "done".into();
    p.current_file = String::new();
    emit(app, p);
    Ok(())
}
