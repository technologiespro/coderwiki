// Качество документации: строгие правила языка, map-reduce больших файлов, контекст связанных файлов
use crate::lm::{self, Sampling};
use crate::state::{get_str, scan, set_str};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

// ~3.5 символа на токен (та же оценка, что slice_by_tokens)
pub fn est_tokens(s: &str) -> usize {
    s.chars().count() * 2 / 7 + 1
}

fn sha(data: &str) -> String {
    format!("{:x}", Sha256::digest(data.as_bytes()))
}

fn lang_name(language: &str) -> String {
    let mut c = language.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

// Общие жёсткие правила для всех промптов генерации
pub fn lang_rules(language: &str) -> String {
    let lang = lang_name(language);
    format!(
        "CRITICAL RULES: \
1) Write STRICTLY in {lang}: every explanation, heading, table cell and diagram label must be in {lang} with professional software-engineering terminology — never switch to English just because the source code or comments are in English. \
2) Keep code snippets, file paths, identifiers (function, class, method, variable, type names), CLI commands and config keys exactly as in the source — never translate them. \
3) Be exhaustive, not superficial: document parameters with their types, return values, error handling, edge cases and the internal logic. \
4) Use ONLY facts present in the provided code and notes — never invent functions, arguments, options or behavior. \
5) Output STRICTLY valid Markdown without conversational filler, preamble or closing remarks.",
        lang = lang
    )
}

// Что обязательно раскрыть на странице раздела
pub const COVERAGE: &str = "For the section cover: purpose and core responsibility; public API / exports (functions, classes, interfaces with parameters and return types); architecture and internal logic (how it works under the hood); error handling and edge cases; dependencies and integration points with other parts of the repository.";

// Начала строк-объявлений: границы логических блоков и «сигнатуры» связанных файлов
const DECL: &[&str] = &[
    "fn ", "pub ", "impl", "struct ", "enum ", "trait ", "mod ", "class ", "def ", "async ", "function", "export ",
    "interface ", "type ", "func ", "public ", "private ", "protected ", "static ", "module.exports", "abstract ",
];
const LEAD: &[&str] = &["#[", "@", "///", "/**", "\"\"\"", "describe(", "it(", "test("];

fn is_decl(t: &str) -> bool {
    DECL.iter().any(|k| t.starts_with(k))
}

fn is_boundary(line: &str) -> bool {
    !line.starts_with(char::is_whitespace) && (is_decl(line) || LEAD.iter().any(|k| line.starts_with(k)))
}

// Разбиение файла на блоки по объявлениям верхнего уровня (функции/классы), ~target токенов в блоке
pub fn logical_chunks(content: &str, target_tokens: usize) -> Vec<(String, usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let target = target_tokens * 7 / 2;
    let hard = target * 3 / 2;
    let mut out = Vec::new();
    let (mut start, mut size) = (0usize, 0usize);
    for (i, l) in lines.iter().enumerate() {
        if i > start && ((size >= target && is_boundary(l)) || size >= hard) {
            out.push((lines[start..i].join("\n"), start + 1, i));
            start = i;
            size = 0;
        }
        size += l.len() + 1;
    }
    if start < lines.len() {
        out.push((lines[start..].join("\n"), start + 1, lines.len()));
    }
    out.into_iter().map(|(t, a, b)| (t.chars().take(hard).collect(), a, b)).collect()
}

pub struct Llm<'a> {
    pub db: &'a sled::Db,
    pub lm_url: Option<&'a str>,
    pub model: &'a str,
    pub sampling: Sampling,
    pub cancel: &'a Arc<AtomicBool>,
    pub max_tokens: usize,
}

impl Llm<'_> {
    // Запрос с кэшем в sled: ключ — содержимое запроса и модель
    pub async fn cached(&self, tag: &str, messages: Value) -> Result<String, String> {
        let key = format!("cache:{}", sha(&format!("{}::{}::{}", tag, self.model, messages)));
        if let Some(c) = get_str(self.db, &key) {
            return Ok(c);
        }
        let text = crate::pipeline::with_cancel(self.cancel, lm::chat_once(self.lm_url, self.model, &messages, &self.sampling)).await?;
        if !text.trim().is_empty() {
            set_str(self.db, &key, &text);
        }
        Ok(text)
    }

    pub async fn uncached(&self, messages: &Value) -> Result<String, String> {
        crate::pipeline::with_cancel(self.cancel, lm::chat_once(self.lm_url, self.model, messages, &self.sampling)).await
    }

    pub fn chunk_target(&self) -> usize {
        self.max_tokens.saturating_sub(1500).clamp(800, 6000)
    }

    // Map: конспект каждого блока; Reduce: слияние конспектов в пределах доли бюджета
    pub async fn map_reduce_file<F: FnMut(usize, usize)>(&self, path: &str, content: &str, share: usize, mut on_part: F) -> Result<(String, usize), String> {
        let chunks = logical_chunks(content, self.chunk_target());
        let n = chunks.len();
        let mut notes = String::new();
        for (k, (text, a, b)) in chunks.iter().enumerate() {
            on_part(k + 1, n);
            let msgs = json!([
                { "role": "system", "content": "You are a senior engineer preparing working notes for documentation writers. Analyze ONE fragment of a larger source file and extract dense, factual notes in English: the purpose of the fragment; every public/exported function, method, class, type and constant with parameters (names and types), return types and behavior; error handling and edge cases; important internal logic and data flow; dependencies used. Bullet points only, identifiers verbatim, no filler, no code beyond signatures. Never invent anything that is not present in the fragment." },
                { "role": "user", "content": format!("File: {} (lines {}-{}, part {}/{})\n\n```\n{}\n```", path, a, b, k + 1, n, text) }
            ]);
            let part = self.cached("map-v1", msgs).await?;
            notes.push_str(&format!("#### lines {}-{}\n{}\n\n", a, b, part.trim()));
        }
        if n > 1 && est_tokens(&notes) > share {
            let input = crate::pipeline::slice_by_tokens(&notes, self.max_tokens.saturating_sub(1500).max(800));
            let msgs = json!([
                { "role": "system", "content": format!("Merge the working notes about one source file into a single consolidated, de-duplicated set of notes of at most ~{} tokens. Keep every public API signature with parameters and return types, error handling, edge cases and the key internal logic; drop repetition. Bullet points, identifiers verbatim, English, no filler.", share) },
                { "role": "user", "content": format!("File: {}\n\nNotes:\n{}", path, input) }
            ]);
            notes = self.cached("reduce-v1", msgs).await?;
        }
        Ok((crate::pipeline::slice_by_tokens(&notes, share), n))
    }
}

// Контекст раздела: маленькие файлы целиком, большие — через map-reduce (вместо обрезки «с головы»)
pub async fn section_context<F: FnMut(&str, usize, usize)>(llm: &Llm<'_>, sec_files: &[String], contents: &HashMap<String, String>, budget: usize, mut on_part: F) -> Result<String, String> {
    let mut order: Vec<&String> = sec_files.iter().collect();
    order.sort_by_key(|f| contents.get(*f).map(|c| c.len()).unwrap_or(0));
    let mut blocks: HashMap<&String, String> = HashMap::new();
    let mut remaining = budget;
    let n = order.len();
    for (idx, f) in order.iter().enumerate() {
        let code = contents.get(*f).map(String::as_str).unwrap_or("");
        let share = (remaining / (n - idx)).max(300);
        let t = est_tokens(code);
        let block = if t <= share {
            format!("### {}\n```\n{}\n```\n\n", f, code)
        } else {
            let (notes, parts) = llm.map_reduce_file(f, code, share, |k, total| on_part(f, k, total)).await?;
            format!("### {} (large file: structured notes from a map-reduce pass over all {} parts — the whole file was analyzed)\n{}\n\n", f, parts, notes)
        };
        remaining = remaining.saturating_sub(est_tokens(&block));
        blocks.insert(f, block);
    }
    Ok(sec_files.iter().filter_map(|f| blocks.remove(f)).collect())
}

const GENERIC_STEMS: &[&str] = &["index", "mod", "main", "lib", "utils", "util", "types", "init", "__init__", "app", "config"];

fn stem_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name).to_lowercase();
    if GENERIC_STEMS.contains(&stem.as_str()) {
        // Для index.ts / mod.rs значим каталог
        path.rsplit('/').nth(1).map(|d| d.to_lowercase()).unwrap_or(stem)
    } else {
        stem
    }
}

// Токены из строк импорта: use/mod/import/from/require/#include
fn import_tokens(content: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for l in content.lines().take(400) {
        let t = l.trim_start();
        let is_import = ["use ", "pub use ", "mod ", "pub mod ", "import ", "from ", "#include", "export * from", "export {"].iter().any(|k| t.starts_with(k))
            || t.contains("require(")
            || t.contains("import(");
        if is_import {
            for w in t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')) {
                if w.len() >= 3 {
                    out.insert(w.to_lowercase());
                }
            }
        }
    }
    out
}

// Сигнатуры объявлений файла — компактный «интерфейс» связанного модуля
pub fn signatures(content: &str, budget_tokens: usize) -> String {
    let mut out = String::new();
    for l in content.lines() {
        let t = l.trim();
        if is_decl(t) && !t.starts_with("pub use") {
            let line: String = t.chars().take(200).collect();
            if est_tokens(&out) + est_tokens(&line) > budget_tokens {
                break;
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

pub struct EmbChunk {
    pub file: String,
    pub text: String,
    pub start: u64,
    pub end: u64,
    pub vector: Vec<f32>,
}

pub fn load_embeddings(db: &sled::Db, base: &str) -> Vec<EmbChunk> {
    let prefix = format!("emb:{}:", base);
    scan(db, &prefix)
        .into_iter()
        .filter_map(|(k, v)| {
            let rest = &k[prefix.len()..];
            let file = rest.rsplit_once(':')?.0.to_string();
            let j: Value = serde_json::from_slice(&v).ok()?;
            let vector: Vec<f32> = j["vector"].as_array()?.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            Some(EmbChunk { file, text: j["text"].as_str()?.to_string(), start: j["startLine"].as_u64().unwrap_or(0), end: j["endLine"].as_u64().unwrap_or(0), vector })
        })
        .collect()
}

// Связанный код из других разделов: импорты в обе стороны (сигнатуры) + семантически близкие фрагменты
pub fn related_context(sec_files: &[String], all_files: &[String], contents: &HashMap<String, String>, embs: &[EmbChunk], budget: usize) -> String {
    let in_sec: HashSet<&String> = sec_files.iter().collect();
    let mut sec_imports: HashSet<String> = HashSet::new();
    for f in sec_files {
        sec_imports.extend(import_tokens(contents.get(f).map(String::as_str).unwrap_or("")));
    }
    let sec_stems: HashSet<String> = sec_files.iter().map(|f| stem_of(f)).collect();
    let edges = crate::repo::dep_edges(all_files, contents, usize::MAX);
    let mut scored: Vec<(usize, &String)> = all_files
        .iter()
        .filter(|f| !in_sec.contains(f))
        .filter_map(|f| {
            let mut score = 0;
            if edges.iter().any(|(a, b)| (in_sec.contains(a) && b == f) || (a == f && in_sec.contains(b))) {
                score += 3;
            }
            if sec_imports.contains(&stem_of(f)) {
                score += 2;
            }
            if import_tokens(contents.get(f).map(String::as_str).unwrap_or("")).iter().any(|t| sec_stems.contains(t)) {
                score += 1;
            }
            (score > 0).then_some((score, f))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    scored.truncate(6);

    let sem_budget = if embs.is_empty() { 0 } else { budget / 2 };
    let imp_budget = budget - sem_budget;
    let mut out = String::new();
    if !scored.is_empty() {
        let per = (imp_budget / scored.len()).max(120);
        for (_, f) in &scored {
            let sig = signatures(contents.get(*f).map(String::as_str).unwrap_or(""), per);
            if !sig.is_empty() {
                out.push_str(&format!("#### {} (declarations)\n```\n{}```\n\n", f, sig));
            }
        }
    }
    if sem_budget > 0 {
        let sec_vecs: Vec<&Vec<f32>> = embs.iter().filter(|e| in_sec.contains(&e.file)).map(|e| &e.vector).collect();
        if let Some(dim) = sec_vecs.first().map(|v| v.len()) {
            let mut centroid = vec![0f32; dim];
            for v in &sec_vecs {
                for (c, x) in centroid.iter_mut().zip(v.iter()) {
                    *c += x;
                }
            }
            let mut cands: Vec<(f32, &EmbChunk)> = embs.iter().filter(|e| !in_sec.contains(&e.file)).map(|e| (cosine(&centroid, &e.vector), e)).collect();
            cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut used = 0;
            for (_, e) in cands.into_iter().take(4) {
                let text = crate::pipeline::slice_by_tokens(&e.text, (sem_budget / 4).max(150));
                used += est_tokens(&text);
                if used > sem_budget {
                    break;
                }
                out.push_str(&format!("#### {} (lines {}-{}, semantically related)\n```\n{}\n```\n\n", e.file, e.start, e.end, text));
            }
        }
    }
    if out.is_empty() {
        return out;
    }
    format!("\n\nRelated code from OTHER parts of the repository (context only — document the section files above; use this to explain integration points and how the section is used):\n\n{}", out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big_rust_file(fns: usize) -> String {
        (0..fns).map(|i| format!("/// doc {i}\npub fn func_{i}(a: u32) -> u32 {{\n{}    a + {i}\n}}\n", "    let x = 1;\n".repeat(30))).collect()
    }

    #[test]
    fn chunks_split_on_declarations() {
        let src = big_rust_file(40);
        let chunks = logical_chunks(&src, 800);
        assert!(chunks.len() > 2);
        for (text, a, _) in &chunks[1..] {
            assert!(text.starts_with("/// doc") || text.starts_with("pub fn"), "chunk at line {a} starts mid-function");
        }
        let total: usize = chunks.iter().map(|(_, a, b)| b - a + 1).sum();
        assert_eq!(total, src.lines().count());
    }

    #[test]
    fn signatures_and_related() {
        let mut contents = HashMap::new();
        contents.insert("src/a.rs".to_string(), "use crate::store;\npub fn run() {}\n".to_string());
        contents.insert("src/store.rs".to_string(), "pub struct Store {}\nimpl Store {\n    pub fn get(&self, k: &str) -> Option<String> { None }\n}\n".to_string());
        contents.insert("src/other.rs".to_string(), "pub fn unrelated() {}\n".to_string());
        let files: Vec<String> = vec!["src/a.rs".into(), "src/other.rs".into(), "src/store.rs".into()];
        let ctx = related_context(&["src/a.rs".to_string()], &files, &contents, &[], 600);
        assert!(ctx.contains("src/store.rs") && ctx.contains("pub fn get(&self, k: &str)"));
        assert!(!ctx.contains("src/other.rs"));
    }

    #[test]
    fn rules_name_language() {
        assert!(lang_rules("russian").contains("STRICTLY in Russian"));
    }

    // Против мок-LM (node /app/tests/mock_lm.js): большой файл проходит map-reduce, малый — целиком
    #[tokio::test]
    async fn map_reduce_against_mock() {
        if reqwest::get("http://localhost:1234/v1/models").await.is_err() {
            eprintln!("mock LM не запущен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join(format!("cw-q-{}", std::process::id()));
        let db = sled::open(&dir).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let llm = Llm { db: &db, lm_url: Some("http://localhost:1234"), model: "mock-model", sampling: Sampling::default(), cancel: &cancel, max_tokens: 3000 };
        let mut contents = HashMap::new();
        contents.insert("big.rs".to_string(), big_rust_file(120));
        contents.insert("small.rs".to_string(), "pub fn tiny() {}\n".to_string());
        let files = vec!["big.rs".to_string(), "small.rs".to_string()];
        let mut parts = 0;
        let ctx = section_context(&llm, &files, &contents, 2000, |_, _, n| parts = n).await.unwrap();
        assert!(parts > 1, "big file must be split");
        assert!(ctx.contains("### big.rs (large file") && ctx.contains("pub fn tiny()"));
        assert!(est_tokens(&ctx) < 2600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
