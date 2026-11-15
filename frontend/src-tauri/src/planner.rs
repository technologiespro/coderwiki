// План wiki как на DeepWiki: главы → подразделы с осмысленными названиями.
// Модель возвращает только оглавление и пути-префиксы (компактный JSON), файлы раскладываются детерминированно.
use crate::quality::Llm;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

pub type Plan = Vec<(String, Vec<String>)>;

pub fn overview_title_for(language: &str) -> &'static str {
    match language {
        "russian" => "Обзор",
        "chinese" => "概述",
        "japanese" => "概要",
        "spanish" => "Visión general",
        "german" => "Überblick",
        "portuguese" => "Visão geral",
        "french" => "Vue d'ensemble",
        "arabic" => "نظرة عامة",
        "hindi" => "अवलोकन",
        "indonesian" => "Ikhtisar",
        "vietnamese" => "Tổng quan",
        "kazakh" => "Шолу",
        "swahili" => "Muhtasari",
        _ => "Overview",
    }
}

pub fn other_title_for(language: &str) -> &'static str {
    match language {
        "russian" => "Прочие файлы",
        "chinese" => "其他文件",
        "japanese" => "その他のファイル",
        "spanish" => "Otros archivos",
        "german" => "Weitere Dateien",
        "portuguese" => "Outros arquivos",
        "french" => "Autres fichiers",
        "arabic" => "ملفات أخرى",
        "hindi" => "अन्य फ़ाइलें",
        "indonesian" => "Berkas lainnya",
        "vietnamese" => "Tệp khác",
        "kazakh" => "Басқа файлдар",
        "swahili" => "Faili nyingine",
        _ => "Other Files",
    }
}

// Целевое число страниц по размеру репозитория (с запасом — «ничего не упускать»)
pub fn page_range(n_files: usize) -> (usize, usize) {
    match n_files {
        0..=8 => (3, 5),
        9..=25 => (6, 10),
        26..=60 => (10, 16),
        61..=150 => (15, 24),
        _ => (20, 30),
    }
}

fn dir_of(f: &str) -> &str {
    f.rfind('/').map(|i| &f[..i]).unwrap_or("")
}

fn name_of(f: &str) -> &str {
    f.rsplit('/').next().unwrap_or(f)
}

// Сжатое дерево репозитория: каталог → файлы с подсказкой первой строки; ужимается под бюджет
pub fn repo_outline(files: &[String], hints: &HashMap<String, String>, budget_chars: usize) -> String {
    let mut dirs: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for f in files {
        dirs.entry(dir_of(f)).or_default().push(f);
    }
    for hint_len in [70usize, 30, 0] {
        let mut out = String::new();
        for (d, fl) in &dirs {
            out.push_str(&format!("{}/ ({} files)\n", if d.is_empty() { "." } else { d }, fl.len()));
            for f in fl {
                let h: String = hints.get(*f).map(|h| h.chars().take(hint_len).collect()).unwrap_or_default();
                if h.is_empty() {
                    out.push_str(&format!("  {}\n", name_of(f)));
                } else {
                    out.push_str(&format!("  {} — {}\n", name_of(f), h));
                }
            }
        }
        if out.len() <= budget_chars || hint_len == 0 {
            if out.len() <= budget_chars {
                return out;
            }
            // Совсем большой репозиторий: только каталоги с примерами имён
            let mut short = String::new();
            for (d, fl) in &dirs {
                let names: Vec<&str> = fl.iter().take(8).map(|f| name_of(f)).collect();
                short.push_str(&format!("{}/ ({} files): {}{}\n", if d.is_empty() { "." } else { d }, fl.len(), names.join(", "), if fl.len() > 8 { ", …" } else { "" }));
            }
            return short.chars().take(budget_chars).collect();
        }
    }
    String::new()
}

fn lang_name(language: &str) -> String {
    let mut c = language.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn system_prompt(language: &str, n_files: usize) -> String {
    let (lo, hi) = page_range(n_files);
    format!(
        "You are an expert software architect designing the table of contents of a documentation wiki exactly like DeepWiki (deepwiki.com). \
Produce {lo}-{hi} pages grouped into chapters (2-5 pages per chapter; a chapter may have a single page). Rules: \
1) The FIRST chapter is the project overview; its first page is a high-level overview, its second page is getting started (installation, configuration, running) when a README or manifest exists. \
2) Order chapters from high-level (overview, architecture, configuration) to specific subsystems, then APIs/tooling, then operations, deployment, CI and tests. \
3) Titles must be SPECIFIC and descriptive like an architect would write them, naming the real concepts, components, protocols and technologies of THIS project, e.g. \"Storage Layer (Sled)\", \"Mempool & Antispam\", \"Validation Rules & Dynamic Fees\", \"Network Files: network.json, genesis\". \
Keep titles concise (2-7 words), no filler like \"Overview of\" or \"Implementation and Usage\". \
NEVER use bare directory or file names as titles and never titles like \"Module src\", \"src\", \"Root\", \"Misc\", \"Utils\". \
4) Each page should cover a focused topic of about 1-15 files: split large directories into several pages by their subdirectories or files and use the DEEPEST matching directory prefixes (\"src/crypto/\" rather than \"src/\"). Every subdirectory with source code deserves its own page or a clear place in one. Cover EVERY directory of the outline. \
5) For each page give \"paths\": directory prefixes ending with \"/\" (e.g. \"src/network/\") or exact file paths, relative to the repository root, exactly as in the outline. \
6) Chapter and page titles strictly in {lang}; keep identifiers, file names and product names unchanged. \
Respond with ONLY a valid JSON array, no markdown fences, no comments: \
[{{\"chapter\":\"...\",\"sections\":[{{\"title\":\"...\",\"paths\":[\"dir/\",\"dir/file.ext\"]}}]}}]",
        lo = lo,
        hi = hi,
        lang = lang_name(language)
    )
}

// Ремонт типичных огрехов локальных моделей: ```json-обёртка, хвостовые запятые
fn parse_json_array(raw: &str) -> Option<Vec<Value>> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    if end <= start {
        return None;
    }
    let mut s = raw[start..=end].to_string();
    loop {
        let fixed = s.replace(",]", "]").replace(",}", "}").replace(", ]", "]").replace(", }", "}").replace(",\n]", "]").replace(",\n}", "}");
        if fixed == s {
            break;
        }
        s = fixed;
    }
    serde_json::from_str::<Value>(&s).ok()?.as_array().cloned()
}

struct Page {
    chapter: Option<String>,
    title: String,
    paths: Vec<String>,
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().trim_start_matches("./").to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

fn parse_toc(raw: &str) -> Vec<Page> {
    let mut pages = Vec::new();
    for item in parse_json_array(raw).unwrap_or_default() {
        let paths_of = |v: &Value| {
            let mut p = str_list(&v["paths"]);
            p.extend(str_list(&v["files"]));
            p
        };
        if let (Some(ch), Some(subs)) = (item["chapter"].as_str(), item["sections"].as_array()) {
            for s in subs {
                if let Some(t) = s["title"].as_str().filter(|t| !t.trim().is_empty()) {
                    pages.push(Page { chapter: Some(ch.trim().to_string()), title: t.trim().to_string(), paths: paths_of(s) });
                }
            }
        } else if let Some(t) = item["title"].as_str().filter(|t| !t.trim().is_empty()) {
            pages.push(Page { chapter: item["group"].as_str().map(|g| g.trim().to_string()), title: t.trim().to_string(), paths: paths_of(&item) });
        }
    }
    pages
}

fn tokens(s: &str) -> HashSet<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 3).map(String::from).collect()
}

// Файл → страница: самый длинный совпавший путь; иначе общие слова пути и заголовка/путей; иначе соседи по каталогу
pub fn assign_files(pages_paths: &[(String, Vec<String>)], files: &[String]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![Vec::new(); pages_paths.len()];
    let mut unassigned = Vec::new();
    for f in files {
        let mut best: Option<(usize, usize)> = None;
        for (i, (_, paths)) in pages_paths.iter().enumerate() {
            for p in paths {
                let score = if p == f {
                    10_000
                } else if p.ends_with('/') && f.starts_with(p.as_str()) {
                    p.len()
                } else if !p.ends_with('/') && f.starts_with(&format!("{}/", p)) {
                    p.len() + 1
                } else {
                    0
                };
                if score > 0 && best.map_or(true, |(_, b)| score > b) {
                    best = Some((i, score));
                }
            }
        }
        match best {
            Some((i, _)) => out[i].push(f.clone()),
            None => unassigned.push(f.clone()),
        }
    }
    let mut rest = Vec::new();
    for f in unassigned {
        let ft = tokens(&f);
        let mut best: Option<(usize, usize)> = None;
        for (i, (title, paths)) in pages_paths.iter().enumerate() {
            let mut pt = tokens(title);
            for p in paths {
                pt.extend(tokens(p));
            }
            let score = ft.intersection(&pt).count();
            if score > 0 && best.map_or(true, |(_, b)| score > b) {
                best = Some((i, score));
            }
        }
        if best.is_none() {
            let d = dir_of(&f);
            let mut counts: HashMap<usize, usize> = HashMap::new();
            for (i, fl) in out.iter().enumerate() {
                let c = fl.iter().filter(|x| dir_of(x) == d).count();
                if c > 0 {
                    counts.insert(i, c);
                }
            }
            best = counts.into_iter().max_by_key(|(i, c)| (*c, usize::MAX - i));
        }
        match best {
            Some((i, _)) => out[i].push(f),
            None => rest.push(f),
        }
    }
    out.push(rest);
    out
}

fn humanize(seg: &str) -> String {
    seg.split(|c: char| c == '_' || c == '-' || c == '.')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// Запасной план без LLM: глава = каталог верхнего уровня, крупные каталоги делятся по подкаталогам
pub fn fallback_plan(files: &[String], language: &str, groups: &mut HashMap<String, String>) -> Plan {
    let overview = overview_title_for(language).to_string();
    let mut by_top: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut root = Vec::new();
    for f in files {
        match f.find('/') {
            Some(i) => by_top.entry(f[..i].to_string()).or_default().push(f.clone()),
            None => root.push(f.clone()),
        }
    }
    let mut plan: Plan = Vec::new();
    if !root.is_empty() {
        groups.insert(overview.clone(), overview.clone());
        plan.push((overview.clone(), root));
    }
    for (top, fl) in by_top {
        let chapter = humanize(&top);
        let mut subs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if fl.len() > 15 {
            for f in &fl {
                let rest = &f[top.len() + 1..];
                let key = rest.find('/').map(|i| humanize(&rest[..i])).unwrap_or_else(|| chapter.clone());
                subs.entry(key).or_default().push(f.clone());
            }
        } else {
            subs.insert(chapter.clone(), fl);
        }
        for (title, sfl) in subs {
            let title = if title != chapter && plan.iter().any(|(t, _)| *t == title) { format!("{} ({})", title, chapter) } else { title };
            groups.insert(title.clone(), chapter.clone());
            plan.push((title, sfl));
        }
    }
    plan
}

// Оглавление → план; пустые страницы отбрасываются, дубли названий уточняются главой
fn build_plan(pages: Vec<Page>, files: &[String], language: &str, groups: &mut HashMap<String, String>) -> Option<Plan> {
    if pages.len() < 2 {
        return None;
    }
    let pp: Vec<(String, Vec<String>)> = pages.iter().map(|p| (p.title.clone(), p.paths.clone())).collect();
    let assigned = assign_files(&pp, files);
    let mut plan: Plan = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let fl = assigned[i].clone();
        if fl.is_empty() {
            continue;
        }
        let mut title = page.title.clone();
        if plan.iter().any(|(t, _)| *t == title) {
            title = format!("{} ({})", title, page.chapter.clone().unwrap_or_else(|| (i + 1).to_string()));
        }
        if let Some(ch) = &page.chapter {
            groups.insert(title.clone(), ch.clone());
        }
        plan.push((title, fl));
    }
    let rest = assigned.last().cloned().unwrap_or_default();
    if !rest.is_empty() {
        let title = other_title_for(language).to_string();
        if let Some(ch) = pages.last().and_then(|p| p.chapter.clone()) {
            groups.insert(title.clone(), ch);
        }
        plan.push((title, rest));
    }
    // Слишком мало страниц для большого репозитория — модель «схалтурила»
    let (lo, _) = page_range(files.len());
    if plan.len() < 2 || plan.len() < lo / 2 {
        return None;
    }
    Some(plan)
}

// Две попытки у модели (вторая — с указанием на ошибку), затем запасной план. ok=false — план не кэшировать
pub async fn plan_wiki(llm: &Llm<'_>, repo_key: &str, files: &[String], hints: &HashMap<String, String>, readme: &str, language: &str, groups: &mut HashMap<String, String>) -> Result<(Plan, bool), String> {
    let budget_chars = llm.max_tokens.saturating_sub(2500).max(1500) * 7 / 2;
    let readme: String = readme.chars().take((budget_chars / 6).min(3000)).collect();
    let outline = repo_outline(files, hints, budget_chars.saturating_sub(readme.len()));
    let user = format!("Repository: {}\nFiles: {}\n\nREADME excerpt:\n{}\n\nRepository outline (directory/ (count) then files — first-line hint):\n{}", repo_key, files.len(), readme, outline);
    let mut messages = json!([
        { "role": "system", "content": system_prompt(language, files.len()) },
        { "role": "user", "content": user }
    ]);
    for attempt in 0..2 {
        let raw = llm.uncached(&messages).await?;
        let mut g = HashMap::new();
        if let Some(plan) = build_plan(parse_toc(&raw), files, language, &mut g) {
            groups.extend(g);
            return Ok((plan, true));
        }
        if attempt == 0 {
            let arr = messages.as_array_mut().unwrap();
            arr.push(json!({ "role": "assistant", "content": raw.chars().take(4000).collect::<String>() }));
            arr.push(json!({ "role": "user", "content": format!("That answer was not usable: it must be a valid JSON array of chapters with {}-{} specific, non-directory page titles, and \"paths\" taken from the outline. Respond again with ONLY the JSON.", page_range(files.len()).0, page_range(files.len()).1) }));
        }
    }
    Ok((fallback_plan(files, language, groups), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<String> {
        ["README.md", "Cargo.toml", "src/main.rs", "src/network/p2p.rs", "src/network/peer.rs", "src/storage/db.rs", "src/crypto/keys.rs", "src/crypto/pq.rs", "tests/sync.rs", "scripts/deploy.sh"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_toc_with_fences_and_trailing_commas() {
        let raw = "```json\n[{\"chapter\":\"Overview\",\"sections\":[{\"title\":\"Project Overview\",\"paths\":[\"README.md\",\"Cargo.toml\",],},]},{\"chapter\":\"Networking\",\"sections\":[{\"title\":\"P2P Layer\",\"paths\":[\"src/network/\"]}]}]\n```";
        let pages = parse_toc(raw);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].paths, vec!["src/network/"]);
    }

    #[test]
    fn assigns_by_longest_prefix_then_tokens() {
        let pp = vec![
            ("Overview".to_string(), vec!["README.md".to_string(), "src/".to_string()]),
            ("P2P Networking".to_string(), vec!["src/network/".to_string()]),
            ("Cryptography".to_string(), vec!["src/crypto".to_string()]),
            ("Tests & Deployment".to_string(), vec![]),
        ];
        let a = assign_files(&pp, &files());
        assert!(a[1].contains(&"src/network/p2p.rs".to_string()));
        assert!(a[2].contains(&"src/crypto/pq.rs".to_string()));
        assert!(a[0].contains(&"src/main.rs".to_string()));
        assert!(a[3].contains(&"tests/sync.rs".to_string()));
        assert_eq!(a.iter().map(|x| x.len()).sum::<usize>(), files().len());
    }

    #[test]
    fn fallback_has_no_module_prefix_and_is_localized() {
        let mut g = HashMap::new();
        let plan = fallback_plan(&files(), "english", &mut g);
        assert_eq!(plan[0].0, "Overview");
        assert!(plan.iter().all(|(t, _)| !t.starts_with("Модуль")));
        assert!(plan.iter().any(|(t, _)| t == "Src"));
    }

    #[test]
    fn outline_fits_budget() {
        let many: Vec<String> = (0..2000).map(|i| format!("src/mod{}/file{}.rs", i % 40, i)).collect();
        let o = repo_outline(&many, &HashMap::new(), 6000);
        assert!(o.len() <= 6000 && o.contains("src/mod1/"));
    }
}

// Ручная проверка промпта на реальном репозитории: CW_REPO=путь cargo test plan_dump -- --ignored
#[cfg(test)]
mod manual {
    use super::*;

    #[test]
    #[ignore]
    fn plan_dump() {
        let root = std::env::var("CW_REPO").expect("CW_REPO");
        let collected = crate::repo::collect_files(std::path::Path::new(&root), 300);
        let files: Vec<String> = collected.iter().map(|(p, _)| p.clone()).collect();
        let hints: HashMap<String, String> = collected.iter().map(|(p, c)| (p.clone(), c.lines().find(|l| l.trim().len() > 2).unwrap_or("").trim().chars().take(80).collect())).collect();
        let readme = collected.iter().find(|(p, _)| p.to_lowercase() == "readme.md").map(|(_, c)| c.chars().take(3000).collect::<String>()).unwrap_or_default();
        let outline = repo_outline(&files, &hints, 5500 * 7 / 2 - readme.len());
        let lang = std::env::var("CW_LANG").unwrap_or("english".into());
        let msgs = json!([{ "role": "system", "content": system_prompt(&lang, files.len()) }, { "role": "user", "content": format!("Repository: x\nFiles: {}\n\nREADME excerpt:\n{}\n\nRepository outline:\n{}", files.len(), readme, outline) }]);
        std::fs::write("/tmp/plan_msgs.json", msgs.to_string()).unwrap();
        if let Ok(raw) = std::fs::read_to_string("/tmp/plan_raw.txt") {
            let mut g = HashMap::new();
            match build_plan(parse_toc(&raw), &files, &lang, &mut g) {
                Some(plan) => {
                    for (t, fl) in &plan {
                        println!("[{}] {} — {} files", g.get(t).cloned().unwrap_or_default(), t, fl.len());
                    }
                }
                None => println!("PLAN REJECTED"),
            }
        }
        println!("files: {}", files.len());
    }
}
