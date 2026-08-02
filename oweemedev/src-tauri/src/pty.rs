// Terminal tabs: one real OS pseudo-terminal per tab, streamed to the
// frontend as raw bytes over a Tauri event. Used both for a plain shell and
// for launching a specific CLI tool (Claude Code, an AI CLI, etc.) directly.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tauri::Emitter;

use crate::util::resolve_login_shell_path;

struct PtySession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

static PTY_SESSIONS: Lazy<Mutex<HashMap<String, PtySession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Opens a new PTY under `id`, spawning either a login shell or (if `command`
/// is given) a specific CLI directly, and starts a background thread that
/// forwards its output to the frontend as `pty:{id}` events until it exits.
#[tauri::command]
pub async fn pty_create(
    id: String,
    cwd: String,
    cols: u16,
    rows: u16,
    command: Option<String>,
    extra_env: Option<Vec<(String, String)>>,
    extra_args: Option<Vec<String>>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let size = PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
    let pair = pty_system.openpty(size).map_err(|e| e.to_string())?;

    let cmd = match command.as_deref() {
        Some(cli) => {
            // Run a specific CLI (claude, gemini, etc.) directly.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let resolved_path = resolve_login_shell_path().await;

            let mut c = CommandBuilder::new(cli);
            if let Some(args) = &extra_args {
                c.args(args);
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if let Some(p) = resolved_path {
                c.env("PATH", p.trim());
            }
            if let Some(envs) = &extra_env {
                for (k, v) in envs {
                    c.env(k, v);
                }
            }
            c.cwd(&cwd);
            c.env("TERM", "xterm-256color");
            c.env("COLORTERM", "truecolor");
            c
        }
        None => {
            // Use the appropriate shell for each platform
            #[cfg(target_os = "windows")]
            let (shell, args): (&str, &[&str]) = {
                // Prefer PowerShell if available, fall back to cmd.exe
                if std::path::Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
                    ("powershell.exe", &["-NoLogo"])
                } else {
                    ("cmd.exe", &[])
                }
            };
            #[cfg(target_os = "macos")]
            let (shell, args): (&str, &[&str]) = ("/bin/zsh", &["--login", "-i"]);
            #[cfg(target_os = "linux")]
            let (shell, args): (&str, &[&str]) = ("/bin/bash", &["--login", "-i"]);

            let mut c = CommandBuilder::new(shell);
            if !args.is_empty() { c.args(args); }
            c.cwd(&cwd);
            c.env("TERM", "xterm-256color");
            c.env("COLORTERM", "truecolor");
            c
        }
    };

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(writer));
    let master = Arc::new(Mutex::new(pair.master));

    PTY_SESSIONS.lock().unwrap().insert(id.clone(), PtySession { writer: Arc::clone(&writer), master: Arc::clone(&master) });

    let app = app_handle.clone();
    let event_id = format!("pty:{}", id);
    let exit_id  = format!("pty-exit:{}", id);
    let id_clone = id.clone();

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    app.emit(&event_id, chunk).ok();
                }
            }
        }
        PTY_SESSIONS.lock().unwrap().remove(&id_clone);
        app.emit(&exit_id, "").ok();
    });

    Ok(())
}

#[tauri::command]
pub fn pty_write(id: String, data: String) -> Result<(), String> {
    let sessions = PTY_SESSIONS.lock().unwrap();
    if let Some(s) = sessions.get(&id) {
        let mut w = s.writer.lock().unwrap();
        w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        w.flush().ok();
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(id: String, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = PTY_SESSIONS.lock().unwrap();
    if let Some(s) = sessions.get(&id) {
        s.master.lock().unwrap()
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_kill(id: String) {
    PTY_SESSIONS.lock().unwrap().remove(&id);
}
