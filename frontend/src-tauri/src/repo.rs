// Клонирование GitHub-репозиториев и сбор файлов (паритет с backend/git.js + pipeline.collectFiles)
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

const IGNORE_DIRS: &[&str] = &[
    "node_modules", ".git", "dist", "build", "target", "vendor", "coverage", ".next", ".nuxt",
    "__pycache__", "venv", ".venv", "out", ".idea", ".vscode", "bin", "obj", ".emergent",
];
const IGNORE_FILES: &[&str] = &[
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "cargo.lock", "composer.lock",
    "poetry.lock", ".ds_store",
];
const ALLOWED_EXT: &[&str] = &[
    "js", "jsx", "ts", "tsx", "vue", "svelte", "py", "rb", "go", "rs", "java", "kt", "swift",
    "c", "h", "cpp", "hpp", "cc", "cs", "php", "scala", "sh", "bash", "sql", "html", "css",
    "scss", "less", "json", "yaml", "yml", "toml", "ini", "cfg", "md", "markdown", "txt",
    "xml", "graphql", "proto", "lua", "dart", "ex", "exs",
];

pub fn parse_repo_url(repo_url: &str) -> Result<(String, String, String), String> {
    let s = repo_url.trim().trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let idx = s.find("github.com").ok_or("Некорректный GitHub URL. Ожидается https://github.com/owner/repo")?;
    let rest = &s[idx + "github.com".len()..];
    let rest = rest.trim_start_matches([':', '/']);
    let mut parts = rest.split('/');
    let owner = parts.next().unwrap_or("").to_string();
    let name = parts.next().unwrap_or("").to_string();
    if owner.is_empty() || name.is_empty() {
        return Err("Некорректный GitHub URL. Ожидается https://github.com/owner/repo".into());
    }
    let key = format!("{}/{}", owner, name);
    Ok((owner, name, key))
}

pub fn clone_repo(repo_url: &str, repos_dir: &Path, github_token: Option<&str>) -> Result<(String, PathBuf), String> {
    let (owner, name, key) = parse_repo_url(repo_url)?;
    let clone_url = format!("https://github.com/{}/{}.git", owner, name);
    let target = repos_dir.join(format!("{}__{}", owner, name));
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(repos_dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1", &clone_url])
        .arg(&target)
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(token) = github_token.filter(|t| !t.is_empty()) {
        // PAT через одноразовый http.extraHeader: токен не попадает в URL, логи и .git/config
        use base64::Engine as _;
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{}", token));
        cmd.env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env("GIT_CONFIG_VALUE_0", format!("Authorization: Basic {}", auth));
    }
    let out = cmd.output().map_err(|e| format!("git не найден в PATH: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git clone failed: {}", err.chars().rev().take(400).collect::<String>().chars().rev().collect::<String>()));
    }
    Ok((key, target))
}

// Сбор текстовых файлов с учётом .gitignore (crate `ignore`), лимитов размера и количества
pub fn collect_files(root: &Path, max_files: usize) -> Vec<(String, String)> {
    let max_size: u64 = 200 * 1024;
    let mut files: Vec<(String, String)> = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .build();
    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let lower_name = entry.file_name().to_string_lossy().to_lowercase();
        if IGNORE_FILES.contains(&lower_name.as_str()) {
            continue;
        }
        if rel.split('/').any(|part| IGNORE_DIRS.contains(&part.to_lowercase().as_str())) {
            continue;
        }
        let ext = entry.path().extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !ALLOWED_EXT.contains(&ext.as_str()) && lower_name != "dockerfile" && lower_name != "makefile" {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size == 0 || size > max_size {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            files.push((rel, content));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.truncate(max_files);
    files
}

// Дерево файлов той же формы, что ждёт фронтенд (TreeNode: name/path/type/children)
pub fn build_tree(paths: &[String]) -> Value {
    fn insert(level: &mut Vec<Value>, parts: &[&str], acc: String) {
        if parts.is_empty() {
            return;
        }
        let part = parts[0];
        let path = if acc.is_empty() { part.to_string() } else { format!("{}/{}", acc, part) };
        let is_file = parts.len() == 1;
        let pos = level.iter().position(|n| n["name"] == part);
        let idx = match pos {
            Some(i) => i,
            None => {
                let mut node = json!({ "name": part, "path": path, "type": if is_file { "file" } else { "dir" } });
                if !is_file {
                    node["children"] = json!([]);
                }
                level.push(node);
                level.len() - 1
            }
        };
        if !is_file {
            if let Some(children) = level[idx]["children"].as_array_mut() {
                insert(children, &parts[1..], path);
                return;
            }
        }
        let _ = path;
    }
    fn sort_tree(nodes: &mut Vec<Value>) {
        nodes.sort_by(|a, b| {
            let (ta, tb) = (a["type"].as_str().unwrap_or(""), b["type"].as_str().unwrap_or(""));
            if ta == tb {
                a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
            } else if ta == "dir" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
        for n in nodes.iter_mut() {
            if let Some(ch) = n["children"].as_array_mut() {
                sort_tree(ch);
            }
        }
    }
    let mut root: Vec<Value> = Vec::new();
    for p in paths {
        let parts: Vec<&str> = p.split('/').collect();
        insert(&mut root, &parts, String::new());
    }
    sort_tree(&mut root);
    Value::Array(root)
}

// Извлечение зависимостей import/require между файлами (для вкладки «Архитектура»)
pub fn extract_deps(files: &[String], contents: &std::collections::HashMap<String, String>) -> Value {
    Value::Array(dep_edges(files, contents, 80).into_iter().map(|(a, b)| json!([a, b])).collect())
}

// Рёбра «файл → импортируемый файл» по относительным import/require (не более limit)
pub fn dep_edges(files: &[String], contents: &std::collections::HashMap<String, String>, limit: usize) -> Vec<(String, String)> {
    let set: std::collections::HashSet<&String> = files.iter().collect();
    let mut edges: Vec<(String, String)> = Vec::new();
    for f in files {
        let content = match contents.get(f) {
            Some(c) => c,
            None => continue,
        };
        let dir = f.rfind('/').map(|i| &f[..i]).unwrap_or("");
        for cap in find_rel_imports(content) {
            let raw = if dir.is_empty() { cap.clone() } else { format!("{}/{}", dir, cap) };
            let mut stack: Vec<&str> = Vec::new();
            for p in raw.split('/') {
                match p {
                    "." | "" => {}
                    ".." => {
                        stack.pop();
                    }
                    other => stack.push(other),
                }
            }
            let base = stack.join("/");
            let target = if set.contains(&base) {
                Some(base.clone())
            } else {
                [".js", ".ts", ".jsx", ".tsx", ".vue", ".py", "/index.js"]
                    .iter()
                    .map(|e| format!("{}{}", base, e))
                    .find(|c| set.contains(c))
            };
            if let Some(t) = target {
                if &t != f {
                    edges.push((f.clone(), t));
                }
            }
        }
        if edges.len() >= limit {
            break;
        }
    }
    edges.truncate(limit);
    edges
}

// Поиск относительных путей в import/require/from без regex-крейта
fn find_rel_imports(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let q = bytes[i];
            if let Some(end) = content[i + 1..].find(q as char) {
                let s = &content[i + 1..i + 1 + end];
                if (s.starts_with("./") || s.starts_with("../")) && s.len() < 200 {
                    // проверяем, что перед кавычкой было import/require/from
                    let before: String = content[..i].chars().rev().take(40).collect::<String>().chars().rev().collect();
                    if before.contains("import") || before.contains("require") || before.contains("from") {
                        out.push(s.to_string());
                    }
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}
