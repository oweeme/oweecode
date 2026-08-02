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
    let shell = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    // Needs `-i` (interactive), not just `--login`: Debian/Ubuntu's default
    // ~/.bashrc starts with a `[ -z "$PS1" ] && return` (or `case $- in
    // *i*)...`) guard that no-ops the rest of the file — including whatever
    // appended PATH exports — for any non-interactive invocation. `-i` sets
    // bash's interactive flag even without a real tty, satisfying that guard.
    tokio::process::Command::new(shell)
        .args(["--login", "-i", "-c", "echo -n \"$PATH\""])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|p| !p.trim().is_empty())
}

/// Formats a Unix epoch (in whole seconds) as a short local-time display string
/// (e.g. "28/07/26 01:22") shared by the local file list (`filesystem::list_dir`)
/// and the SFTP branch of the remote file list (`remote::remote_list_dir`) — FTP's
/// LIST output brings its own pre-formatted date text instead, since the server
/// already renders it and reparsing risks misreading truncated/ambiguous formats
/// (e.g. "Jan 15 2023" vs "Jan 15 10:30").
pub fn format_epoch_secs(secs: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%d/%m/%y %H:%M").to_string())
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
        result.push(if chunk.len() > 1 { CHARS[(n >> 6) & 63] as char } else { '=' });
        result.push(if chunk.len() > 2 { CHARS[n & 63] as char } else { '=' });
    }
    result
}
