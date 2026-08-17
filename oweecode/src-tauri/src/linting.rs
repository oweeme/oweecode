// Lightweight syntax linting for languages without an installed LSP: shells
// out to each language's own toolchain (python3 -c compile, node --check,
// php -l, rustc --emit=metadata) rather than reimplementing a parser.
// Independent from — and simpler than — the full LSP bridge in lsp.rs.

use std::fs;
use std::io::Write;
use serde::Serialize;

#[derive(Serialize, Default, Clone)]
pub struct LintError {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub message: String,
    pub severity: String,
}

#[tauri::command]
pub async fn lint_file(language: String, content: String) -> Vec<LintError> {
    tokio::task::spawn_blocking(move || run_lint(&language, &content))
        .await
        .unwrap_or_default()
}

fn run_lint(language: &str, content: &str) -> Vec<LintError> {
    match language {
        "python"     => lint_python(content),
        "javascript" | "typescript" => lint_js(content, language),
        "php"        => lint_php(content),
        "rust"       => lint_rust(content),
        _            => vec![],
    }
}

fn tmp_path(ext: &str) -> std::path::PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("_owee_lint_{}.{}", ms, ext))
}

/// Feeds source to `python3 -c "compile(...)"` over stdin and parses the resulting SyntaxError.
fn lint_python(content: &str) -> Vec<LintError> {
    use std::process::{Command, Stdio};
    let script = concat!(
        "import ast,sys\n",
        "src=sys.stdin.buffer.read().decode('utf-8','replace')\n",
        "try:\n",
        "    compile(src,'<stdin>','exec')\n",
        "except SyntaxError as e:\n",
        "    print(f'ERR:{e.lineno or 1}:{e.offset or 0}:{e.msg}')\n",
        "    sys.exit(1)\n"
    );
    let mut child = match Command::new("python3")
        .args(["-c", script])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn() { Ok(c) => c, Err(_) => return vec![] };

    if let Some(mut i) = child.stdin.take() { let _ = i.write_all(content.as_bytes()); }
    let out = match child.wait_with_output() { Ok(o) => o, Err(_) => return vec![] };
    if out.status.success() { return vec![]; }

    String::from_utf8_lossy(&out.stdout).lines()
        .filter_map(|l| l.strip_prefix("ERR:"))
        .filter_map(|r| {
            let p: Vec<&str> = r.splitn(3, ':').collect();
            if p.len() == 3 {
                let ln = p[0].parse::<usize>().unwrap_or(1).saturating_sub(1);
                let col = p[1].parse::<usize>().unwrap_or(1).saturating_sub(1);
                Some(LintError { line: ln, col, end_line: ln, end_col: 1000,
                    message: p[2].to_string(), severity: "error".to_string() })
            } else { None }
        }).collect()
}

/// Writes source to a temp file and runs `node --check` on it (syntax-only, no execution).
fn lint_js(content: &str, language: &str) -> Vec<LintError> {
    let ext = if language == "typescript" { "ts" } else { "js" };
    let tmp = tmp_path(ext);
    if fs::write(&tmp, content).is_err() { return vec![]; }
    let out = std::process::Command::new("node").arg("--check").arg(&tmp).output();
    let _ = fs::remove_file(&tmp);
    let out = match out { Ok(o) if !o.status.success() => o, _ => return vec![] };
    let stderr = String::from_utf8_lossy(&out.stderr);
    parse_node_errors(&stderr)
}

fn parse_node_errors(stderr: &str) -> Vec<LintError> {
    let lines: Vec<&str> = stderr.lines().collect();
    let mut errors = vec![];
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("SyntaxError:") || t.starts_with("ReferenceError:") || t.starts_with("TypeError:") {
            let msg = t.to_string();
            // Scan backwards for location line "path:line" or "path:line:col"
            for prev in lines[..i].iter().rev() {
                if prev.starts_with('/') || prev.starts_with('.') {
                    let parts: Vec<&str> = prev.splitn(4, ':').collect();
                    if parts.len() >= 2 {
                        if let Ok(ln) = parts[1].trim().parse::<usize>() {
                            let col = parts.get(2).and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(1);
                            errors.push(LintError {
                                line: ln.saturating_sub(1), col: col.saturating_sub(1),
                                end_line: ln.saturating_sub(1), end_col: 1000,
                                message: msg.clone(), severity: "error".to_string(),
                            });
                            break;
                        }
                    }
                }
            }
        }
    }
    errors
}

/// Writes source to a temp file and runs `php -l` (lint-only, no execution) on it.
fn lint_php(content: &str) -> Vec<LintError> {
    let tmp = tmp_path("php");
    if fs::write(&tmp, content).is_err() { return vec![]; }
    let out = std::process::Command::new("php")
        .args(["-l", "-d", "display_errors=1", tmp.to_str().unwrap_or("")])
        .output();
    let _ = fs::remove_file(&tmp);
    let out = match out { Ok(o) => o, Err(_) => return vec![] };
    let combined = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    combined.lines()
        .filter(|l| (l.contains("Parse error") || l.contains("Fatal error") || l.contains("Warning:"))
            && l.contains(" on line "))
        .filter_map(|l| {
            let n = l.split(" on line ").last()?.trim().parse::<usize>().ok()?;
            let msg = l.split(" in ").next().unwrap_or(l).trim().to_string();
            let sev = if l.contains("Warning:") { "warning" } else { "error" };
            Some(LintError { line: n.saturating_sub(1), col: 0, end_line: n.saturating_sub(1),
                end_col: 1000, message: msg, severity: sev.to_string() })
        }).collect()
}

/// Writes source to a temp file and runs `rustc --emit=metadata` (type-check only,
/// no codegen/linking) on it, parsing rustc's own `--error-format=json` diagnostics.
fn lint_rust(content: &str) -> Vec<LintError> {
    let tmp = tmp_path("rs");
    if fs::write(&tmp, content).is_err() { return vec![]; }
    let out = std::process::Command::new("rustc")
        .args(["--edition", "2021", "--error-format=json", "--emit=metadata", "-o", "/dev/null"])
        .arg(&tmp).output();
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(std::env::temp_dir().join("librust_check.rmeta"));
    let out = match out { Ok(o) => o, Err(_) => return vec![] };
    String::from_utf8_lossy(&out.stderr).lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            let level = v["level"].as_str()?.to_string();
            if level != "error" && level != "warning" { return None; }
            let message = v["message"].as_str().unwrap_or("").to_string();
            let span = v["spans"].as_array()?.first()?;
            let ln = span["line_start"].as_u64().unwrap_or(1) as usize;
            let col = span["column_start"].as_u64().unwrap_or(1) as usize;
            let end_ln = span["line_end"].as_u64().unwrap_or(ln as u64) as usize;
            let end_col = span["column_end"].as_u64().unwrap_or(col as u64) as usize;
            Some(LintError { line: ln.saturating_sub(1), col: col.saturating_sub(1),
                end_line: end_ln.saturating_sub(1), end_col, message, severity: level })
        }).collect()
}
