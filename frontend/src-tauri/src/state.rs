use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub db: sled::Db,
    pub data_dir: PathBuf,
    pub cancel: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(db: sled::Db, data_dir: PathBuf) -> Self {
        Self { db, data_dir, cancel: Arc::new(AtomicBool::new(false)) }
    }
}

pub fn get_json(db: &sled::Db, key: &str) -> Option<Value> {
    db.get(key).ok().flatten().and_then(|v| serde_json::from_slice(&v).ok())
}

pub fn set_json(db: &sled::Db, key: &str, val: &Value) {
    let _ = db.insert(key, serde_json::to_vec(val).unwrap_or_default());
}

pub fn get_str(db: &sled::Db, key: &str) -> Option<String> {
    db.get(key).ok().flatten().map(|v| String::from_utf8_lossy(&v).to_string())
}

pub fn set_str(db: &sled::Db, key: &str, val: &str) {
    let _ = db.insert(key, val.as_bytes());
}

pub fn del_prefix(db: &sled::Db, prefix: &str) {
    let keys: Vec<sled::IVec> = db.scan_prefix(prefix).filter_map(|r| r.ok()).map(|(k, _)| k).collect();
    for k in keys {
        let _ = db.remove(k);
    }
}

pub fn scan(db: &sled::Db, prefix: &str) -> Vec<(String, Vec<u8>)> {
    db.scan_prefix(prefix)
        .filter_map(|r| r.ok())
        .map(|(k, v)| (String::from_utf8_lossy(&k).to_string(), v.to_vec()))
        .collect()
}

// Языковые версии делят исходники и RAG-индекс по базовому ключу (без @language)
pub fn base_repo(k: &str) -> &str {
    k.split('@').next().unwrap_or(k)
}

pub fn repo_docs_dir(data_dir: &PathBuf, repo_key: &str) -> PathBuf {
    data_dir.join("docs").join(repo_key.replace('/', "__"))
}
