// Local filesystem operations for the file explorer and editor: listing,
// reading/writing, creating, renaming, deleting (to trash or permanently),
// plus the project-wide text search used by the "search in files" panel.

use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::util::{base64_encode, format_epoch_secs};

#[derive(Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

/// Lists the immediate children of `path`, directories first then alphabetical.
#[tauri::command]
pub fn list_dir(path: String, show_hidden: Option<bool>) -> Result<Vec<FileEntry>, String> {
    let show_hidden = show_hidden.unwrap_or(false);
    let dir = fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut entries: Vec<FileEntry> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let path = e.path().to_string_lossy().to_string();
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta.as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_epoch_secs(d.as_secs()))
                .unwrap_or_default();
            Some(FileEntry { name, path, is_dir, size, modified })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_file(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_file(path: String) -> Result<(), String> {
    if Path::new(&path).exists() {
        return Err(format!("File already exists: {}", path));
    }
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, "").map_err(|e| e.to_string())
}

/// Writes a file, creating parent directories as needed and overwriting any
/// existing content — used to (re)generate config files like a container
/// stack's nginx.conf without the caller having to check existence first.
#[tauri::command]
pub fn write_config_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_dir_cmd(path: String) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

/// Moves a file/dir to the freedesktop.org trash (`~/.local/share/Trash`) instead
/// of deleting it outright, writing the required `.trashinfo` sidecar so desktop
/// file managers show it correctly and can restore it.
#[tauri::command]
pub fn move_to_trash(path: String) -> Result<(), String> {
    let src = Path::new(&path);
    if !src.exists() {
        return Err(format!("Path not found: {}", path));
    }

    // Build trash dirs: ~/.local/share/Trash/{files,info}
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let trash_files = format!("{}/.local/share/Trash/files", home);
    let trash_info  = format!("{}/.local/share/Trash/info",  home);
    fs::create_dir_all(&trash_files).map_err(|e| e.to_string())?;
    fs::create_dir_all(&trash_info).map_err(|e| e.to_string())?;

    let name = src.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Avoid collisions: append number if name already exists in trash
    let mut dest_name = name.clone();
    let mut counter = 1u32;
    while Path::new(&format!("{}/{}", trash_files, dest_name)).exists() {
        let stem = Path::new(&name).file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        let ext = Path::new(&name).extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        dest_name = format!("{}_{}{}", stem, counter, ext);
        counter += 1;
    }

    let dest = format!("{}/{}", trash_files, dest_name);
    let info = format!("{}/{}.trashinfo", trash_info, dest_name);

    // Write .trashinfo (freedesktop spec)
    let abs_path = src.canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(path.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let date = format_trash_date(now);
    let info_content = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", abs_path, date);
    fs::write(&info, info_content).map_err(|e| e.to_string())?;

    // Move to trash
    if src.is_dir() {
        // Try rename first (same filesystem), fallback to copy+delete
        if fs::rename(&path, &dest).is_err() {
            copy_dir_recursive(&path, &dest)?;
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
    } else {
        if fs::rename(&path, &dest).is_err() {
            fs::copy(&path, &dest).map_err(|e| e.to_string())?;
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Renders a Unix epoch as the ISO-ish timestamp format freedesktop's `.trashinfo` expects.
/// Hand-rolled (no chrono) since it only needs to run once per delete and avoids pulling
/// in a full calendar dependency just for this one call site.
fn format_trash_date(secs: u64) -> String {
    // Format: 2024-01-15T14:30:00
    let s = secs;
    let secs_per_day = 86400u64;
    let days = s / secs_per_day;
    let time_s = s % secs_per_day;
    // Simple calculation (approximate, ignores leap years perfectly)
    let mut year = 1970u64;
    let mut remaining_days = days;
    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 366 } else { 365 };
        if remaining_days < days_in_year { break; }
        remaining_days -= days_in_year;
        year += 1;
    }
    let months = [31u64,28,31,30,31,30,31,31,30,31,30,31];
    let mut month = 0usize;
    let mut day = remaining_days;
    for (i, &m) in months.iter().enumerate() {
        if day < m { month = i; break; }
        day -= m;
    }
    let h = time_s / 3600;
    let m = (time_s % 3600) / 60;
    let s2 = time_s % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", year, month+1, day+1, h, m, s2)
}

fn copy_dir_recursive(src: &str, dst: &str) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let src_path = format!("{}/{}", src, name);
        let dst_path = format!("{}/{}", dst, name);
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Permanently deletes a file/dir (bypasses trash). Refuses non-empty directories
/// unless `force` is set, so the UI can confirm before a recursive delete.
#[tauri::command]
pub fn delete_entry(path: String, force: bool) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() {
        let count = fs::read_dir(&path).map(|d| d.count()).unwrap_or(0);
        if count > 0 && !force {
            return Err(format!("NONEMPTY:{}", count));
        }
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

/// Reads an image file and returns it base64-encoded, for inline `<img>` preview in the UI.
#[tauri::command]
pub fn read_image_base64(path: String) -> Result<String, String> {
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(base64_encode(&bytes))
}

#[derive(Serialize)]
pub struct SearchHit {
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// Recursively greps text files under `root_path` for `query` (plain substring match;
/// `use_regex` is accepted for API compatibility but not implemented yet).
#[tauri::command]
pub fn search_in_files(
    root_path: String,
    query: String,
    case_sensitive: bool,
    use_regex: bool,
    max_results: usize,
) -> Result<Vec<SearchHit>, String> {
    let mut hits = Vec::new();
    let skip_dirs = ["node_modules", ".git", "target", "dist", ".cache", "__pycache__"];
    let text_exts = ["js","ts","tsx","jsx","vue","php","go","py","rs","html","htm","css","scss",
                     "sass","json","yaml","yml","toml","xml","md","txt","sql","sh","bash","env",
                     "c","cpp","h","hpp","java","rb","swift","kt","cs"];

    fn walk(
        dir: &str, query: &str, case_sensitive: bool, skip_dirs: &[&str],
        text_exts: &[&str], hits: &mut Vec<SearchHit>, max: usize,
    ) {
        if hits.len() >= max { return; }
        let entries = match fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
        for entry in entries.flatten() {
            if hits.len() >= max { return; }
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();
            if path.is_dir() {
                if name.starts_with('.') { continue; } // skip .git, .node_modules, etc.
                if skip_dirs.contains(&name.as_str()) { continue; }
                walk(&path_str, query, case_sensitive, skip_dirs, text_exts, hits, max);
            } else {
                let ext = path.extension().map(|e| e.to_str().unwrap_or("")).unwrap_or("");
                if !text_exts.contains(&ext) { continue; }
                if let Ok(content) = fs::read_to_string(&path_str) {
                    for (i, line) in content.lines().enumerate() {
                        if hits.len() >= max { return; }
                        let matched = if case_sensitive {
                            line.contains(query)
                        } else {
                            line.to_lowercase().contains(&query.to_lowercase())
                        };
                        if matched {
                            hits.push(SearchHit {
                                file: path_str.clone(),
                                line: i + 1,
                                text: line.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Ignore use_regex for now (simple contains search)
    let _ = use_regex;
    walk(&root_path, &query, case_sensitive, &skip_dirs, &text_exts, &mut hits, max_results);
    Ok(hits)
}

#[tauri::command]
pub fn open_in_file_manager(path: String) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/".to_string())
}

#[tauri::command]
pub fn path_join(base: String, segment: String) -> String {
    Path::new(&base).join(&segment).to_string_lossy().to_string()
}
