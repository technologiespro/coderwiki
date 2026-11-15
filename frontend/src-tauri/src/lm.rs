// Клиент LM Studio (OpenAI-совместимый API)
use futures_util::StreamExt;
use serde_json::{json, Value};

pub fn base_url(url: Option<&str>) -> String {
    let mut u = url.unwrap_or("http://localhost:1234").trim().trim_end_matches('/').to_string();
    if u.ends_with("/v1") {
        u.truncate(u.len() - 3);
        u = u.trim_end_matches('/').to_string();
    }
    u
}

pub async fn fetch_models(lm_url: Option<&str>) -> Result<Value, String> {
    let url = format!("{}/v1/models", base_url(lm_url));
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("LM Studio недоступен ({}): {}", url, e))?;
    resp.json::<Value>().await.map_err(|e| format!("Некорректный ответ LM Studio: {}", e))
}

// Параметры сэмплирования: для технической документации — точность важнее креативности
#[derive(Clone, Copy, Debug)]
pub struct Sampling {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
    pub frequency_penalty: f64,
}

impl Default for Sampling {
    fn default() -> Self {
        Self { temperature: 0.2, top_p: 0.9, top_k: 40, frequency_penalty: 0.1 }
    }
}

// Стриминговый chat completions; on_delta вызывается на каждый фрагмент
pub async fn chat_stream<F: FnMut(&str)>(
    lm_url: Option<&str>,
    model: &str,
    messages: &Value,
    sampling: &Sampling,
    mut on_delta: F,
) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", base_url(lm_url));
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "temperature": sampling.temperature,
            "top_p": sampling.top_p,
            "top_k": sampling.top_k,
            "frequency_penalty": sampling.frequency_penalty,
        }))
        .send()
        .await
        .map_err(|e| format!("LM Studio недоступен ({}): {}", url, e))?;
    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LM Studio ошибка {}: {}", code, body.chars().take(300).collect::<String>()));
    }
    let mut full = String::new();
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("обрыв стрима LM: {}", e))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(d) = v["choices"][0]["delta"]["content"].as_str() {
                        full.push_str(d);
                        on_delta(d);
                    }
                }
            }
        }
    }
    Ok(full)
}

pub async fn chat_once(lm_url: Option<&str>, model: &str, messages: &Value, sampling: &Sampling) -> Result<String, String> {
    chat_stream(lm_url, model, messages, sampling, |_| {}).await
}

pub async fn embeddings(lm_url: Option<&str>, model: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let url = format!("{}/v1/embeddings", base_url(lm_url));
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&json!({ "model": model, "input": inputs }))
        .send()
        .await
        .map_err(|e| format!("embeddings недоступны: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("embeddings ошибка {}", resp.status()));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = vec![Vec::new(); inputs.len()];
    if let Some(arr) = v["data"].as_array() {
        for item in arr {
            let idx = item["index"].as_u64().unwrap_or(0) as usize;
            if let (Some(slot), Some(emb)) = (out.get_mut(idx), item["embedding"].as_array()) {
                *slot = emb.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            }
        }
    }
    Ok(out)
}
