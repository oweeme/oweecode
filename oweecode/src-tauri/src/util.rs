// Small helpers shared by more than one module — kept separate so pty.rs,
// lsp.rs, filesystem.rs and remote.rs don't need to depend on each other
// just to reach a one-off function.

/// Resolves the PATH a real interactive login shell would see. GUI-launched
/// apps only inherit a minimal PATH — missing whatever a shell profile adds
/// (nvm, npm global bin, homebrew, `go install`'s GOPATH/bin, cargo/rustup's
/// component dir, composer's global vendor/bin...). A binary installed via
/// any of those (typescript-language-server, gopls, phpactor, rust-analyzer,
/// pylsp, or a plain CLI like `claude`) can be on disk yet "not found" here
/// even though it works fine from an actual terminal. Used by both
/// `pty::pty_create` (launching CLI tools in a terminal tab) and
/// `lsp::lsp_start` (launching language servers).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn resolve_login_shell_path() -> Option<String> {
    use tokio::sync::OnceCell;
    static CACHE: OnceCell<Option<String>> = OnceCell::const_new();
    CACHE
        .get_or_init(|| async { compute_login_shell_path().await })
        .await
        .clone()
}

/// Asks the user's own shell ($SHELL: bash, zsh or fish) for its PATH, then
/// merges the usual install locations on top. The shell's output is wrapped in
/// markers because `.bashrc`/`.zshrc` may print banners to stdout, which would
/// otherwise corrupt the value. Even if the shell fails, the well-known
/// directories (cargo, go, npm, composer, pip, nvm...) still make tools resolve.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn compute_login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else {
                "/bin/bash".into()
            }
        });
    let is_fish = shell.ends_with("fish");
    let script = if is_fish {
        "echo -n __OWEE_PATH_START__; string join : $PATH; echo -n __OWEE_PATH_END__"
    } else {
        "printf '__OWEE_PATH_START__%s__OWEE_PATH_END__' \"$PATH\""
    };
    let from_shell = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(&shell)
            .args(["-l", "-i", "-c", script])
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .and_then(|out| {
        let a = out.find("__OWEE_PATH_START__")? + "__OWEE_PATH_START__".len();
        let b = out[a..].find("__OWEE_PATH_END__")? + a;
        Some(out[a..b].trim().to_string())
    })
    .filter(|p| !p.is_empty());

    let mut parts: Vec<String> = Vec::new();
    let mut push = |p: String| {
        if !p.is_empty() && !parts.contains(&p) {
            parts.push(p);
        }
    };
    if let Some(p) = from_shell {
        for d in p.split(':') {
            push(d.to_string());
        }
    }
    if let Ok(p) = std::env::var("PATH") {
        for d in p.split(':') {
            push(d.to_string());
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        for d in [
            ".local/bin",
            ".cargo/bin",
            "go/bin",
            ".npm-global/bin",
            ".config/composer/vendor/bin",
            ".composer/vendor/bin",
            ".bun/bin",
            ".volta/bin",
            ".local/share/pnpm",
        ] {
            push(format!("{home}/{d}"));
        }
        // Qt installer puts each kit in ~/Qt/<version>/<kit>/bin (qmake, qmlls, qt-cmake, windeployqt…)
        if let Ok(rd) = std::fs::read_dir(format!("{home}/Qt")) {
            let mut vers: Vec<_> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            vers.sort();
            vers.reverse(); // newest first
            for v in vers {
                if let Ok(kits) = std::fs::read_dir(&v) {
                    for k in kits.flatten() {
                        let b = k.path().join("bin");
                        if b.is_dir() {
                            push(b.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        // mise / asdf expose every managed runtime through shims, plus one bin dir per install
        push(format!("{home}/.local/share/mise/shims"));
        push(format!("{home}/.asdf/shims"));
        if let Ok(rd) = std::fs::read_dir(format!("{home}/.local/share/mise/installs")) {
            for tool in rd.flatten() {
                if let Ok(vers) = std::fs::read_dir(tool.path()) {
                    for v in vers.flatten() {
                        let b = v.path().join("bin");
                        if b.is_dir() {
                            push(b.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        // nvm / fnm keep one bin dir per installed Node version
        for base in [".nvm/versions/node", ".local/share/fnm/node-versions"] {
            if let Ok(rd) = std::fs::read_dir(format!("{home}/{base}")) {
                for e in rd.flatten() {
                    let p = e.path();
                    for sub in ["bin", "installation/bin"] {
                        let c = p.join(sub);
                        if c.is_dir() {
                            push(c.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    for d in [
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/local/go/bin",
        "/opt/homebrew/bin",
        "/snap/bin",
        "/usr/local/sbin",
        "/usr/sbin",
    ] {
        push(d.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(":"))
    }
}

/// Finds `binary` in the resolved PATH; returns its full path if present.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn find_binary(binary: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let path = resolve_login_shell_path().await?;
    path.split(':')
        .map(|d| std::path::Path::new(d).join(binary))
        .find(|p| {
            p.is_file()
                && std::fs::metadata(p)
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
        })
        .map(|p| p.to_string_lossy().to_string())
}

/// Formats a Unix epoch (in whole seconds) as a short local-time display string
/// (e.g. "28/07/26 01:22") shared by the local file list (`filesystem::list_dir`)
/// and the SFTP branch of the remote file list (`remote::remote_list_dir`) — FTP's
/// LIST output brings its own pre-formatted date text instead, since the server
/// already renders it and reparsing risks misreading truncated/ambiguous formats
/// (e.g. "Jan 15 2023" vs "Jan 15 10:30").
pub fn format_epoch_secs(secs: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%d/%m/%y %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// Plain base64 encoder (no external crate) — used to send binary image data
/// (`filesystem::read_image_base64`) to the frontend as a data URI-friendly string.
pub fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[(n >> 18) & 63] as char);
        result.push(CHARS[(n >> 12) & 63] as char);
        result.push(if chunk.len() > 1 {
            CHARS[(n >> 6) & 63] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            CHARS[n & 63] as char
        } else {
            '='
        });
    }
    result
}
