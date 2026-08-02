// Language Server Protocol bridge: speaks raw JSON-RPC over stdio
// (`Content-Length: N\r\n\r\n{json}` framing) to a real language server
// binary (typescript-language-server, vue-language-server, gopls,
// rust-analyzer, phpactor, pylsp), one process per language, shared across
// every open file of that language. The frontend calls `lsp_start` once per
// language, then `lsp_complete`/`lsp_diagnostics`/`lsp_notify_open` per file.

use std::io::{BufRead as LspBufRead, BufReader as LspBufReader, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
use std::sync::mpsc;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{json, Value};

use crate::util::resolve_login_shell_path;

#[derive(Serialize, Clone, Debug, Default)]
pub struct LspCompletionItem {
    pub label: String,
    pub kind: Option<u32>,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    // Present when the server sent a `textEdit` (the common case for member
    // completions): the exact character range on the requested line to
    // replace, and the text to put there. This is NOT necessarily "insert at
    // the cursor" — e.g. for `api.|` the range often starts *before* the dot
    // and `insert_text` is ".get", replacing the dot itself. Applying
    // `insert_text` at the cursor position instead (ignoring this range)
    // produces a duplicated dot ("api..get") — exactly the bug this fixes.
    pub edit_start_col: Option<u32>,
    pub edit_end_col: Option<u32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct LspDiagnosticItem {
    pub message: String,
    pub severity: u32,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

struct LspSession {
    stdin: Mutex<std::process::ChildStdin>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    diagnostics: Mutex<HashMap<String, Vec<LspDiagnosticItem>>>,
    initialized: std::sync::atomic::AtomicBool,
    stderr_log: Mutex<Vec<String>>,
    // Latest document version WE sent per URI (via didOpen/didChange). A
    // publishDiagnostics notification carries the version the server was
    // looking at when it computed those diagnostics — if that's older than
    // what we've since sent, the diagnostics describe text that no longer
    // exists (e.g. a typo the user already fixed) and must be dropped,
    // not shown as if they still applied to the current buffer.
    doc_versions: Mutex<HashMap<String, u64>>,
}

/// Appends a line to a session's captured stderr, keeping only the last 30 —
/// enough to see a crash's error message without unbounded memory growth for
/// a long-lived server that logs routinely.
/// Returns the next document version to send for `uri` and records it as the
/// latest — single source of truth shared by every call site that mutates a
/// document (didChange in lsp_complete AND lsp_signature_help both used to
/// keep their own independent counters, which could send the two requests'
/// version numbers out of order for the same file; this fixes that too).
/// didOpen's hardcoded version 1 seeds the entry before this is ever called.
fn next_doc_version(session: &LspSession, uri: &str) -> u64 {
    let mut versions = session.doc_versions.lock().unwrap();
    let ver = versions.get(uri).copied().unwrap_or(1) + 1;
    versions.insert(uri.to_string(), ver);
    ver
}

fn push_stderr_line(log: &Mutex<Vec<String>>, line: String) {
    let mut log = log.lock().unwrap();
    log.push(line);
    let len = log.len();
    if len > 30 { log.drain(0..len - 30); }
}

static LSP_SESSIONS: Lazy<Mutex<HashMap<String, Arc<LspSession>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn lsp_binary(language: &str) -> Option<&'static str> {
    match language {
        "typescript" | "javascript" => Some("typescript-language-server"),
        "vue"    => Some("vue-language-server"),
        "go"     => Some("gopls"),
        "rust"   => Some("rust-analyzer"),
        "php"    => Some("phpactor"),
        "python" => Some("pylsp"),
        _ => None,
    }
}

fn lsp_args(language: &str) -> Vec<&'static str> {
    match language {
        "typescript" | "javascript" | "vue" => vec!["--stdio"],
        "php" => vec!["language-server"],
        _ => vec![],
    }
}

/// `@vue/language-server` (Volar) needs to be told where a real TypeScript
/// install's `tsserverlibrary.js` lives via `initializationOptions.typescript.tsdk`
/// — without it, it crashes on the `initialized` notification with
/// `TypeError: Cannot read properties of undefined (reading 'server')` instead
/// of erroring cleanly. Resolves it the same way `npm ls -g` would: ask the
/// login shell for its global node_modules root, then look for `typescript/lib`
/// under it.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn resolve_typescript_tsdk() -> Option<String> {
    let shell = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    let global_root = tokio::process::Command::new(shell)
        .args(["--login", "-i", "-c", "npm root -g"])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    let tsdk_dir = format!("{global_root}/typescript/lib");
    if std::path::Path::new(&format!("{tsdk_dir}/tsserverlibrary.js")).exists() {
        Some(tsdk_dir)
    } else {
        None
    }
}

fn lsp_lang_id(language: &str) -> &'static str {
    match language {
        "vue" => "vue",
        "typescript" => "typescript",
        "javascript" => "javascript",
        "go" => "go", "rust" => "rust",
        "php" => "php", "python" => "python",
        _ => "plaintext",
    }
}

fn lsp_write_msg(stdin: &mut std::process::ChildStdin, msg: &str) -> Result<(), String> {
    let frame = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
    stdin.write_all(frame.as_bytes()).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn lsp_read_msg(reader: &mut LspBufReader<std::process::ChildStdout>) -> Option<Value> {
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 { return None; }
        let line = line.trim_end_matches(|c| c == '\r' || c == '\n');
        if line.is_empty() { break; }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_len = rest.trim().parse().unwrap_or(0);
        }
    }
    if content_len == 0 { return None; }
    let mut body = vec![0u8; content_len];
    use std::io::Read as IoR;
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Background thread that reads the server's stdout for as long as the process
/// lives: responses get routed to whichever `lsp_complete`/`lsp_start` call is
/// waiting on that request id, and `publishDiagnostics` notifications are cached
/// per-file for `lsp_diagnostics` to read on demand.
fn lsp_start_reader(mut reader: LspBufReader<std::process::ChildStdout>, session: Arc<LspSession>, language: String) {
    std::thread::spawn(move || {
        while let Some(msg) = lsp_read_msg(&mut reader) {
            if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                let tx = session.pending.lock().unwrap().remove(&id);
                if let Some(tx) = tx { let _ = tx.send(msg); }
            } else if msg.get("method").and_then(|v| v.as_str()) == Some("textDocument/publishDiagnostics") {
                if let Some(params) = msg.get("params") {
                    let uri = params["uri"].as_str().unwrap_or("").to_string();

                    // Discard diagnostics computed against a document version we've
                    // since moved past (see doc_versions doc comment) — otherwise a
                    // slow analysis pass can surface errors about text the user
                    // already fixed/changed, pointing at whatever now happens to
                    // sit at the same range (e.g. "Could not find name 'aapi'" long
                    // after the typo was corrected to "api").
                    if let Some(msg_version) = params["version"].as_u64() {
                        let latest_sent = session.doc_versions.lock().unwrap().get(&uri).copied().unwrap_or(0);
                        if msg_version < latest_sent { continue; }
                    }

                    let diags: Vec<LspDiagnosticItem> = params["diagnostics"]
                        .as_array().unwrap_or(&vec![]).iter()
                        .filter_map(|d| {
                            let range = d.get("range")?;
                            Some(LspDiagnosticItem {
                                message:    d["message"].as_str().unwrap_or("").to_string(),
                                severity:   d["severity"].as_u64().unwrap_or(1) as u32,
                                start_line: range["start"]["line"].as_u64().unwrap_or(0) as u32,
                                start_col:  range["start"]["character"].as_u64().unwrap_or(0) as u32,
                                end_line:   range["end"]["line"].as_u64().unwrap_or(0) as u32,
                                end_col:    range["end"]["character"].as_u64().unwrap_or(0) as u32,
                            })
                        }).collect();
                    session.diagnostics.lock().unwrap().insert(uri, diags);
                }
            }
        }
        // stdout closed: the server process died. Evict it so the next
        // lsp_start actually respawns instead of reusing a dead pipe
        // forever (this was the bug behind persistent "Broken pipe" errors).
        let mut sessions = LSP_SESSIONS.lock().unwrap();
        if let Some(current) = sessions.get(&language) {
            if Arc::ptr_eq(current, &session) { sessions.remove(&language); }
        }
    });
}

/// Starts (or no-ops if already running) the language server for `language`,
/// rooted at `root_path`, and completes the `initialize`/`initialized` handshake.
#[tauri::command]
pub async fn lsp_start(language: String, root_path: String) -> Result<bool, String> {
    {
        let sessions = LSP_SESSIONS.lock().unwrap();
        if sessions.contains_key(&language) { return Ok(true); }
    }
    let binary = lsp_binary(&language).ok_or_else(|| format!("No LSP para {}", language))?;
    let args   = lsp_args(&language);

    // Same PATH gap as pty::pty_create's CLI spawn: a language server installed
    // via npm/go install/rustup/composer/pip lives in a directory this
    // GUI process never inherits on its own.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let resolved_path = resolve_login_shell_path().await;

    let mut cmd = std::process::Command::new(binary);
    cmd.args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = &resolved_path {
        cmd.env("PATH", p.trim());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let path_debug = match &resolved_path {
        Some(p) => format!("PATH resuelto ({} chars): {}", p.trim().len(), p.trim()),
        None => "no se pudo resolver PATH via login shell".to_string(),
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let path_debug = "N/A (no-unix)".to_string();

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{binary} no encontrado: {e}. [{path_debug}] Instálalo con el comando que aparece en Preferencias → LSP."))?;

    let stdin  = child.stdin.take().ok_or("no stdin")?;
    let stdout = LspBufReader::new(child.stdout.take().ok_or("no stdout")?);
    let stderr = child.stderr.take();

    let session = Arc::new(LspSession {
        stdin:        Mutex::new(stdin),
        next_id:      AtomicU64::new(1),
        pending:      Mutex::new(HashMap::new()),
        diagnostics:  Mutex::new(HashMap::new()),
        initialized:  std::sync::atomic::AtomicBool::new(false),
        stderr_log:   Mutex::new(Vec::new()),
        doc_versions: Mutex::new(HashMap::new()),
    });

    if let Some(stderr) = stderr {
        let log_session = session.clone();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match std::io::BufRead::read_line(&mut reader, &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => push_stderr_line(&log_session.stderr_log, line.trim_end().to_string()),
                }
            }
        });
    }

    lsp_start_reader(stdout, session.clone(), language.clone());

    // Send initialize
    let req_id   = session.next_id.fetch_add(1, AtomicOrd::SeqCst);
    let root_uri = format!("file://{}", root_path);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let tsdk = if language == "vue" { resolve_typescript_tsdk().await } else { None };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let tsdk: Option<String> = None;

    let mut init_params = json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "capabilities": {
            "textDocument": {
                "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["plaintext"] } },
                "publishDiagnostics": { "relatedInformation": false }
            }
        }
    });
    if let Some(tsdk) = tsdk {
        init_params["initializationOptions"] = json!({ "typescript": { "tsdk": tsdk } });
    }
    let init = json!({
        "jsonrpc": "2.0", "id": req_id, "method": "initialize",
        "params": init_params
    }).to_string();

    let (tx, rx) = mpsc::channel::<Value>();
    session.pending.lock().unwrap().insert(req_id, tx);
    { let mut stdin = session.stdin.lock().unwrap(); lsp_write_msg(&mut stdin, &init)?; }

    // Wait for initialize response (blocking, called in spawn_blocking context is fine)
    tokio::task::spawn_blocking(move || {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
    }).await.ok();

    // Send initialized notification
    let notif = json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }).to_string();
    { let mut stdin = session.stdin.lock().unwrap(); lsp_write_msg(&mut stdin, &notif)?; }
    session.initialized.store(true, AtomicOrd::SeqCst);

    LSP_SESSIONS.lock().unwrap().insert(language, session);
    std::mem::forget(child); // Keep process alive
    Ok(true)
}

/// Requests completions at (`line`, `col`) in `file_path`, first pushing the
/// current buffer via `textDocument/didChange` so the server sees in-progress edits.
#[tauri::command]
pub async fn lsp_complete(
    language: String,
    file_path: String,
    content: String,
    line: u32,
    col: u32,
) -> Result<Vec<LspCompletionItem>, String> {
    let session = LSP_SESSIONS.lock().unwrap().get(&language).cloned();
    let session = match session { Some(s) => s, None => return Ok(vec![]) };
    if !session.initialized.load(AtomicOrd::SeqCst) { return Ok(vec![]); }

    let uri     = format!("file://{}", file_path);
    let lang_id = lsp_lang_id(&language);

    // Sync document state. next_doc_version starts from 1 (didOpen's hardcoded
    // version) so the first didChange here is always >= 2 — the LSP spec
    // requires didChange versions to strictly increase per document, and
    // colliding with didOpen's version 1 made spec-compliant servers ignore
    // the change entirely, silently returning nothing for any position that
    // only existed in the edit.
    let ver = next_doc_version(&session, &uri);
    let did_change = json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": ver, "languageId": lang_id },
            "contentChanges": [{ "text": content }]
        }
    }).to_string();

    let req_id = session.next_id.fetch_add(1, AtomicOrd::SeqCst);
    let comp_req = json!({
        "jsonrpc": "2.0", "id": req_id, "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": col },
            "context": { "triggerKind": 1 }
        }
    }).to_string();

    let (tx, rx) = mpsc::channel::<Value>();
    session.pending.lock().unwrap().insert(req_id, tx);
    {
        let mut stdin = session.stdin.lock().unwrap();
        if let Err(e) = lsp_write_msg(&mut stdin, &did_change).and_then(|_| lsp_write_msg(&mut stdin, &comp_req)) {
            drop(stdin);
            return Err(lsp_dead_session_error(&language, &session, e));
        }
    }

    // The FIRST completion request against a real, multi-file project can take
    // 30+ seconds — the server is building its whole TypeScript program graph
    // on demand, not just answering a query. Measured directly against a real
    // project: initialize returns almost immediately, but the first
    // textDocument/completion response took ~30s. 6s was nowhere near enough
    // and silently returned "no results" while the server was still working.
    // Later requests reuse the now-built program and come back in well under
    // a second — this long timeout is a safety margin for the cold-start case,
    // not the typical wait.
    let response = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(45)).ok()
    }).await.ok().flatten();

    let Some(resp) = response else { return Ok(vec![]); };

    let items_arr = if let Some(arr) = resp["result"].as_array() {
        arr.to_vec()
    } else if let Some(arr) = resp["result"]["items"].as_array() {
        arr.to_vec()
    } else {
        return Ok(vec![]);
    };

    // Some servers (Volar's global-scope completions in particular) return
    // the FULL unfiltered symbol list — thousands of entries — leaving
    // prefix filtering to the client. We used to blindly take() the first
    // 60 of that raw, non-relevance-sorted list, which silently dropped
    // whatever the user was actually typing (e.g. "useRouter" never made
    // the top 60 out of 3000+ scope symbols). Filter by the in-progress
    // word before truncating.
    let prefix = current_word_prefix(&content, line, col);

    let filtered: Vec<&Value> = if prefix.is_empty() {
        items_arr.iter().collect()
    } else {
        let lower = prefix.to_lowercase();
        let matches: Vec<&Value> = items_arr.iter()
            .filter(|item| item["label"].as_str().is_some_and(|l| l.to_lowercase().starts_with(&lower)))
            .collect();
        if matches.is_empty() { items_arr.iter().collect() } else { matches }
    };

    Ok(filtered.iter().take(200).filter_map(|item| {
        let label = item["label"].as_str()?.to_string();
        // Prefer textEdit.newText/range when present — it's the server's exact
        // instruction for what to replace, which for member completions right
        // after a `.` often spans the dot itself (see edit_start_col doc comment
        // on LspCompletionItem). Plain insertText/label have no range attached,
        // so the frontend has to guess — that guess is what caused "api..get".
        let text_edit = &item["textEdit"];
        let (insert_text, edit_start_col, edit_end_col) = if let Some(new_text) = text_edit["newText"].as_str() {
            let start = text_edit["range"]["start"]["character"].as_u64().map(|c| c as u32);
            let end   = text_edit["range"]["end"]["character"].as_u64().map(|c| c as u32);
            (Some(new_text.to_string()), start, end)
        } else {
            (item["insertText"].as_str().map(|s| s.to_string()), None, None)
        };
        Some(LspCompletionItem {
            label,
            kind: item["kind"].as_u64().map(|k| k as u32),
            detail: item["detail"].as_str().map(|s| s.to_string()),
            insert_text,
            edit_start_col,
            edit_end_col,
        })
    }).collect())
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct LspSignatureParam {
    pub label: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct LspSignature {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<LspSignatureParam>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct LspSignatureHelp {
    pub signatures: Vec<LspSignature>,
    pub active_signature: u32,
    pub active_parameter: i32, // -1 when the server doesn't say
}

/// Requests parameter hints for the call at (`line`, `col`) — what a function/method
/// expects, which one is active, and which parameter the cursor is currently on.
/// Meant to be called as the user types `(` or `,` inside a call expression.
#[tauri::command]
pub async fn lsp_signature_help(
    language: String,
    file_path: String,
    content: String,
    line: u32,
    col: u32,
) -> Result<Option<LspSignatureHelp>, String> {
    let session = LSP_SESSIONS.lock().unwrap().get(&language).cloned();
    let session = match session { Some(s) => s, None => return Ok(None) };
    if !session.initialized.load(AtomicOrd::SeqCst) { return Ok(None); }

    let uri     = format!("file://{}", file_path);
    let lang_id = lsp_lang_id(&language);

    let ver = next_doc_version(&session, &uri);
    let did_change = json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": ver, "languageId": lang_id },
            "contentChanges": [{ "text": content }]
        }
    }).to_string();

    let req_id = session.next_id.fetch_add(1, AtomicOrd::SeqCst);
    let sig_req = json!({
        "jsonrpc": "2.0", "id": req_id, "method": "textDocument/signatureHelp",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": col }
        }
    }).to_string();

    let (tx, rx) = mpsc::channel::<Value>();
    session.pending.lock().unwrap().insert(req_id, tx);
    {
        let mut stdin = session.stdin.lock().unwrap();
        if let Err(e) = lsp_write_msg(&mut stdin, &did_change).and_then(|_| lsp_write_msg(&mut stdin, &sig_req)) {
            drop(stdin);
            return Err(lsp_dead_session_error(&language, &session, e));
        }
    }

    // Signature help only runs on an already-open, already-analyzed document
    // (the user is mid-call, so completion/diagnostics already warmed up the
    // project) — no need for the 45s cold-start margin lsp_complete needs.
    let response = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(10)).ok()
    }).await.ok().flatten();

    let Some(resp) = response else { return Ok(None); };
    let result = &resp["result"];
    if result.is_null() { return Ok(None); }

    let signatures: Vec<LspSignature> = result["signatures"].as_array().unwrap_or(&vec![]).iter().map(|s| {
        let doc = match &s["documentation"] {
            Value::String(text) => Some(text.clone()),
            Value::Object(o) => o.get("value").and_then(|v| v.as_str()).map(|s| s.to_string()),
            _ => None,
        };
        let parameters = s["parameters"].as_array().unwrap_or(&vec![]).iter().filter_map(|p| {
            // `label` is either a plain string, or a [start, end] pair of
            // offsets into the signature's own label string.
            match &p["label"] {
                Value::String(text) => Some(LspSignatureParam { label: text.clone() }),
                Value::Array(range) if range.len() == 2 => {
                    let sig_label = s["label"].as_str().unwrap_or("");
                    let start = range[0].as_u64()? as usize;
                    let end   = range[1].as_u64()? as usize;
                    let chars: Vec<char> = sig_label.chars().collect();
                    Some(LspSignatureParam { label: chars.get(start..end)?.iter().collect() })
                }
                _ => None,
            }
        }).collect();
        LspSignature {
            label: s["label"].as_str().unwrap_or("").to_string(),
            documentation: doc,
            parameters,
        }
    }).collect();

    if signatures.is_empty() { return Ok(None); }

    Ok(Some(LspSignatureHelp {
        signatures,
        active_signature: result["activeSignature"].as_u64().unwrap_or(0) as u32,
        active_parameter: result["activeParameter"].as_i64().unwrap_or(-1) as i32,
    }))
}

/// Finds the identifier prefix ending at (`line`, `col`) in `content` — e.g. for
/// "const router=use|" (cursor at `|`) returns "use". Used to filter completions
/// server-side before truncating to the top N.
fn current_word_prefix(content: &str, line: u32, col: u32) -> String {
    let Some(line_text) = content.lines().nth(line as usize) else { return String::new() };
    // Char-based (not byte-based) indexing: col comes from a UTF-16 offset
    // in the editor, and byte-slicing a str containing multi-byte chars
    // (e.g. accented text in comments/strings) could panic on non-boundary.
    let chars: Vec<char> = line_text.chars().collect();
    let col = (col as usize).min(chars.len());
    let mut start = col;
    while start > 0 && { let c = chars[start - 1]; c.is_alphanumeric() || c == '_' || c == '$' } {
        start -= 1;
    }
    chars[start..col].iter().collect()
}

/// Returns whatever diagnostics the server has last pushed for `file_path` (cached, not re-requested).
#[tauri::command]
pub async fn lsp_diagnostics(language: String, file_path: String) -> Result<Vec<LspDiagnosticItem>, String> {
    let session = LSP_SESSIONS.lock().unwrap().get(&language).cloned();
    let session = match session { Some(s) => s, None => return Ok(vec![]) };
    let uri = format!("file://{}", file_path);
    let result = session.diagnostics.lock().unwrap().get(&uri).cloned().unwrap_or_default();
    Ok(result)
}

/// Tells the server a file was opened, so it starts tracking it (required before completions
/// or diagnostics work for that file — see the LSP `textDocument/didOpen` notification).
#[tauri::command]
pub async fn lsp_notify_open(language: String, file_path: String, content: String) -> Result<(), String> {
    let session = LSP_SESSIONS.lock().unwrap().get(&language).cloned();
    let session = match session { Some(s) => s, None => return Ok(()) };
    if !session.initialized.load(AtomicOrd::SeqCst) { return Ok(()); }
    let uri     = format!("file://{}", file_path);
    let lang_id = lsp_lang_id(&language);
    let msg = json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri, "languageId": lang_id, "version": 1, "text": content } }
    }).to_string();
    let mut stdin = session.stdin.lock().unwrap();
    if let Err(e) = lsp_write_msg(&mut stdin, &msg) {
        drop(stdin);
        return Err(lsp_dead_session_error(&language, &session, e));
    }
    Ok(())
}

// A write failure (broken pipe) means the server process died. Evict it so
// the next lsp_start actually respawns instead of reusing the dead pipe
// forever, and surface the server's own stderr so the crash is diagnosable
// (previously stderr was discarded entirely, making crashes invisible).
fn lsp_dead_session_error(language: &str, session: &Arc<LspSession>, write_err: String) -> String {
    let mut sessions = LSP_SESSIONS.lock().unwrap();
    if let Some(current) = sessions.get(language) {
        if Arc::ptr_eq(current, session) { sessions.remove(language); }
    }
    drop(sessions);
    let stderr_tail = session.stderr_log.lock().unwrap().join(" | ");
    if stderr_tail.is_empty() {
        format!("El language server dejó de responder: {write_err}. Vuelve a abrir el archivo para reiniciarlo.")
    } else {
        format!("El language server dejó de responder: {write_err}. Últimas líneas: {stderr_tail}. Vuelve a abrir el archivo para reiniciarlo.")
    }
}

/// Shuts down the server for `language` cleanly (LSP `shutdown`/`exit`) and forgets its session.
#[tauri::command]
pub async fn lsp_stop(language: String) -> Result<(), String> {
    let session = LSP_SESSIONS.lock().unwrap().remove(&language);
    if let Some(s) = session {
        if let Ok(mut stdin) = s.stdin.lock() {
            let shutdown = json!({ "jsonrpc": "2.0", "id": 999999, "method": "shutdown", "params": null }).to_string();
            let _ = lsp_write_msg(&mut stdin, &shutdown);
            let exit = json!({ "jsonrpc": "2.0", "method": "exit", "params": null }).to_string();
            let _ = lsp_write_msg(&mut stdin, &exit);
        }
    }
    Ok(())
}
