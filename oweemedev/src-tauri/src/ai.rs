// AI chat backend: one `call_ai` command that speaks whichever provider's API
// shape the frontend asks for, plus small helpers to list locally-available
// Ollama models and OpenRouter's current free-tier catalog.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Debug)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

/// Sends a chat completion request to the given provider and returns the assistant's
/// reply text. Each provider branch adapts to that provider's own request/response shape
/// (Anthropic's `content` blocks, OpenAI-compatible `choices`, Gemini's `candidates`, etc.)
#[tauri::command]
pub async fn call_ai(
    provider: String,
    api_key: String,
    model: String,
    system: String,
    messages: Vec<AiMessage>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    match provider.as_str() {
        "claude" => {
            let msgs: Vec<Value> = messages.iter().map(|m| json!({
                "role": m.role,
                "content": m.content
            })).collect();

            let body = json!({
                "model": model,
                "max_tokens": 4096,
                "system": system,
                "messages": msgs
            });

            let res = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("API error").to_string());
            }

            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["content"][0]["text"].as_str().unwrap_or("").to_string())
        }
        "openai" => {
            let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }

            let body = json!({
                "model": model,
                "messages": msgs,
                "max_tokens": 4096
            });

            let res = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("API error").to_string());
            }

            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
        }
        "gemini" => {
            // system_instruction not supported in all versions — prepend as user/model pair
            let mut msgs: Vec<Value> = vec![];
            if !system.is_empty() {
                msgs.push(json!({ "role": "user",  "parts": [{ "text": system }] }));
                msgs.push(json!({ "role": "model", "parts": [{ "text": "Understood. I will follow those instructions." }] }));
            }
            for m in &messages {
                let role = if m.role == "assistant" { "model" } else { "user" };
                msgs.push(json!({ "role": role, "parts": [{ "text": m.content }] }));
            }
            let body = json!({
                "contents": msgs,
                "generationConfig": { "maxOutputTokens": 4096 }
            });
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, api_key
            );
            let res = client.post(&url)
                .header("content-type", "application/json")
                .json(&body)
                .send().await.map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("Gemini API error").to_string());
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string())
        }
        "ollama" => {
            // Ollama runs locally at http://localhost:11434
            let mut msgs: Vec<Value> = vec![json!({"role":"system","content":system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }
            let body = json!({ "model": model, "messages": msgs, "stream": false });
            let res = client.post("http://localhost:11434/api/chat")
                .header("content-type", "application/json")
                .json(&body)
                .send().await
                .map_err(|e| format!("Ollama not running? Start with: ollama serve\n{}", e))?;
            if !res.status().is_success() {
                let txt = res.text().await.unwrap_or_default();
                return Err(format!("Ollama error: {}", txt));
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["message"]["content"].as_str().unwrap_or("").to_string())
        }
        "deepseek" => {
            // DeepSeek uses OpenAI-compatible API
            let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }
            let body = json!({
                "model": model,
                "messages": msgs,
                "max_tokens": 4096
            });
            let res = client
                .post("https://api.deepseek.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("content-type", "application/json")
                .json(&body)
                .send().await
                .map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("DeepSeek error").to_string());
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
        }
        "groq" => {
            // Groq — OpenAI-compatible API, free tier with generous rate limits.
            let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }
            let body = json!({
                "model": model,
                "messages": msgs,
                "max_tokens": 4096
            });
            let res = client
                .post("https://api.groq.com/openai/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("content-type", "application/json")
                .json(&body)
                .send().await
                .map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("Groq error").to_string());
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
        }
        "openrouter" => {
            // OpenRouter — OpenAI-compatible API aggregating many providers, including
            // several genuinely free models (id suffix ":free").
            let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }
            let body = json!({
                "model": model,
                "messages": msgs,
                "max_tokens": 4096
            });
            let res = client
                .post("https://openrouter.ai/api/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("content-type", "application/json")
                .json(&body)
                .send().await
                .map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("OpenRouter error").to_string());
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
        }
        "omniroute" => {
            // OmniRoute — local OpenAI-compatible router/proxy (npm install -g omniroute &&
            // omniroute), fans out across 268+ providers with automatic failover, several
            // permanently free. Runs on the user's own machine, default port 20128.
            let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
            for m in &messages {
                msgs.push(json!({"role": m.role, "content": m.content}));
            }
            let body = json!({
                "model": model,
                "messages": msgs,
                "max_tokens": 4096
            });
            let res = client
                .post("http://localhost:20128/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("content-type", "application/json")
                .json(&body)
                .send().await
                .map_err(|e| format!("No se pudo conectar con OmniRoute en localhost:20128 — ¿está corriendo? (`omniroute` en una terminal). {}", e))?;
            if !res.status().is_success() {
                let err: Value = res.json().await.unwrap_or(json!({"error":{"message":"Unknown error"}}));
                return Err(err["error"]["message"].as_str().unwrap_or("OmniRoute error").to_string());
            }
            let data: Value = res.json().await.map_err(|e| e.to_string())?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
        }
        _ => Err(format!("Unknown provider: {}", provider))
    }
}

/// Lists model names currently pulled in a local Ollama install.
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|_| "Ollama no está corriendo. Inicia con: ollama serve".to_string())?;
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let models = data["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(models)
}

#[derive(Serialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
}

/// OpenRouter's free-tier catalog (the ":free" model slugs) changes over time —
/// entries get discontinued or lose their free variant — so a hardcoded list
/// goes stale. Ask OpenRouter's own public model list instead and filter for
/// whatever is actually free right now.
#[tauri::command]
pub async fn list_openrouter_free_models() -> Result<Vec<OpenRouterModel>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("OpenRouter respondió con estado {}", res.status()));
    }
    let data: Value = res.json().await.map_err(|e| e.to_string())?;
    let mut models: Vec<OpenRouterModel> = data["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?.to_string();
            if !id.ends_with(":free") {
                return None;
            }
            let prompt_free = m["pricing"]["prompt"].as_str() == Some("0");
            let completion_free = m["pricing"]["completion"].as_str() == Some("0");
            if !prompt_free || !completion_free {
                return None;
            }
            let name = m["name"].as_str().unwrap_or(&id).to_string();
            Some(OpenRouterModel { id, name })
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}
