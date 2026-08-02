// Git integration: shells out to the system `git` binary so the user's
// existing credentials (SSH agent, credential helper, GitHub CLI auth, etc.)
// just work — no token management needed inside the IDE.

use std::fs;
use std::path::Path;
use serde::Serialize;

fn git_args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

async fn run_git(cwd: &str, args: Vec<String>) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar git: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else if !stdout.is_empty() { stdout } else { format!("git {} falló", args.join(" ")) };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Serialize, Clone)]
pub struct GitFileEntry {
    pub path: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflicted: bool,
    pub status_code: String,
}

#[derive(Serialize)]
pub struct GitStatusResult {
    pub is_repo: bool,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: i32,
    pub behind: i32,
    pub files: Vec<GitFileEntry>,
}

fn push_status_entry(files: &mut Vec<GitFileEntry>, path: &str, xy: &str) {
    let chars: Vec<char> = xy.chars().collect();
    let x = chars.first().copied().unwrap_or('.');
    let y = chars.get(1).copied().unwrap_or('.');
    files.push(GitFileEntry {
        path: path.to_string(),
        staged: x != '.',
        unstaged: y != '.',
        untracked: false,
        conflicted: false,
        status_code: xy.to_string(),
    });
}

/// Parses `git status --porcelain=v2 --branch` into branch/ahead/behind + per-file status.
#[tauri::command]
pub async fn git_status(path: String) -> Result<GitStatusResult, String> {
    let check = tokio::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&path)
        .output().await;
    let is_repo = matches!(&check, Ok(o) if o.status.success());
    if !is_repo {
        return Ok(GitStatusResult { is_repo: false, branch: String::new(), upstream: None, ahead: 0, behind: 0, files: vec![] });
    }

    let out = run_git(&path, git_args(&["status", "--porcelain=v2", "--branch"])).await?;
    let mut branch = String::new();
    let mut upstream = None;
    let mut ahead = 0i32;
    let mut behind = 0i32;
    let mut files = Vec::new();

    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            upstream = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') { ahead = n.parse().unwrap_or(0); }
                else if let Some(n) = part.strip_prefix('-') { behind = n.parse().unwrap_or(0); }
            }
        } else if let Some(rest) = line.strip_prefix("1 ") {
            let parts: Vec<&str> = rest.splitn(8, ' ').collect();
            if parts.len() == 8 { push_status_entry(&mut files, parts[7], parts[0]); }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            let parts: Vec<&str> = rest.splitn(9, ' ').collect();
            if parts.len() == 9 {
                let newpath = parts[8].split('\t').next().unwrap_or(parts[8]);
                push_status_entry(&mut files, newpath, parts[0]);
            }
        } else if let Some(rest) = line.strip_prefix("u ") {
            let parts: Vec<&str> = rest.splitn(10, ' ').collect();
            if parts.len() == 10 {
                files.push(GitFileEntry {
                    path: parts[9].to_string(), staged: false, unstaged: false,
                    untracked: false, conflicted: true, status_code: parts[0].to_string(),
                });
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            files.push(GitFileEntry {
                path: path.to_string(), staged: false, unstaged: false,
                untracked: true, conflicted: false, status_code: "??".to_string(),
            });
        }
    }

    Ok(GitStatusResult { is_repo: true, branch, upstream, ahead, behind, files })
}

#[tauri::command]
pub async fn git_diff(path: String, file: String, staged: bool) -> Result<String, String> {
    let mut args = git_args(&["diff", "--no-color"]);
    if staged { args.push("--cached".to_string()); }
    args.push("--".to_string());
    args.push(file);
    run_git(&path, args).await
}

#[tauri::command]
pub async fn git_stage(path: String, files: Vec<String>) -> Result<(), String> {
    let mut args = git_args(&["add", "--"]);
    args.extend(files);
    run_git(&path, args).await.map(|_| ())
}

#[tauri::command]
pub async fn git_unstage(path: String, files: Vec<String>) -> Result<(), String> {
    let mut args = git_args(&["restore", "--staged", "--"]);
    args.extend(files);
    run_git(&path, args).await.map(|_| ())
}

#[tauri::command]
pub async fn git_discard(path: String, files: Vec<String>) -> Result<(), String> {
    let mut args = git_args(&["checkout", "--"]);
    args.extend(files);
    run_git(&path, args).await.map(|_| ())
}

#[tauri::command]
pub async fn git_clean_untracked(path: String, files: Vec<String>) -> Result<(), String> {
    for f in &files {
        let full = Path::new(&path).join(f);
        if full.is_dir() { let _ = fs::remove_dir_all(&full); }
        else { let _ = fs::remove_file(&full); }
    }
    Ok(())
}

#[tauri::command]
pub async fn git_commit(path: String, message: String) -> Result<String, String> {
    run_git(&path, git_args(&["commit", "-m", &message])).await
}

#[tauri::command]
pub async fn git_push(path: String) -> Result<String, String> {
    run_git(&path, git_args(&["push"])).await
}

#[tauri::command]
pub async fn git_pull(path: String) -> Result<String, String> {
    run_git(&path, git_args(&["pull"])).await
}

#[tauri::command]
pub async fn git_fetch(path: String) -> Result<String, String> {
    run_git(&path, git_args(&["fetch"])).await
}

#[derive(Serialize)]
pub struct GitBranch { pub name: String, pub current: bool }

#[tauri::command]
pub async fn git_branches(path: String) -> Result<Vec<GitBranch>, String> {
    let out = run_git(&path, git_args(&["branch", "--list"])).await?;
    let mut branches = Vec::new();
    for line in out.lines() {
        let current = line.starts_with('*');
        let name = line.trim_start_matches('*').trim().to_string();
        if name.is_empty() || name.starts_with('(') { continue; }
        branches.push(GitBranch { name, current });
    }
    Ok(branches)
}

#[tauri::command]
pub async fn git_checkout_branch(path: String, branch: String) -> Result<(), String> {
    run_git(&path, git_args(&["checkout", &branch])).await.map(|_| ())
}

#[tauri::command]
pub async fn git_create_branch(path: String, name: String) -> Result<(), String> {
    run_git(&path, git_args(&["checkout", "-b", &name])).await.map(|_| ())
}

#[derive(Serialize)]
pub struct GitCommitInfo { pub hash: String, pub author: String, pub date: String, pub message: String }

#[tauri::command]
pub async fn git_log(path: String, limit: u32) -> Result<Vec<GitCommitInfo>, String> {
    let out = run_git(&path, git_args(&[
        "log", &format!("--max-count={}", limit), "--pretty=format:%H%x1f%an%x1f%ar%x1f%s",
    ])).await?;
    let mut commits = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        if parts.len() == 4 {
            commits.push(GitCommitInfo {
                hash: parts[0].to_string(), author: parts[1].to_string(),
                date: parts[2].to_string(), message: parts[3].to_string(),
            });
        }
    }
    Ok(commits)
}

#[tauri::command]
pub async fn git_remote_url(path: String) -> Result<Option<String>, String> {
    match run_git(&path, git_args(&["remote", "get-url", "origin"])).await {
        Ok(url) => Ok(Some(url.trim().to_string())),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn git_init(path: String) -> Result<(), String> {
    run_git(&path, git_args(&["init"])).await.map(|_| ())
}

#[tauri::command]
pub async fn git_config_get(path: String, key: String) -> Result<Option<String>, String> {
    match run_git(&path, git_args(&["config", "--get", &key])).await {
        Ok(v) => {
            let v = v.trim().to_string();
            Ok(if v.is_empty() { None } else { Some(v) })
        }
        Err(_) => Ok(None), // `git config --get` exits non-zero when the key isn't set
    }
}

#[tauri::command]
pub async fn git_config_set(path: String, key: String, value: String, global: bool) -> Result<(), String> {
    let mut args = vec!["config".to_string()];
    if global { args.push("--global".to_string()); }
    args.push(key);
    args.push(value);
    run_git(&path, args).await.map(|_| ())
}
