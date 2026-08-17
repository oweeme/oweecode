// Remote file access over FTP or SFTP: connect, browse, read/write text files,
// byte-safe upload/download with progress events, delete/mkdir/rename. Both
// protocols are unified behind one `RemoteConn` enum so the rest of the app
// (and the frontend) doesn't need to care which one a given connection uses.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use tauri::Emitter;
use ssh2::Session;
use std::net::TcpStream;

use crate::util::format_epoch_secs;

#[derive(Serialize, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

struct SftpSession {
    session: Mutex<Session>,
    #[allow(dead_code)]
    root: String,
}

struct FtpSession {
    stream: Mutex<suppaftp::FtpStream>,
    #[allow(dead_code)]
    root: String,
}

enum RemoteConn {
    Sftp(SftpSession),
    Ftp(FtpSession),
}

static REMOTE: Lazy<Mutex<HashMap<String, Arc<RemoteConn>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Parses one line of a Unix-style FTP `LIST` response into a `RemoteEntry`.
fn parse_ftp_list(line: &str, parent: &str) -> Option<RemoteEntry> {
    let cols: Vec<&str> = line.split_ascii_whitespace().collect();
    if cols.len() < 9 { return None; }
    let is_dir = cols[0].starts_with('d');
    let size = cols[4].parse::<u64>().unwrap_or(0);
    let modified = cols[5..8].join(" ");
    let name = cols[8..].join(" ");
    if name == "." || name == ".." { return None; }
    let path = if parent == "/" { format!("/{}", name) }
               else { format!("{}/{}", parent.trim_end_matches('/'), name) };
    Some(RemoteEntry { name, path, is_dir, size, modified })
}

/// Opens an SFTP or FTP connection under `id`, replacing any previous one with the same id.
#[tauri::command]
pub async fn remote_connect(
    id: String, protocol: String, host: String, port: u16,
    user: String, pass: String, root: String,
) -> Result<(), String> {
    REMOTE.lock().unwrap().remove(&id);
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        let conn: RemoteConn = match protocol.as_str() {
            "sftp" => {
                let tcp = TcpStream::connect(format!("{}:{}", host, port))
                    .map_err(|e| format!("TCP: {}", e))?;
                let mut sess = Session::new().map_err(|e| e.to_string())?;
                sess.set_tcp_stream(tcp);
                sess.handshake().map_err(|e| format!("Handshake: {}", e))?;
                if sess.userauth_agent(&user).is_err() {
                    if !pass.is_empty() {
                        sess.userauth_password(&user, &pass)
                            .map_err(|e| format!("Auth: {}", e))?;
                    } else {
                        return Err("Auth failed: no password and no SSH agent key loaded".into());
                    }
                }
                if !sess.authenticated() { return Err("Authentication failed".into()); }
                // Many servers/firewalls drop an SFTP session after ~30-60s of no
                // wire traffic, which browsing around (no data exchange while the
                // user just looks at a listing) triggers easily — "Broken pipe"
                // came from exactly that. Ping periodically so the connection
                // survives idle browsing.
                sess.set_keepalive(false, 20);
                RemoteConn::Sftp(SftpSession { session: Mutex::new(sess), root })
            }
            "ftp" => {
                let mut stream = suppaftp::FtpStream::connect(format!("{}:{}", host, port))
                    .map_err(|e| format!("FTP connect: {}", e))?;
                stream.login(&user, &pass).map_err(|e| format!("FTP login: {}", e))?;
                RemoteConn::Ftp(FtpSession { stream: Mutex::new(stream), root })
            }
            p => return Err(format!("Unknown protocol: {}", p)),
        };
        REMOTE.lock().unwrap().insert(id, Arc::new(conn));
        Ok(())
    }).await.map_err(|e| e.to_string())??;
    spawn_remote_keepalive(id2);
    Ok(())
}

/// Runs for as long as `id` stays in REMOTE, pinging it every 20s so idle
/// browsing doesn't get treated as a dead connection by the server/firewall.
/// Self-terminates once the entry is gone (disconnected, or replaced by a
/// fresh reconnect under the same id).
fn spawn_remote_keepalive(id: String) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            let conn = { REMOTE.lock().unwrap().get(&id).cloned() };
            let Some(conn) = conn else { break };
            let alive = tokio::task::spawn_blocking(move || match &*conn {
                RemoteConn::Sftp(s) => s.session.lock().unwrap().keepalive_send().is_ok(),
                RemoteConn::Ftp(f) => f.stream.lock().unwrap().noop().is_ok(),
            }).await.unwrap_or(false);
            if !alive { break; }
        }
    });
}

#[tauri::command]
pub async fn remote_disconnect(id: String) -> Result<(), String> {
    REMOTE.lock().unwrap().remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn remote_list_dir(id: String, path: String) -> Result<Vec<RemoteEntry>, String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        let mut entries = match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                sftp.readdir(Path::new(&path)).map_err(|e| e.to_string())?
                    .into_iter().filter_map(|(p, stat)| {
                        let name = p.file_name()?.to_string_lossy().to_string();
                        if name == "." || name == ".." { return None; }
                        Some(RemoteEntry {
                            is_dir: stat.is_dir(), size: stat.size.unwrap_or(0),
                            modified: stat.mtime.map(format_epoch_secs).unwrap_or_default(),
                            path: p.to_string_lossy().to_string(), name,
                        })
                    }).collect::<Vec<_>>()
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.list(Some(&path)).map_err(|e| e.to_string())?
                   .iter().filter_map(|l| parse_ftp_list(l, &path)).collect()
            }
        };
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(entries)
    }).await.map_err(|e| e.to_string())?
}

/// Reads a remote file as UTF-8 text — for opening it in the editor. Corrupts
/// binary content by design (lossy UTF-8); use `remote_download_file` for real transfers.
#[tauri::command]
pub async fn remote_read_file(id: String, path: String) -> Result<String, String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp.open(Path::new(&path)).map_err(|e| e.to_string())?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&buf).to_string())
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                let cursor = ftp.retr_as_buffer(&path).map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(cursor.get_ref()).to_string())
            }
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_write_file(id: String, path: String, content: String) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp.create(Path::new(&path)).map_err(|e| e.to_string())?;
                file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
                Ok(())
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.put_file(&path, &mut std::io::Cursor::new(content.as_bytes()))
                   .map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }).await.map_err(|e| e.to_string())?
}

// Byte-safe transfer commands used for actual file upload/download (as opposed to
// remote_read_file/remote_write_file above, which are String-based and only meant
// for opening remote text files in the editor — they corrupt binary content).
//
// Both commands stream through a thin Read/Write wrapper that reports bytes
// transferred so far via a Tauri event, throttled to ~80ms so a fast localhost
// transfer doesn't flood the IPC bridge with thousands of events. Transfers in
// the frontend are strictly sequential (one at a time), so a single unscoped
// event name is enough — no per-transfer id needed.
#[derive(Serialize, Clone)]
struct TransferProgress { done: u64, total: u64 }

fn emit_transfer_progress(app: &tauri::AppHandle, done: u64, total: u64) {
    app.emit("ftp-transfer-progress", TransferProgress { done, total }).ok();
}

struct ProgressReader<R> {
    inner: R,
    done: u64,
    total: u64,
    app: tauri::AppHandle,
    last_emit: std::time::Instant,
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done += n as u64;
        if n == 0 || self.last_emit.elapsed().as_millis() >= 80 {
            emit_transfer_progress(&self.app, self.done, self.total);
            self.last_emit = std::time::Instant::now();
        }
        Ok(n)
    }
}

struct ProgressWriter<W> {
    inner: W,
    done: u64,
    total: u64,
    app: tauri::AppHandle,
    last_emit: std::time::Instant,
}

impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.done += n as u64;
        if self.last_emit.elapsed().as_millis() >= 80 {
            emit_transfer_progress(&self.app, self.done, self.total);
            self.last_emit = std::time::Instant::now();
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> { self.inner.flush() }
}

/// Downloads a remote file to disk, streaming bytes so large files don't need to
/// fit in memory, and emitting throttled `ftp-transfer-progress` events as it goes.
#[tauri::command]
pub async fn remote_download_file(id: String, remote_path: String, local_path: String, total_size: u64, app: tauri::AppHandle) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if let Some(parent) = Path::new(&local_path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let out_file = fs::File::create(&local_path).map_err(|e| e.to_string())?;
        let mut writer = ProgressWriter { inner: out_file, done: 0, total: total_size, app: app.clone(), last_emit: std::time::Instant::now() };
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp.open(Path::new(&remote_path)).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut writer).map_err(|e| e.to_string())?;
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.retr(&remote_path, |reader| {
                    std::io::copy(reader, &mut writer)
                        .map(|_| ())
                        .map_err(suppaftp::FtpError::ConnectionError)
                }).map_err(|e| e.to_string())?;
            }
        }
        emit_transfer_progress(&app, total_size, total_size);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// Uploads a local file to the remote, streaming bytes and emitting progress events.
#[tauri::command]
pub async fn remote_upload_file(id: String, local_path: String, remote_path: String, app: tauri::AppHandle) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    let total_size = fs::metadata(&local_path).map_err(|e| e.to_string())?.len();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let in_file = fs::File::open(&local_path).map_err(|e| e.to_string())?;
        let mut reader = ProgressReader { inner: in_file, done: 0, total: total_size, app: app.clone(), last_emit: std::time::Instant::now() };
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp.create(Path::new(&remote_path)).map_err(|e| e.to_string())?;
                std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.put_file(&remote_path, &mut reader).map_err(|e| e.to_string())?;
            }
        }
        emit_transfer_progress(&app, total_size, total_size);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_delete(id: String, path: String, is_dir: bool) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                if is_dir { sftp.rmdir(Path::new(&path)).map_err(|e| e.to_string()) }
                else { sftp.unlink(Path::new(&path)).map_err(|e| e.to_string()) }
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                if is_dir { ftp.rmdir(&path).map_err(|e| e.to_string()) }
                else { ftp.rm(&path).map_err(|e| e.to_string()) }
            }
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_mkdir(id: String, path: String) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                sftp.mkdir(Path::new(&path), 0o755).map_err(|e| e.to_string())
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.mkdir(&path).map_err(|e| e.to_string())
            }
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_rename(id: String, old_path: String, new_path: String) -> Result<(), String> {
    let conn = REMOTE.lock().unwrap().get(&id).cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                sftp.rename(Path::new(&old_path), Path::new(&new_path), None)
                    .map_err(|e| e.to_string())
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.rename(&old_path, &new_path).map_err(|e| e.to_string())
            }
        }
    }).await.map_err(|e| e.to_string())?
}
