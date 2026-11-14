// Экспорт/импорт .xdp в desktop app. Ядро формата (XDP2, ZSTD, генетический словарь) —
// общая библиотека xdp-util (/app/xdp-rust), та же, что у CLI-утилиты web-версии.
use crate::state::{get_json, scan, set_json, set_str};
use serde_json::{json, Value};
use std::io::Read;
use xdp_util::{pack_bytes, unpack_bytes, PackOptions};

fn add_entry(b: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8]) {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o644);
    let _ = b.append_data(&mut h, name, data);
}

// Сборка tar с содержимым проекта (тот же layout, что packager.js) и упаковка в XDP2
pub fn build_xdp<F: FnMut(String)>(db: &sled::Db, repo_key: &str, on_line: F) -> Result<Vec<u8>, String> {
    let record = get_json(db, &format!("repo:{}", repo_key)).ok_or("Репозиторий не найден")?;
    let base = crate::state::base_repo(repo_key).to_string();
    // Все языковые версии репозитория — в один контейнер (xdp/2)
    let all_records: Vec<Value> = scan(db, "repo:")
        .into_iter()
        .filter_map(|(k, v)| {
            let key = &k[5..];
            if crate::state::base_repo(key) != base {
                return None;
            }
            let mut rec: Value = serde_json::from_slice(&v).ok()?;
            rec["repoKey"] = json!(key);
            Some(rec)
        })
        .collect();
    let src_prefix = format!("src:{}:", base);
    let srcs = scan(db, &src_prefix);
    let embs = scan(db, &format!("emb:{}:", base));

    let mut b = tar::Builder::new(Vec::new());
    let languages: Vec<Value> = all_records.iter().map(|r| r["language"].clone()).collect();
    let manifest = json!({
        "format": "xdp/2",
        "repoKey": repo_key,
        "baseKey": base,
        "languages": languages,
        "generatedAt": record["generatedAt"],
        "language": record["language"],
        "modelId": record["modelId"],
    });
    add_entry(&mut b, "manifest.json", serde_json::to_string_pretty(&manifest).unwrap().as_bytes());
    // record.json — версия, из которой запущен экспорт (совместимость со старыми импортёрами)
    add_entry(&mut b, "record.json", serde_json::to_string(&record).unwrap().as_bytes());
    add_entry(&mut b, "records.json", serde_json::to_string(&all_records).unwrap().as_bytes());
    if let Some(ov) = record["overview"].as_str() {
        add_entry(&mut b, "docs/_OVERVIEW.md", ov.as_bytes());
    }
    for rec in &all_records {
        let lang = rec["language"].as_str().unwrap_or("default");
        if let Some(sections) = rec["sections"].as_array() {
            for s in sections {
                let id = s["id"].as_str().unwrap_or("section");
                let md = s["md"].as_str().unwrap_or("");
                add_entry(&mut b, &format!("sections/{}/{}.md", lang, id), md.as_bytes());
            }
        }
    }
    for (key, value) in &srcs {
        add_entry(&mut b, &format!("src/{}", &key[src_prefix.len()..]), value);
    }
    let vectors: Vec<Value> = embs
        .iter()
        .filter_map(|(k, v)| {
            serde_json::from_slice::<Value>(v).ok().map(|mut val| {
                val["key"] = json!(k);
                val
            })
        })
        .collect();
    add_entry(&mut b, "index/vectors.json", serde_json::to_string(&vectors).unwrap().as_bytes());
    let tar_bytes = b.into_inner().map_err(|e| e.to_string())?;
    pack_bytes(&tar_bytes, &PackOptions::default(), on_line)
}

pub fn import_xdp(db: &sled::Db, data_dir: &std::path::PathBuf, bytes: &[u8]) -> Result<Vec<String>, String> {
    let tar_bytes = unpack_bytes(bytes)?;
    let mut archive = tar::Archive::new(tar_bytes.as_slice());
    let mut record: Option<Value> = None;
    let mut records_json: Option<Value> = None;
    let mut srcs: Vec<(String, String)> = Vec::new();
    let mut vectors: Vec<Value> = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())?.flatten() {
        let mut e = entry;
        let name = e.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let mut buf = Vec::new();
        if e.read_to_end(&mut buf).is_err() {
            continue;
        }
        if name == "record.json" {
            record = serde_json::from_slice(&buf).ok();
        } else if name == "records.json" {
            records_json = serde_json::from_slice(&buf).ok();
        } else if let Some(rel) = name.strip_prefix("src/") {
            srcs.push((rel.to_string(), String::from_utf8_lossy(&buf).to_string()));
        } else if name == "index/vectors.json" {
            if let Ok(Value::Array(arr)) = serde_json::from_slice::<Value>(&buf) {
                vectors = arr;
            }
        }
    }
    // xdp/2: records.json со всеми языковыми версиями; иначе одиночный record.json
    let records: Vec<Value> = match records_json {
        Some(Value::Array(arr)) if !arr.is_empty() => arr,
        _ => vec![record.ok_or("Файл .xdp не содержит record.json")?],
    };
    let mut repo_keys: Vec<String> = Vec::new();
    for rec in &records {
        let repo_key = match rec["repoKey"].as_str() {
            Some(k) if !(rec["sections"].is_null() && rec["docs"].is_null()) => k.to_string(),
            _ => continue,
        };
        set_json(db, &format!("repo:{}", repo_key), rec);
        // .md файлы разделов в data/docs/
        let out_dir = crate::state::repo_docs_dir(data_dir, &repo_key);
        let _ = std::fs::remove_dir_all(&out_dir);
        let _ = std::fs::create_dir_all(&out_dir);
        if let Some(sections) = rec["sections"].as_array() {
            for s in sections {
                let id = s["id"].as_str().unwrap_or("section");
                let _ = std::fs::write(out_dir.join(format!("{}.md", id)), s["md"].as_str().unwrap_or(""));
            }
        }
        repo_keys.push(repo_key);
    }
    if repo_keys.is_empty() {
        return Err("Повреждённый record.json в .xdp".into());
    }
    let base = crate::state::base_repo(&repo_keys[0]).to_string();
    for (rel, content) in srcs {
        set_str(db, &format!("src:{}:{}", base, rel), &content);
    }
    for mut v in vectors {
        if let Some(key) = v["key"].as_str().map(String::from) {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("key");
            }
            set_json(db, &key, &v);
        }
    }
    Ok(repo_keys)
}
