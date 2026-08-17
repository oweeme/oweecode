// Local AI agent: confines the existing filesystem/git commands to the open
// project's root, adds a one-shot (non-interactive) shell runner, and talks
// to Ollama's tool-calling chat API plus its model-pull endpoint. This module
// doesn't reimplement filesystem access — it validates the path and then
// delegates to `filesystem::*`, so the agent gets the exact same behavior the
// file explorer already has, just scoped to the project.

use std::path::{Path, PathBuf};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Emitter;

use crate::ai::AiMessage;
use crate::filesystem::{self, FileEntry, SearchHit};

/// Resolves `target` against `root` and rejects anything that escapes it
/// (absolute paths outside root, `..` traversal). `target` doesn't need to
/// exist yet — only the longest existing ancestor is canonicalized, then the
/// remaining (not-yet-created) components are appended back on, so writing a
/// brand-new file still gets checked correctly.
fn confine(root: &str, target: &str) -> Result<PathBuf, String> {
    let root_canon = Path::new(root)
        .canonicalize()
        .map_err(|e| format!("Root de proyecto inválido: {}", e))?;

    if target.trim().is_empty() || target == "." {
        return Ok(root_canon);
    }

    let target_path = Path::new(target);
    let joined = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        root_canon.join(target_path)
    };

    let mut existing = joined.clone();
    let mut remainder: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().map(|n| n.to_os_string());
        let parent = existing.parent().map(|p| p.to_path_buf());
        match (name, parent) {
            (Some(n), Some(p)) => { remainder.push(n); existing = p; }
            _ => break,
        }
    }
    let existing_canon = existing
        .canonicalize()
        .map_err(|_| format!("Ruta inválida: {}", target))?;

    let mut full = existing_canon;
    for part in remainder.into_iter().rev() {
        full.push(part);
    }

    if !full.starts_with(&root_canon) {
        return Err(format!("Ruta fuera del proyecto abierto: {}", target));
    }
    Ok(full)
}

// A model this small guesses filenames wrong often enough that it matters —
// on a miss, list what's actually in that directory instead of returning a
// bare IO error, so the next turn has something concrete to correct itself
// with instead of repeating the same wrong guess.
#[tauri::command]
pub fn agent_read_file(root: String, path: String) -> Result<String, String> {
    let p = confine(&root, &path)?;
    filesystem::open_file(p.to_string_lossy().to_string()).map_err(|e| {
        let parent = p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| p.clone());
        match filesystem::list_dir(parent.to_string_lossy().to_string(), Some(false)) {
            Ok(entries) if !entries.is_empty() => {
                let names: Vec<&str> = entries.iter().map(|f| f.name.as_str()).collect();
                format!("{} — Archivos reales en esa carpeta: {}", e, names.join(", "))
            }
            _ => e,
        }
    })
}

#[tauri::command]
pub fn agent_write_file(root: String, path: String, content: String) -> Result<(), String> {
    let p = confine(&root, &path)?;
    filesystem::write_config_file(p.to_string_lossy().to_string(), content)
}

#[tauri::command]
pub fn agent_list_dir(root: String, path: String, show_hidden: Option<bool>) -> Result<Vec<FileEntry>, String> {
    let p = confine(&root, &path)?;
    filesystem::list_dir(p.to_string_lossy().to_string(), show_hidden)
}

// fs_write_file/agent_write_file already creates parent directories as a
// side effect (write_config_file does), so this is only needed for an empty
// directory with no file in it yet — but the model reliably expects a
// mkdir-shaped tool to exist alongside read/write/list, so it gets one
// instead of guessing a name that silently fails as "unknown tool".
#[tauri::command]
pub fn agent_create_dir(root: String, path: String) -> Result<(), String> {
    let p = confine(&root, &path)?;
    filesystem::create_dir_cmd(p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn agent_search(
    root: String,
    query: String,
    case_sensitive: bool,
    use_regex: bool,
    max_results: usize,
) -> Result<Vec<SearchHit>, String> {
    // Always scoped to the whole project root — the agent doesn't get a
    // narrower/wider search target than what the user already has open.
    filesystem::search_in_files(root, query, case_sensitive, use_regex, max_results)
}

#[derive(Serialize)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

const SHELL_COMMAND_TIMEOUT_SECS: u64 = 120;

#[tauri::command]
pub async fn agent_run_command(root: String, cwd: Option<String>, command: String) -> Result<ShellResult, String> {
    let cwd_path = confine(&root, cwd.as_deref().unwrap_or("."))?;

    #[cfg(target_os = "windows")]
    let (shell, shell_arg): (&str, &str) = ("cmd", "/C");
    #[cfg(not(target_os = "windows"))]
    let (shell, shell_arg): (&str, &str) = ("/bin/sh", "-c");

    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg(shell_arg).arg(&command).current_dir(&cwd_path);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = crate::util::resolve_login_shell_path().await {
        cmd.env("PATH", p.trim());
    }

    let timeout = std::time::Duration::from_secs(SHELL_COMMAND_TIMEOUT_SECS);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) => Ok(ShellResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            timed_out: false,
        }),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Ok(ShellResult {
            stdout: String::new(),
            stderr: format!("Comando cancelado: superó el límite de {}s", SHELL_COMMAND_TIMEOUT_SECS),
            exit_code: -1,
            timed_out: true,
        }),
    }
}

#[derive(Serialize)]
pub struct OllamaStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
}

#[tauri::command]
pub async fn ollama_check_installed() -> OllamaStatus {
    let mut cmd = tokio::process::Command::new("ollama");
    cmd.arg("--version");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = crate::util::resolve_login_shell_path().await {
        cmd.env("PATH", p.trim());
    }
    let version = cmd.output().await.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let installed = version.is_some();

    let running = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(2)).build() {
        Ok(client) => client.get("http://localhost:11434/api/tags").send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false),
        Err(_) => false,
    };

    OllamaStatus { installed, running, version }
}

/// Structured tool-call the agent loop can act on, normalized from Ollama's
/// `message.tool_calls[].function` shape.
#[derive(Serialize, Debug, Clone)]
pub struct AgentToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Serialize, Debug)]
pub struct AgentChatResponse {
    pub content: String,
    pub tool_calls: Vec<AgentToolCall>,
}

/// One turn of the agent's tool-calling chat. Separate from `ai::call_ai`
/// because it needs a `tools` request field and structured `tool_calls` back
/// — none of the other 7 providers speak that shape, so folding this in would
/// mean touching every other branch for nothing.
#[tauri::command]
pub async fn agent_ollama_chat(
    model: String,
    system: String,
    messages: Vec<AiMessage>,
    tools: Value,
) -> Result<AgentChatResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system})];
    for m in &messages {
        msgs.push(json!({"role": m.role, "content": m.content}));
    }

    let body = json!({
        "model": model,
        "messages": msgs,
        "tools": tools,
        "stream": false,
    });

    let res = client.post("http://localhost:11434/api/chat")
        .header("content-type", "application/json")
        .json(&body)
        .send().await
        .map_err(|e| format!("Ollama no está corriendo. Iniciá el servicio e intentá de nuevo.\n{}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let txt = res.text().await.unwrap_or_default();
        if status.as_u16() == 401 || txt.contains("Unauthorized") {
            return Err(format!(
                "\"{}\" es un modelo cloud y tu sesión de ollama.com no está activa (o venció). Iniciá sesión de nuevo desde el botón ⚙ → \"Iniciar sesión en Ollama.com\", o elegí un modelo local mientras tanto.\n{}",
                model, txt
            ));
        }
        return Err(format!("Ollama error: {}", txt));
    }

    let data: Value = res.json().await.map_err(|e| e.to_string())?;
    let content = data["message"]["content"].as_str().unwrap_or("").to_string();

    let tool_calls = data["message"]["tool_calls"]
        .as_array()
        .map(|arr| {
            arr.iter().enumerate().filter_map(|(i, tc)| {
                let name = tc["function"]["name"].as_str()?.to_string();
                let arguments = tc["function"]["arguments"].clone();
                Some(AgentToolCall { id: format!("call_{}", i), name, arguments })
            }).collect()
        })
        .unwrap_or_default();

    Ok(AgentChatResponse { content, tool_calls })
}

#[derive(Serialize, Clone)]
struct PullProgress {
    model: String,
    status: String,
    completed: u64,
    total: u64,
}

fn emit_pull_progress(app: &tauri::AppHandle, p: PullProgress) {
    app.emit("ollama-pull-progress", p).ok();
}

/// Streams `ollama pull`'s NDJSON progress to the frontend as throttled
/// `ollama-pull-progress` events, same throttling approach as
/// `remote::emit_transfer_progress` (~150ms) so a fast local pull doesn't
/// flood the IPC bridge.
#[tauri::command]
pub async fn ollama_pull_model(model: String, app: tauri::AppHandle) -> Result<(), String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.post("http://localhost:11434/api/pull")
        .json(&json!({"name": model, "stream": true}))
        .send().await
        .map_err(|e| format!("No se pudo conectar con Ollama: {}", e))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("Error al descargar {}: {}", model, txt));
    }

    let mut stream = res.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim();
            if line.is_empty() { continue; }
            let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
            if let Some(err) = v["error"].as_str() {
                return Err(err.to_string());
            }
            let status = v["status"].as_str().unwrap_or("").to_string();
            let completed = v["completed"].as_u64().unwrap_or(0);
            let total = v["total"].as_u64().unwrap_or(0);
            let is_final = status.starts_with("success") || (total > 0 && completed >= total);
            if is_final || last_emit.elapsed().as_millis() >= 150 {
                emit_pull_progress(&app, PullProgress { model: model.clone(), status, completed, total });
                last_emit = std::time::Instant::now();
            }
        }
    }
    emit_pull_progress(&app, PullProgress { model: model.clone(), status: "success".into(), completed: 0, total: 0 });
    Ok(())
}
