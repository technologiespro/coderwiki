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
    pub repo_key: String,
    pub sections: Vec<Value>,
}

// Локализованный заголовок обязательного раздела «Установка и использование»
fn install_title_for(language: &str) -> &'static str {
    match language {
        "russian" => "Установка и использование",
        "chinese" => "安装与使用",
        "japanese" => "インストールと使い方",
        "spanish" => "Instalación y uso",
        "german" => "Installation und Verwendung",
        "portuguese" => "Instalação e uso",
        "french" => "Installation et utilisation",
        "arabic" => "التثبيت والاستخدام",
        "hindi" => "इंस्टॉलेशन और उपयोग",
        "indonesian" => "Instalasi dan Penggunaan",
        "vietnamese" => "Cài đặt và sử dụng",
        "kazakh" => "Орнату және пайдалану",
        "swahili" => "Usakinishaji na matumizi",
        _ => "Installation & Usage",
    }
}

const INSTALL_KEYWORDS: &[&str] = &[
    "install", "установк", "getting started", "начало работы", "setup", "quick start",
    "быстрый старт", "использовани", "usage", "安装", "インストール", "instalac",
];

fn is_install_title(title: &str) -> bool {
    let l = title.to_lowercase();
    INSTALL_KEYWORDS.iter().any(|k| l.contains(k))
}

const INSTALL_MANIFESTS: &[&str] = &[
    "package.json", "requirements.txt", "pyproject.toml", "setup.py", "cargo.toml", "go.mod",
    "composer.json", "gemfile", "makefile", "docker-compose.yml", "dockerfile", ".env.example",
    "install.md", "usage.md", "bower.json",
];

// Гарантирует раздел «Установка и использование» вторым после обзора (как на deepwiki.com)
fn ensure_install_section(plan: &mut Vec<(String, Vec<String>)>, files: &[String], language: &str, groups: &mut HashMap<String, String>) {
    if plan.iter().any(|(t, _)| is_install_title(t)) {
        return;
    }
    let picks: Vec<String> = files
        .iter()
        .filter(|f| {
            if f.contains('/') {
                return false;
            }
            let l = f.to_lowercase();
            l.starts_with("readme") || INSTALL_MANIFESTS.contains(&l.as_str())
        })
        .take(8)
        .cloned()
        .collect();
    if picks.is_empty() {
        return;
    }
    let title = install_title_for(language).to_string();
    // Наследуем главу обзорного раздела, чтобы страница была в первой главе
    if let Some(g) = plan.first().and_then(|(t, _)| groups.get(t)).cloned() {
        groups.insert(title.clone(), g);
    }
    let idx = 1.min(plan.len());
    plan.insert(idx, (title, picks));
}

// Локализованный заголовок страницы «Идеи развития»
fn ideas_title_for(language: &str) -> &'static str {
    match language {
        "russian" => "Идеи развития",
        "chinese" => "改进建议",
        "japanese" => "改善アイデア",
        "spanish" => "Ideas de mejora",
        "german" => "Verbesserungsideen",
        "portuguese" => "Ideias de melhoria",
        "french" => "Idées d'amélioration",
        "arabic" => "أفكار للتحسين",
        "hindi" => "सुधार के विचार",
        "indonesian" => "Ide Pengembangan",
        "vietnamese" => "Ý tưởng cải tiến",
        "kazakh" => "Даму идеялары",
        "swahili" => "Mawazo ya kuboresha",
        _ => "Improvement Ideas",
    }
}

// Эвристика для новых файлов: индекс раздела с самым длинным общим префиксом пути
fn assign_heuristic(plan: &[(String, Vec<String>)], f: &str) -> usize {
    let fb: Vec<&str> = f.split('/').collect();
    let mut best = plan.len().saturating_sub(1);
    let mut best_score = -1i32;
    for (idx, (_, fl)) in plan.iter().enumerate() {
        let mut score = 0i32;
        for sf in fl {
            let a: Vec<&str> = sf.split('/').collect();
            let mut i = 0;
            while i + 1 < a.len() && i + 1 < fb.len() && a[i] == fb[i] {
                i += 1;
            }
            score = score.max(i as i32);
        }
        if score > best_score {
            best_score = score;
            best = idx;
        }
    }
    best
}

pub fn emit(app: &tauri::AppHandle, p: &Progress) {
    let _ = app.emit("gen-progress", p);
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenParams {
    pub repo_url: Option<String>,
    pub name: Option<String>,
    // База репозитория (owner/name) для догенерации языковой версии из кэша исходников
    pub base_key: Option<String>,
    pub files: Option<Vec<LocalFile>>,
    // Доп. страница «Идеи развития» (включается в настройках, по умолчанию выключена)
    pub improvement_ideas: Option<bool>,
    // GitHub PAT для клонирования приватных репозиториев (хранится только на клиенте)
    pub github_token: Option<String>,
    pub language: Option<String>,
    pub model_id: String,
    pub lm_url: Option<String>,
    pub embedding_model: Option<String>,
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub max_files: Option<usize>,
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub max_tokens: Option<usize>,
    // Сэмплирование из настроек (по умолчанию — рекомендованные для документации)
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub temperature: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub top_p: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub top_k: Option<usize>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub frequency_penalty: Option<f64>,
    // Ключ готовой языковой версии: новая версия переводится из неё, а не генерируется заново
    #[serde(default)]
    pub translate_from: Option<String>,
}

impl GenParams {
    pub fn sampling(&self) -> lm::Sampling {
        let d = lm::Sampling::default();
        lm::Sampling {
            temperature: self.temperature.map_or(d.temperature, |v| v.clamp(0.0, 2.0)),
            top_p: self.top_p.map_or(d.top_p, |v| v.clamp(0.05, 1.0)),
            top_k: self.top_k.map_or(d.top_k, |v| v.clamp(1, 500) as u32),
            frequency_penalty: self.frequency_penalty.map_or(d.frequency_penalty, |v| v.clamp(0.0, 2.0)),
        }
    }
}

pub fn de_opt_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<serde_json::Value> = serde::Deserialize::deserialize(d)?;
    Ok(v.and_then(|x| match x {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }))
}

// Настройки из UI приходят строками ("100") или числами — принимаем оба варианта
pub fn de_opt_usize<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<serde_json::Value> = serde::Deserialize::deserialize(d)?;
    Ok(v.and_then(|x| match x {
        Value::Number(n) => n.as_u64().map(|n| n as usize),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }))
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

pub(crate) fn slugify(t: &str) -> String {
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
// Ожидание LLM-запроса с быстрой отменой по флагу cancel (проверка каждые 200 мс)
pub(crate) async fn with_cancel<T>(cancel: &Arc<AtomicBool>, fut: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::pin!(fut);
    loop {
        tokio::select! {
            res = &mut fut => return res,
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if cancel.load(Ordering::SeqCst) {
                    return Err("Отменено пользователем".into());
                }
            }
        }
    }
}

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
    let res = match params.translate_from.as_deref().filter(|k| !k.is_empty()) {
        Some(src) => crate::translate::run(&app, &db, &data_dir, &cancel, &params, src, &mut p).await,
        None => run_inner(&app, &db, &data_dir, &cancel, &params, &mut p).await,
    };
    match res {
        Ok(()) => {}
        Err(e) => {
            if e == "Отменено пользователем" {
                p.status = "cancelled".into();
            } else {
                p.status = "error".into();
                p.error = e;
            }
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
    let sampling = params.sampling();
    let rules = crate::quality::lang_rules(&language);

    // Клонирование или локальные файлы
    let (repo_key, files_with_content): (String, Vec<(String, String)>) = if let Some(files) = &params.files {
        // base_key задан — догенерация другой языковой версии существующего репозитория
        let key = if let Some(base) = &params.base_key {
            format!("{}@{}", base, language)
        } else {
            format!("local/{}@{}", params.name.clone().unwrap_or_else(|| "folder".into()), language)
        };
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
        let gh_token = params.github_token.clone();
        let (key, dir) = tauri::async_runtime::spawn_blocking(move || repo::clone_repo(&url, &repos_dir, gh_token.as_deref()))
            .await
            .map_err(|e| e.to_string())??;
        let key = format!("{}@{}", key, language);
        p.status = "parsing".into();
        emit(app, p);
        let list = repo::collect_files(&dir, max_files);
        (key, list)
    };
    if files_with_content.is_empty() {
        return Err("Не найдено подходящих текстовых файлов".into());
    }

    let files: Vec<String> = files_with_content.iter().map(|(f, _)| f.clone()).collect();
    p.repo_key = repo_key.clone();
    let mut contents: HashMap<String, String> = HashMap::new();
    let mut changed: std::collections::HashSet<String> = Default::default();
    for (f, c) in &files_with_content {
        let key = format!("src:{}:{}", crate::state::base_repo(&repo_key), f);
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
    // Диф-обновление: если wiki уже существует, переиспользуем прежний план разделов
    // и перегенерируем только разделы, затронутые изменёнными/новыми/удалёнными файлами
    let prev_record0 = get_json(db, &format!("repo:{}", repo_key));
    let mut prev_md_by_title: HashMap<String, String> = HashMap::new();
    let mut prev_files_by_title: HashMap<String, Vec<String>> = HashMap::new();
    // Главы разделов (title -> chapter) для древовидного сайдбара
    let mut groups: HashMap<String, String> = HashMap::new();
    let mut plan_diff: Option<Vec<(String, Vec<String>)>> = None;
    if let Some(prev) = &prev_record0 {
        if let Some(secs) = prev["sections"].as_array() {
            let prev_secs: Vec<&Value> = secs
                .iter()
                .filter(|s| !s["ideas"].as_bool().unwrap_or(false) && s["title"].is_string())
                .collect();
            if !prev_secs.is_empty() {
                for s in &prev_secs {
                    let t = s["title"].as_str().unwrap_or("").to_string();
                    let fl: Vec<String> = s["files"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    if let Some(g) = s["group"].as_str() {
                        groups.insert(t.clone(), g.to_string());
                    }
                    prev_md_by_title.insert(t.clone(), s["md"].as_str().unwrap_or("").to_string());
                    prev_files_by_title.insert(t, fl);
                }
                let file_set: std::collections::HashSet<&String> = files.iter().collect();
                let known: std::collections::HashSet<String> =
                    prev_files_by_title.values().flatten().cloned().collect();
                let mut plan_new: Vec<(String, Vec<String>)> = prev_secs
                    .iter()
                    .filter_map(|s| {
                        let t = s["title"].as_str()?.to_string();
                        let fl: Vec<String> = prev_files_by_title.get(&t)?.iter().filter(|f| file_set.contains(f)).cloned().collect();
                        Some((t, fl))
                    })
                    .collect();
                let added: Vec<String> = files.iter().filter(|f| !known.contains(*f)).cloned().collect();
                if !added.is_empty() && plan_new.iter().any(|(_, fl)| !fl.is_empty()) {
                    // LLM распределяет новые файлы по разделам; при ошибке — эвристика по пути
                    p.current_file = "Распределение новых файлов".into();
                    emit(app, p);
                    let list: String = plan_new
                        .iter()
                        .map(|(t, fl)| format!("- {}: {}", t, fl.iter().take(8).cloned().collect::<Vec<_>>().join(", ")))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let msgs = json!([
                        { "role": "system", "content": "You are organizing a documentation wiki. Assign each NEW file to exactly one of the EXISTING sections. Respond with ONLY a valid JSON object mapping file path to section title, no markdown fences, no comments: {\"path/one\":\"Section Title\"}" },
                        { "role": "user", "content": format!("Existing sections:\n{}\n\nNew files:\n{}", list, added.join("\n")) }
                    ]);
                    let raw = with_cancel(cancel, lm::chat_once(lm_url, model, &msgs, &sampling)).await.unwrap_or_default();
                    if cancel.load(Ordering::SeqCst) {
                        return Err("Отменено пользователем".into());
                    }
                    let assigned: Option<Value> = raw
                        .find('{')
                        .and_then(|st| raw.rfind('}').map(|en| raw[st..=en].to_string()))
                        .and_then(|s| serde_json::from_str(&s).ok());
                    for f in &added {
                        let idx = assigned
                            .as_ref()
                            .and_then(|a| a[f.as_str()].as_str())
                            .and_then(|t| plan_new.iter().position(|(pt, _)| pt == t))
                            .unwrap_or_else(|| assign_heuristic(&plan_new, f));
                        plan_new[idx].1.push(f.clone());
                    }
                }
                plan_new.retain(|(_, fl)| !fl.is_empty());
                if !plan_new.is_empty() {
                    plan_diff = Some(plan_new);
                }
            }
        }
    }
    let plan_key = format!("cache:{}", sha(&format!("plan::{}::{}::{}::plan-v3", files.join(","), language, model)));
    let mut plan: Option<Vec<(String, Vec<String>)>> = plan_diff;
    if plan.is_none() {
        plan = get_json(db, &plan_key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    let t = s["title"].as_str()?.to_string();
                    if let Some(g) = s["group"].as_str() {
                        groups.insert(t.clone(), g.to_string());
                    }
                    Some((
                        t,
                        s["files"].as_array()?.iter().filter_map(|f| f.as_str().map(String::from)).collect(),
                    ))
                })
                .collect()
        })
        });
    }
    if plan.is_none() {
        // Оглавление как на DeepWiki: главы → подразделы; модель отдаёт только названия и пути-префиксы
        let hints: HashMap<String, String> = files.iter().map(|f| (f.clone(), first_line(contents.get(f).unwrap()))).collect();
        let readme = files
            .iter()
            .find(|f| !f.contains('/') && f.to_lowercase().starts_with("readme"))
            .and_then(|f| contents.get(f))
            .cloned()
            .unwrap_or_default();
        let llm = crate::quality::Llm { db, lm_url, model, sampling, cancel, max_tokens };
        let (parsed, ok) = crate::planner::plan_wiki(&llm, &repo_key, &files, &hints, &readme, &language, &mut groups).await?;
        // Запасной план не кэшируем — при следующем запуске модель попробует снова
        if ok {
            let as_json: Value = Value::Array(
                parsed.iter().map(|(t, fl)| json!({ "title": t, "files": fl, "group": groups.get(t) })).collect(),
            );
            set_json(db, &plan_key, &as_json);
        }
        plan = Some(parsed);
    }
    let mut plan = plan.unwrap();
    ensure_install_section(&mut plan, &files, &language, &mut groups);
    let plan = plan;

    // Этап 2а: эмбеддинги ДО генерации — RAG-контекст связанных файлов для разделов и чат
    if let Some(em) = params.embedding_model.as_deref().filter(|m| !m.is_empty()) {
        p.status = "embedding".into();
        emit(app, p);
        for f in &files {
            if cancel.load(Ordering::SeqCst) {
                return Err("Отменено пользователем".into());
            }
            let has_emb = db.scan_prefix(format!("emb:{}:{}:", crate::state::base_repo(&repo_key), f)).next().is_some();
            if has_emb && !changed.contains(f) {
                continue;
            }
            crate::state::del_prefix(db, &format!("emb:{}:{}:", crate::state::base_repo(&repo_key), f));
            p.current_file = f.clone();
            emit(app, p);
            let chunks = chunk_code(contents.get(f).unwrap());
            if chunks.is_empty() {
                continue;
            }
            let texts: Vec<String> = chunks.iter().map(|(t, _, _)| t.clone()).collect();
            match with_cancel(cancel, lm::embeddings(lm_url, em, &texts)).await {
                Ok(vectors) => {
                    for (i, (text, s, e)) in chunks.iter().enumerate() {
                        if let Some(v) = vectors.get(i).filter(|v| !v.is_empty()) {
                            set_json(
                                db,
                                &format!("emb:{}:{}:{}", crate::state::base_repo(&repo_key), f, i),
                                &json!({ "text": text, "startLine": s, "endLine": e, "vector": v }),
                            );
                        }
                    }
                }
                Err(_) => break, // эмбеддинги необязательны
            }
        }
    }

    let embs = crate::quality::load_embeddings(db, crate::state::base_repo(&repo_key));
    let llm = crate::quality::Llm { db, lm_url, model, sampling, cancel, max_tokens };

    // Этап 2: генерация страниц разделов (инкрементальное сохранение .md после каждого)
    let out_dir = crate::state::repo_docs_dir(data_dir, &repo_key);
    let _ = std::fs::remove_dir_all(&out_dir);
    let _ = std::fs::create_dir_all(&out_dir);
    let section_ids: Vec<String> = plan.iter().enumerate().map(|(i, (t, _))| format!("{:02}-{}", i + 1, slugify(t))).collect();
    p.status = "generating".into();
    p.total = plan.len();
    p.processed = 0;
    p.sections = plan.iter().enumerate().map(|(i, (t, _))| serde_json::json!({ "id": section_ids[i], "title": t, "group": groups.get(t), "done": false })).collect();
    emit(app, p);
    // README для первой страницы: помогает LLM определить назначение проекта
    let readme_hint: String = files
        .iter()
        .find(|f| {
            let l = f.to_lowercase();
            l == "readme.md" || l.ends_with("/readme.md") || l.starts_with("readme.")
        })
        .and_then(|f| contents.get(f))
        .map(|c| c.chars().take(2000).collect())
        .unwrap_or_default();
    let mut sections: Vec<Value> = Vec::new();
    let mut updated_ids: Vec<String> = Vec::new();
    for (i, (title, sec_files)) in plan.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("Отменено пользователем".into());
        }
        p.current_file = title.clone();
        p.partial = String::new();
        emit(app, p);
        // Диф-обновление: раздел без изменённых файлов берём из прежней записи как есть
        let sec_unchanged = prev_files_by_title
            .get(title)
            .map_or(false, |pf| pf == sec_files && !sec_files.iter().any(|f| changed.contains(f)))
            && prev_md_by_title.get(title).map_or(false, |m| !m.is_empty());
        // Бюджет контекста: ~85% — код раздела (map-reduce для больших файлов), ~15% — связанный код
        let budget = max_tokens.saturating_sub(2500).max(1500);
        let related_budget = budget * 15 / 100;
        let code_ctx = if sec_unchanged {
            String::new()
        } else {
            let mut ctx = crate::quality::section_context(&llm, sec_files, &contents, budget - related_budget, |f, k, n| {
                p.current_file = format!("{} · {} ({}/{})", title, f, k, n);
                emit(app, p);
            })
            .await?;
            ctx.push_str(&crate::quality::related_context(sec_files, &files, &contents, &embs, related_budget));
            p.current_file = title.clone();
            emit(app, p);
            ctx
        };
        let cache_key = format!("cache:{}", sha(&format!("section::{}::{}::{}::{}{}{}::q2", title, code_ctx, language, model, if i == 0 { "::first-v2" } else { "" }, if is_install_title(title) { "::install-v1" } else { "" })));
        let md_text = if sec_unchanged {
            p.cached_hits += 1;
            prev_md_by_title[title.as_str()].clone()
        } else if let Some(cached) = get_str(db, &cache_key) {
            p.cached_hits += 1;
            cached
        } else {
            updated_ids.push(section_ids[i].clone());
            // Первая страница wiki должна начинаться с назначения проекта
            let first_rule = if i == 0 {
                " This is the FIRST page of the wiki: BEFORE any implementation or configuration details, open with 2-3 paragraphs that clearly identify WHAT this project is, what problem it solves, its purpose and intended users — infer this from the README, manifests and code."
            } else {
                ""
            };
            let hint_block = if i == 0 && !readme_hint.is_empty() {
                format!("\n\nProject README excerpt (use it to identify the project purpose):\n{}", readme_hint)
            } else {
                String::new()
            };
            let messages = if is_install_title(title) {
                // Раздел установки: практический гайд по README и манифестам, без mermaid
                json!([
                    { "role": "system", "content": format!("You are writing the getting-started page of a documentation wiki for the \"{repo_key}\" repository. Based PRIMARILY on the README and manifest/config files provided, write a practical installation and usage guide. Structure: 1) start with a heading \"# {title}\"; 2) \"Prerequisites\" — required runtimes, tools and versions; 3) \"Installation\" — exact step-by-step commands (cloning, dependency installation) in ```bash code blocks; 4) \"Configuration\" — environment variables, config files and build scripts if any; 5) \"Usage examples\" — reproduce the real usage and code examples from the README accurately in code blocks; when the README has none, derive minimal examples from the public API; 6) \"Running tests\" if test scripts exist. Use ONLY facts found in the provided files — never invent commands, package names or versions. Reference file paths in backticks. {rules}", repo_key = repo_key, title = title, rules = rules) },
                    { "role": "user", "content": format!("Section: {}\n\nSource files (README, manifests, configs):\n\n{}", title, code_ctx) }
                ])
            } else {
                json!([
                { "role": "system", "content": format!("You are an expert software architect writing one section of a documentation wiki for the \"{repo_key}\" repository. Write a comprehensive, well-structured documentation page for the section \"{title}\" based on the provided source code.{first_rule} Structure: 1) start with a heading \"# {title}\"; 2) several paragraphs explaining the purpose and role of this part of the system; 3) EXACTLY ONE ```mermaid code block (graph TD syntax) showing relationships between the components of this section. {rules} 4) a Markdown table of key modules with columns Module | Purpose | Key file; 5) detailed subsections (## headings) describing the important components, their APIs, data flow and interactions. {coverage} Reference every file path in backticks like `src/app.js`. {lang_rules}", repo_key = repo_key, title = title, first_rule = first_rule, rules = MERMAID_RULES, coverage = crate::quality::COVERAGE, lang_rules = rules) },
                { "role": "user", "content": format!("Section: {}{}\n\nSource code of the section files:\n\n{}", title, hint_block, code_ctx) }
                ])
            };
            let app2 = app.clone();
            let mut snapshot = p.clone();
            let mut last = std::time::Instant::now();
            let text = with_cancel(cancel, lm::chat_stream(lm_url, model, &messages, &sampling, |_d| {
                if last.elapsed().as_millis() > 400 {
                    last = std::time::Instant::now();
                    snapshot.partial.clear();
                    emit(&app2, &snapshot);
                }
            }))
            .await?;
            set_str(db, &cache_key, &text);
            text
        };
        let _ = std::fs::write(out_dir.join(format!("{}.md", section_ids[i])), &md_text);
        sections.push(json!({
            "id": section_ids[i],
            "title": title,
            "group": groups.get(title),
            "files": sec_files,
            "md": md_text,
        }));
        p.processed += 1;
        p.partial = String::new();
        p.sections[i]["done"] = json!(true);
        emit(app, p);
    }

    // Этап 4: обзор
    p.status = "overview".into();
    p.current_file = String::new();
    emit(app, p);
    // Выдержки разделов для обзора: бюджет контекста делится поровну (а не 500 символов на раздел)
    let per_section_chars = (max_tokens.saturating_sub(2500).max(1500) * 7 / 2 / sections.len().max(1)).clamp(500, 4000);
    let excerpts: String = sections
        .iter()
        .map(|s| format!("### {}\n{}", s["title"].as_str().unwrap_or(""), s["md"].as_str().unwrap_or("").chars().take(per_section_chars).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n\n");
    let ov_messages = json!([
        { "role": "system", "content": format!("You are an expert software architect. Based on the documentation section excerpts provided, write a comprehensive high-level overview of the repository. Structure it as follows: 1) A short opening paragraph explaining what the project is and its purpose; 2) \"Purpose and audience\" section describing target users and use cases; 3) \"Key features\" section as a Markdown table with columns Feature | Description | Key files; 4) \"Architecture\" section with EXACTLY ONE ```mermaid code block (graph TD syntax) showing the relationships between the main modules. {} 5) \"Key modules\" section briefly describing the most important parts. Reference file paths in backticks. {}", MERMAID_RULES, rules) },
        { "role": "user", "content": format!("Repository: {}\n\nSection excerpts:\n{}", repo_key, excerpts) }
    ]);
    let overview = with_cancel(cancel, lm::chat_once(lm_url, model, &ov_messages, &sampling)).await.unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        return Err("Отменено пользователем".into());
    }

    // Доп. страница «Идеи развития» (включается в настройках, по умолчанию выключена)
    if params.improvement_ideas == Some(true) {
        let ideas_title = ideas_title_for(&language);
        p.current_file = ideas_title.to_string();
        emit(app, p);
        let ideas_key = format!("cache:{}", sha(&format!("ideas::{}::{}::{}", excerpts, language, model)));
        let ideas_md = if let Some(cached) = get_str(db, &ideas_key) {
            cached
        } else {
            let ideas_messages = json!([
                { "role": "system", "content": format!("You are an experienced staff engineer and product strategist reviewing the \"{repo_key}\" repository. Based on the documentation section excerpts, write an \"improvement ideas\" page. Structure: 1) start with a heading \"# {title}\"; 2) \"Architecture and code\" — refactoring and tech-debt suggestions; 3) \"Performance and security\" — potential optimizations and hardening; 4) \"New features\" as a Markdown table with columns Idea | Value | Complexity (S/M/L); 5) \"Quick wins\" — small improvements achievable in under a day. Be specific to THIS project, reference file paths in backticks. {rules}", repo_key = crate::state::base_repo(&repo_key), title = ideas_title, rules = rules) },
                { "role": "user", "content": format!("Repository: {}\n\nDocumentation section excerpts:\n\n{}", crate::state::base_repo(&repo_key), excerpts) }
            ]);
            let text = with_cancel(cancel, lm::chat_once(lm_url, model, &ideas_messages, &sampling)).await.unwrap_or_default();
            if cancel.load(Ordering::SeqCst) {
                return Err("Отменено пользователем".into());
            }
            if !text.is_empty() {
                set_str(db, &ideas_key, &text);
            }
            text
        };
        if !ideas_md.is_empty() {
            let ideas_id = format!("{:02}-improvement-ideas", sections.len() + 1);
            let _ = std::fs::write(out_dir.join(format!("{}.md", ideas_id)), &ideas_md);
            sections.push(json!({ "id": ideas_id, "title": ideas_title, "files": [], "md": ideas_md, "ideas": true }));
        }
        p.current_file = String::new();
        emit(app, p);
    }

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
        Some(json!({ "added": added, "modified": modified, "removed": removed, "sectionsUpdated": updated_ids, "updatedAt": now_iso() }))
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
