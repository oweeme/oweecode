// Remote file access over FTP or SFTP: connect, browse, read/write text files,
// byte-safe upload/download with progress events, delete/mkdir/rename. Both
// protocols are unified behind one `RemoteConn` enum so the rest of the app
// (and the frontend) doesn't need to care which one a given connection uses.

use once_cell::sync::Lazy;
use serde::Serialize;
use ssh2::Session;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

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

/// Plain FTP or explicit FTPS (TLS) stream behind one interface, so every
/// command below works the same for both.
enum FtpAny {
    Plain(suppaftp::FtpStream),
    Tls(suppaftp::NativeTlsFtpStream),
}

macro_rules! ftp_dispatch {
    ($self:expr, $s:ident => $body:expr) => {
        match $self {
            FtpAny::Plain($s) => $body,
            FtpAny::Tls($s) => $body,
        }
    };
}

impl FtpAny {
    /// FTP starts in ASCII mode, where servers rewrite line endings: any binary file (images, zips, PDFs…) arrives corrupted.
    fn binary(&mut self) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.transfer_type(suppaftp::types::FileType::Binary))
    }
    fn noop(&mut self) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.noop())
    }
    fn list(&mut self, p: Option<&str>) -> suppaftp::FtpResult<Vec<String>> {
        ftp_dispatch!(self, s => s.list(p))
    }
    fn retr_as_buffer(&mut self, p: &str) -> suppaftp::FtpResult<std::io::Cursor<Vec<u8>>> {
        ftp_dispatch!(self, s => s.retr_as_buffer(p))
    }
    fn put_file<R: Read>(&mut self, p: &str, r: &mut R) -> suppaftp::FtpResult<u64> {
        match self {
            FtpAny::Plain(s) => s.put_file(p, r),
            FtpAny::Tls(s) => {
                // With TLS 1.3 the server sends session tickets right after the handshake. If they are still unread when the
                // data socket is closed, the kernel answers with a reset and servers drop data they had not read yet: files
                // arrived cut short (112 KB of 200 KB). Reading them first (a short, harmless read) avoids the reset.
                let mut ds = s.put_with_stream(p)?;
                macro_rules! drain_tickets {
                    ($ds:expr) => {{
                        let _ = $ds
                            .get_ref()
                            .set_read_timeout(Some(std::time::Duration::from_millis(40)));
                        let mut one = [0u8; 1];
                        let _ = $ds.read(&mut one);
                        let _ = $ds.get_ref().set_read_timeout(None);
                    }};
                }
                drain_tickets!(ds);
                let n = std::io::copy(r, &mut ds).map_err(suppaftp::FtpError::ConnectionError)?;
                let _ = std::io::Write::flush(&mut ds);
                drain_tickets!(ds);
                s.finalize_put_stream(ds)?;
                Ok(n)
            }
        }
    }
    fn retr<F, D>(&mut self, p: &str, f: F) -> suppaftp::FtpResult<D>
    where
        F: FnMut(&mut dyn Read) -> suppaftp::FtpResult<D>,
    {
        ftp_dispatch!(self, s => s.retr(p, f))
    }
    fn rmdir(&mut self, p: &str) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.rmdir(p))
    }
    fn rm(&mut self, p: &str) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.rm(p))
    }
    fn mkdir(&mut self, p: &str) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.mkdir(p))
    }
    fn rename(&mut self, a: &str, b: &str) -> suppaftp::FtpResult<()> {
        ftp_dispatch!(self, s => s.rename(a, b))
    }
}

struct FtpSession {
    stream: Mutex<FtpAny>,
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
    if cols.len() < 9 {
        return None;
    }
    let is_dir = cols[0].starts_with('d');
    let size = cols[4].parse::<u64>().unwrap_or(0);
    let modified = cols[5..8].join(" ");
    let name = cols[8..].join(" ");
    if name == "." || name == ".." {
        return None;
    }
    let path = if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    };
    Some(RemoteEntry {
        name,
        path,
        is_dir,
        size,
        modified,
    })
}

/// Opens a throwaway explicit-FTPS handshake (AUTH TLS) accepting any certificate, only to read the
/// SHA-256 fingerprint of the one the server presents ("AA:BB:…"). Nothing is sent but AUTH TLS.
fn probe_ftps_fingerprint(host: &str, port: u16) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::{BufRead, BufReader};
    use std::net::ToSocketAddrs;
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("No se pudo resolver el servidor")?;
    let tcp = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(10))
        .map_err(|e| format!("TCP: {e}"))?;
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();
    let mut reader = BufReader::new(tcp.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    loop {
        // welcome banner (may be multi-line: "220-…" … "220 …")
        line.clear();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("El servidor cerró la conexión".into());
        }
        if line.len() >= 4 && line.as_bytes()[3] == b' ' {
            break;
        }
    }
    (&tcp)
        .write_all(b"AUTH TLS\r\n")
        .map_err(|e| e.to_string())?;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("Sin respuesta a AUTH TLS".into());
        }
        if line.len() >= 4 && line.as_bytes()[3] == b' ' {
            break;
        }
    }
    if !line.starts_with("234") {
        return Err(format!("El servidor no ofrece FTPS: {}", line.trim()));
    }
    let conn = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| e.to_string())?;
    let stream = conn.connect(host, tcp).map_err(|e| format!("TLS: {e}"))?;
    let der = stream
        .peer_certificate()
        .map_err(|e| e.to_string())?
        .ok_or("El servidor no presentó certificado")?
        .to_der()
        .map_err(|e| e.to_string())?;
    Ok(Sha256::digest(&der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}

/// Checks the server's host key against ~/.ssh/known_hosts (like OpenSSH).
/// Unknown host  -> Err("HOSTKEY_UNKNOWN|<type>|<fingerprint>") unless `accept` is set,
///                  in which case it is remembered for next time.
/// Changed key   -> Err("HOSTKEY_MISMATCH|...") always: possible man-in-the-middle.
fn verify_host_key(sess: &Session, host: &str, port: u16, accept: bool) -> Result<(), String> {
    use ssh2::{CheckResult, HashType, KnownHostFileKind, KnownHostKeyFormat};
    let (key, key_type) = sess.host_key().ok_or("El servidor no envió su clave")?;
    let fingerprint = sess
        .host_key_hash(HashType::Sha256)
        .map(|h| {
            format!(
                "SHA256:{}",
                crate::util::base64_encode(h).trim_end_matches('=')
            )
        })
        .unwrap_or_else(|| "desconocida".to_string());
    let kind = format!("{:?}", key_type);

    let mut kh = sess.known_hosts().map_err(|e| e.to_string())?;
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("No se encontró HOME")?;
    let ssh_dir = Path::new(&home).join(".ssh");
    let file = ssh_dir.join("known_hosts");
    if file.exists() {
        let _ = kh.read_file(&file, KnownHostFileKind::OpenSSH);
    }

    match kh.check_port(host, port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err(format!(
            "HOSTKEY_MISMATCH|{kind}|{fingerprint}|¡La clave del servidor CAMBIÓ respecto a la guardada en ~/.ssh/known_hosts! Puede ser un ataque (man-in-the-middle). Conexión bloqueada.")),
        CheckResult::NotFound | CheckResult::Failure => {
            if !accept { return Err(format!("HOSTKEY_UNKNOWN|{kind}|{fingerprint}")); }
            if matches!(key_type, ssh2::HostKeyType::Unknown) { return Err("Tipo de clave de servidor no soportado".into()); }
            let fmt: KnownHostKeyFormat = key_type.into();
            let name = if port == 22 { host.to_string() } else { format!("[{host}]:{port}") };
            kh.add(&name, key, "", fmt).map_err(|e| e.to_string())?;
            let _ = fs::create_dir_all(&ssh_dir);
            kh.write_file(&file, KnownHostFileKind::OpenSSH).map_err(|e| format!("No se pudo guardar known_hosts: {e}"))?;
            Ok(())
        }
    }
}

/// Tries the usual private keys in ~/.ssh (the password field doubles as the
/// key passphrase). Returns true once one of them authenticates.
fn try_default_keys(sess: &Session, user: &str, passphrase: &str) -> bool {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return false;
    };
    let dir = std::path::Path::new(&home).join(".ssh");
    let pp = if passphrase.is_empty() {
        None
    } else {
        Some(passphrase)
    };
    for name in ["id_ed25519", "id_rsa", "id_ecdsa"] {
        let key = dir.join(name);
        if key.is_file()
            && sess.userauth_pubkey_file(user, None, &key, pp).is_ok()
            && sess.authenticated()
        {
            return true;
        }
    }
    false
}

/// Opens an SFTP or FTP connection under `id`, replacing any previous one with the same id.
#[tauri::command]
pub async fn remote_connect(
    id: String,
    protocol: String,
    host: String,
    port: u16,
    user: String,
    pass: String,
    root: String,
    accept_host: Option<bool>,
    trust_cert: Option<String>,
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
                verify_host_key(&sess, &host, port, accept_host.unwrap_or(false))?;
                if sess.userauth_agent(&user).is_err() && !try_default_keys(&sess, &user, &pass) {
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
            "ftp" | "ftps" => {
                let addr = format!("{}:{}", host, port);
                let stream = if protocol == "ftps" {
                    let plain = suppaftp::NativeTlsFtpStream::connect(&addr)
                        .map_err(|e| format!("FTP connect: {}", e))?;
                    // A certificate the user already trusted is pinned by fingerprint: the server must still present exactly that one.
                    let pinned = match &trust_cert {
                        Some(fp) => {
                            let actual = probe_ftps_fingerprint(&host, port)?;
                            if !actual.eq_ignore_ascii_case(fp) {
                                return Err(format!("CERT_MISMATCH|{actual}|El certificado del servidor cambió desde la última vez que confiaste en él."));
                            }
                            true
                        }
                        None => false,
                    };
                    let tls = if pinned {
                        native_tls::TlsConnector::builder().danger_accept_invalid_certs(true).danger_accept_invalid_hostnames(true).build()
                    } else { native_tls::TlsConnector::new() }.map_err(|e| format!("TLS: {}", e))?;
                    let mut secure = match plain.into_secure(suppaftp::NativeTlsConnector::from(tls), &host) {
                        Ok(s) => s,
                        Err(suppaftp::FtpError::SecureError(why)) => {
                            // typically a self-signed / expired / wrong-name certificate: show its fingerprint so the user can decide
                            let fp = probe_ftps_fingerprint(&host, port).unwrap_or_else(|_| "desconocida".into());
                            return Err(format!("CERT_UNTRUSTED|{fp}|{why}"));
                        }
                        Err(e) => return Err(format!("FTPS (TLS): {}", e)),
                    };
                    secure.login(&user, &pass).map_err(|e| format!("FTP login: {}", e))?;
                    let mut any = FtpAny::Tls(secure);
                    any.binary().map_err(|e| format!("FTP TYPE I: {}", e))?;
                    any
                } else {
                    let mut plain = suppaftp::FtpStream::connect(&addr)
                        .map_err(|e| format!("FTP connect: {}", e))?;
                    plain.login(&user, &pass).map_err(|e| format!("FTP login: {}", e))?;
                    let mut any = FtpAny::Plain(plain);
                    any.binary().map_err(|e| format!("FTP TYPE I: {}", e))?;
                    any
                };
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
            })
            .await
            .unwrap_or(false);
            if !alive {
                break;
            }
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
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || {
        let mut entries = match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                sftp.readdir(Path::new(&path))
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter_map(|(p, stat)| {
                        let name = p.file_name()?.to_string_lossy().to_string();
                        if name == "." || name == ".." {
                            return None;
                        }
                        Some(RemoteEntry {
                            is_dir: stat.is_dir(),
                            size: stat.size.unwrap_or(0),
                            modified: stat.mtime.map(format_epoch_secs).unwrap_or_default(),
                            path: p.to_string_lossy().to_string(),
                            name,
                        })
                    })
                    .collect::<Vec<_>>()
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.list(Some(&path))
                    .map_err(|e| e.to_string())?
                    .iter()
                    .filter_map(|l| parse_ftp_list(l, &path))
                    .collect()
            }
        };
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads a remote file as UTF-8 text — for opening it in the editor. Corrupts
/// binary content by design (lossy UTF-8); use `remote_download_file` for real transfers.
#[tauri::command]
pub async fn remote_read_file(id: String, path: String) -> Result<String, String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || match &*conn {
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
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_write_file(id: String, path: String, content: String) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || match &*conn {
        RemoteConn::Sftp(s) => {
            let sess = s.session.lock().unwrap();
            let sftp = sess.sftp().map_err(|e| e.to_string())?;
            let mut file = sftp.create(Path::new(&path)).map_err(|e| e.to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        RemoteConn::Ftp(f) => {
            let mut ftp = f.stream.lock().unwrap();
            ftp.put_file(&path, &mut std::io::Cursor::new(content.as_bytes()))
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    })
    .await
    .map_err(|e| e.to_string())?
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
struct TransferProgress {
    done: u64,
    total: u64,
}

fn emit_transfer_progress<RT: tauri::Runtime>(app: &tauri::AppHandle<RT>, done: u64, total: u64) {
    app.emit("ftp-transfer-progress", TransferProgress { done, total })
        .ok();
}

struct ProgressReader<R, RT: tauri::Runtime> {
    inner: R,
    done: u64,
    total: u64,
    app: tauri::AppHandle<RT>,
    last_emit: std::time::Instant,
}

impl<R: Read, RT: tauri::Runtime> Read for ProgressReader<R, RT> {
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

struct ProgressWriter<W, RT: tauri::Runtime> {
    inner: W,
    done: u64,
    total: u64,
    app: tauri::AppHandle<RT>,
    last_emit: std::time::Instant,
}

impl<W: Write, RT: tauri::Runtime> Write for ProgressWriter<W, RT> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.done += n as u64;
        if self.last_emit.elapsed().as_millis() >= 80 {
            emit_transfer_progress(&self.app, self.done, self.total);
            self.last_emit = std::time::Instant::now();
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Downloads a remote file to disk, streaming bytes so large files don't need to
/// fit in memory, and emitting throttled `ftp-transfer-progress` events as it goes.
#[tauri::command]
pub async fn remote_download_file<RT: tauri::Runtime>(
    id: String,
    remote_path: String,
    local_path: String,
    total_size: u64,
    app: tauri::AppHandle<RT>,
) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if let Some(parent) = Path::new(&local_path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let out_file = fs::File::create(&local_path).map_err(|e| e.to_string())?;
        let mut writer = ProgressWriter {
            inner: out_file,
            done: 0,
            total: total_size,
            app: app.clone(),
            last_emit: std::time::Instant::now(),
        };
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp
                    .open(Path::new(&remote_path))
                    .map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut writer).map_err(|e| e.to_string())?;
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.retr(&remote_path, |reader| {
                    std::io::copy(reader, &mut writer)
                        .map(|_| ())
                        .map_err(suppaftp::FtpError::ConnectionError)
                })
                .map_err(|e| e.to_string())?;
            }
        }
        emit_transfer_progress(&app, total_size, total_size);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Uploads a local file to the remote, streaming bytes and emitting progress events.
#[tauri::command]
pub async fn remote_upload_file<RT: tauri::Runtime>(
    id: String,
    local_path: String,
    remote_path: String,
    app: tauri::AppHandle<RT>,
) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    let total_size = fs::metadata(&local_path).map_err(|e| e.to_string())?.len();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let in_file = fs::File::open(&local_path).map_err(|e| e.to_string())?;
        let mut reader = ProgressReader {
            inner: in_file,
            done: 0,
            total: total_size,
            app: app.clone(),
            last_emit: std::time::Instant::now(),
        };
        match &*conn {
            RemoteConn::Sftp(s) => {
                let sess = s.session.lock().unwrap();
                let sftp = sess.sftp().map_err(|e| e.to_string())?;
                let mut file = sftp
                    .create(Path::new(&remote_path))
                    .map_err(|e| e.to_string())?;
                std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
            }
            RemoteConn::Ftp(f) => {
                let mut ftp = f.stream.lock().unwrap();
                ftp.put_file(&remote_path, &mut reader)
                    .map_err(|e| e.to_string())?;
            }
        }
        emit_transfer_progress(&app, total_size, total_size);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_delete(id: String, path: String, is_dir: bool) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || match &*conn {
        RemoteConn::Sftp(s) => {
            let sess = s.session.lock().unwrap();
            let sftp = sess.sftp().map_err(|e| e.to_string())?;
            if is_dir {
                sftp.rmdir(Path::new(&path)).map_err(|e| e.to_string())
            } else {
                sftp.unlink(Path::new(&path)).map_err(|e| e.to_string())
            }
        }
        RemoteConn::Ftp(f) => {
            let mut ftp = f.stream.lock().unwrap();
            if is_dir {
                ftp.rmdir(&path).map_err(|e| e.to_string())
            } else {
                ftp.rm(&path).map_err(|e| e.to_string())
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_mkdir(id: String, path: String) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || match &*conn {
        RemoteConn::Sftp(s) => {
            let sess = s.session.lock().unwrap();
            let sftp = sess.sftp().map_err(|e| e.to_string())?;
            sftp.mkdir(Path::new(&path), 0o755)
                .map_err(|e| e.to_string())
        }
        RemoteConn::Ftp(f) => {
            let mut ftp = f.stream.lock().unwrap();
            ftp.mkdir(&path).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remote_rename(id: String, old_path: String, new_path: String) -> Result<(), String> {
    let conn = REMOTE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| "Not connected".to_string())?;
    tokio::task::spawn_blocking(move || match &*conn {
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
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod hostkey_tests {
    //! Needs an SSH server: set OWEE_SSH_TEST_PY to a python with `paramiko` and the test starts a throwaway one
    //! (no auth accepted — enough to prove the host-key check runs right after the handshake).
    use std::process::{Child, Command, Stdio};

    const SERVER: &str = r#"
import socket, sys, threading, paramiko
port, keyfile = int(sys.argv[1]), sys.argv[2]
try: key = paramiko.ECDSAKey.from_private_key_file(keyfile)
except Exception:
    key = paramiko.ECDSAKey.generate(); key.write_private_key_file(keyfile)
class Srv(paramiko.ServerInterface):
    def check_auth_password(self, u, p): return paramiko.AUTH_FAILED
    def get_allowed_auths(self, u): return "password"
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind(("127.0.0.1", port)); s.listen(5)
def handle(c):
    t = paramiko.Transport(c); t.add_server_key(key)
    try: t.start_server(server=Srv()); t.join(3)
    except Exception: pass
while True:
    c, _ = s.accept(); threading.Thread(target=handle, args=(c,), daemon=True).start()
"#;

    struct Guard(Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn start(py: &str, port: u16, keyfile: &std::path::Path, script: &std::path::Path) -> Guard {
        let c = Command::new(py)
            .arg(script)
            .arg(port.to_string())
            .arg(keyfile)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("python");
        std::thread::sleep(std::time::Duration::from_millis(2500));
        Guard(c)
    }

    async fn connect(accept: bool, port: u16) -> Result<(), String> {
        super::remote_connect(
            "t1".into(),
            "sftp".into(),
            "127.0.0.1".into(),
            port,
            "nadie".into(),
            "mala".into(),
            "/".into(),
            Some(accept),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn unknown_then_trusted_then_changed_key() {
        let Ok(py) = std::env::var("OWEE_SSH_TEST_PY") else {
            println!("SKIP hostkey (define OWEE_SSH_TEST_PY)");
            return;
        };
        let dir = std::env::temp_dir().join("owee_hostkey_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        std::env::set_var("HOME", dir.join("home"));
        let script = dir.join("srv.py");
        std::fs::write(&script, SERVER).unwrap();
        let port = 2299;

        let srv = start(&py, port, &dir.join("k1"), &script);
        let e1 = connect(false, port).await.unwrap_err();
        println!("1) servidor nuevo, sin confiar -> {e1}");
        assert!(e1.starts_with("HOSTKEY_UNKNOWN|"), "{e1}");
        assert!(e1.contains("SHA256:"));
        assert!(
            !dir.join("home/.ssh/known_hosts").exists(),
            "no debe guardar nada sin permiso"
        );

        let e2 = connect(true, port).await.unwrap_err();
        println!("2) confiando -> {e2}");
        assert!(
            !e2.starts_with("HOSTKEY_"),
            "tras confiar debe llegar al login: {e2}"
        );
        let kh = std::fs::read_to_string(dir.join("home/.ssh/known_hosts")).unwrap();
        println!(
            "   known_hosts guardado: {}",
            kh.lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>()
        );
        assert!(kh.contains("[127.0.0.1]:2299"));

        let e3 = connect(false, port).await.unwrap_err();
        println!("3) otra vez, sin pedir confianza -> {e3}");
        assert!(!e3.starts_with("HOSTKEY_"), "{e3}");

        drop(srv);
        let _srv2 = start(&py, port, &dir.join("k2"), &script); // same address, different key = possible attack
        let e4 = connect(true, port).await.unwrap_err();
        println!(
            "4) la clave cambió -> {}",
            e4.chars().take(90).collect::<String>()
        );
        assert!(e4.starts_with("HOSTKEY_MISMATCH|"), "{e4}");
    }
}

#[cfg(test)]
mod ftp_tests {
    //! Needs an FTP server: set OWEE_FTP_TEST_PY to a python with pyftpdlib + pyOpenSSL. Exercises the same
    //! commands the UI calls, for plain FTP and for explicit FTPS (self-signed cert: ask, pin, detect change).
    use std::process::{Child, Command, Stdio};

    const SERVER: &str = r#"
import sys
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import FTPHandler, TLS_FTPHandler
from pyftpdlib.servers import FTPServer
mode, port, root, cert = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None
auth = DummyAuthorizer(); auth.add_user('owee', 'secreto', root, perm='elradfmwMT')
if mode == 'tls':
    h = TLS_FTPHandler; h.certfile = cert; h.tls_control_required = True; h.tls_data_required = True
else: h = FTPHandler
h.authorizer = auth; h.passive_ports = range(30100, 30200)
FTPServer(('127.0.0.1', port), h).serve_forever()
"#;

    struct Guard(Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn spawn(
        py: &str,
        script: &std::path::Path,
        mode: &str,
        port: u16,
        root: &std::path::Path,
        cert: Option<&std::path::Path>,
    ) -> Guard {
        let mut c = Command::new(py);
        c.arg(script).arg(mode).arg(port.to_string()).arg(root);
        if let Some(p) = cert {
            c.arg(p);
        }
        let child = c
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("python");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        Guard(child)
    }
    fn make_cert(dir: &std::path::Path, name: &str) -> (std::path::PathBuf, String) {
        let pem = dir.join(format!("{name}.pem"));
        let st = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&pem)
            .arg("-out")
            .arg(&pem)
            .args(["-days", "2", "-subj", "/CN=localhost"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(st.success());
        let out = Command::new("openssl")
            .args(["x509", "-noout", "-fingerprint", "-sha256", "-in"])
            .arg(&pem)
            .output()
            .unwrap();
        let fp = String::from_utf8_lossy(&out.stdout)
            .trim()
            .split('=')
            .nth(1)
            .unwrap()
            .to_string();
        (pem, fp)
    }

    async fn connect(
        id: &str,
        proto: &str,
        port: u16,
        trust: Option<String>,
    ) -> Result<(), String> {
        super::remote_connect(
            id.into(),
            proto.into(),
            "127.0.0.1".into(),
            port,
            "owee".into(),
            "secreto".into(),
            "/".into(),
            None,
            trust,
        )
        .await
    }

    /// list, write, read, mkdir, rename, upload, download, delete — all through the same wrapper the UI uses
    async fn exercise(id: &str, root: &std::path::Path, label: &str) {
        let app = tauri::test::mock_app();
        let h = app.handle().clone();
        let ls = super::remote_list_dir(id.into(), "/".into()).await.unwrap();
        assert!(
            ls.iter().any(|e| e.name == "hola.txt" && !e.is_dir),
            "{label}: no lista hola.txt: {:?}",
            ls.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            super::remote_read_file(id.into(), "/hola.txt".into())
                .await
                .unwrap()
                .trim(),
            "hola desde el servidor"
        );
        super::remote_write_file(id.into(), "/nuevo.txt".into(), "contenido nuevo".into())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("nuevo.txt")).unwrap(),
            "contenido nuevo"
        );
        super::remote_mkdir(id.into(), "/carpeta".into())
            .await
            .unwrap();
        assert!(root.join("carpeta").is_dir());
        super::remote_rename(
            id.into(),
            "/nuevo.txt".into(),
            "/carpeta/renombrado.txt".into(),
        )
        .await
        .unwrap();
        assert!(root.join("carpeta/renombrado.txt").exists());
        let local = root.join("../subir.bin");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&local, &data).unwrap();
        super::remote_upload_file(
            id.into(),
            local.to_string_lossy().into(),
            "/carpeta/grande.bin".into(),
            h.clone(),
        )
        .await
        .unwrap();
        let got = std::fs::read(root.join("carpeta/grande.bin")).unwrap();
        assert_eq!(got.len(), data.len(), "{label}: tamaño tras subir");
        assert!(got == data, "{label}: la subida no coincide");
        let down = root.join("../bajado.bin");
        super::remote_download_file(
            id.into(),
            "/carpeta/grande.bin".into(),
            down.to_string_lossy().into(),
            data.len() as u64,
            h,
        )
        .await
        .unwrap();
        let back = std::fs::read(&down).unwrap();
        assert_eq!(back.len(), data.len(), "{label}: tamaño tras bajar");
        assert!(back == data, "{label}: la descarga no coincide");
        super::remote_delete(id.into(), "/carpeta/grande.bin".into(), false)
            .await
            .unwrap();
        super::remote_delete(id.into(), "/carpeta/renombrado.txt".into(), false)
            .await
            .unwrap();
        super::remote_delete(id.into(), "/carpeta".into(), true)
            .await
            .unwrap();
        assert!(
            !root.join("carpeta").exists(),
            "{label}: no borró la carpeta"
        );
        super::remote_disconnect(id.into()).await.unwrap();
        println!("   {label}: listar, leer, escribir, crear carpeta, renombrar, subir 200 KB, bajar 200 KB (idéntico), borrar → OK");
    }

    #[tokio::test]
    async fn plain_ftp_and_ftps_end_to_end() {
        let Ok(py) = std::env::var("OWEE_FTP_TEST_PY") else {
            println!("SKIP ftp (define OWEE_FTP_TEST_PY)");
            return;
        };
        let base = std::env::temp_dir().join("owee_ftp_test");
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("srv");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("hola.txt"), "hola desde el servidor").unwrap();
        let script = base.join("srv.py");
        std::fs::write(&script, SERVER).unwrap();

        // plain FTP
        let p = spawn(&py, &script, "plain", 2121, &root, None);
        connect("ftp1", "ftp", 2121, None).await.expect("ftp plano");
        exercise("ftp1", &root, "FTP plano").await;
        drop(p);

        // FTPS with a self-signed certificate
        let (cert1, fp1) = make_cert(&base, "c1");
        let s = spawn(&py, &script, "tls", 2122, &root, Some(&cert1));
        let e = connect("ftps1", "ftps", 2122, None).await.unwrap_err();
        println!(
            "1) FTPS cert propio, sin confiar -> {}",
            e.chars().take(110).collect::<String>()
        );
        assert!(e.starts_with("CERT_UNTRUSTED|"), "{e}");
        let shown = e.split('|').nth(1).unwrap().to_string();
        assert_eq!(
            shown.to_uppercase(),
            fp1.to_uppercase(),
            "la huella mostrada debe ser la real del certificado"
        );
        connect("ftps2", "ftps", 2122, Some(shown.clone()))
            .await
            .expect("ftps tras confiar");
        exercise("ftps2", &root, "FTPS (cert fijado)").await;
        drop(s);

        // the server now presents a different certificate: the pinned fingerprint must refuse it
        let (cert2, fp2) = make_cert(&base, "c2");
        let _s2 = spawn(&py, &script, "tls", 2122, &root, Some(&cert2));
        let e2 = connect("ftps3", "ftps", 2122, Some(shown))
            .await
            .unwrap_err();
        println!(
            "2) el certificado cambió -> {}",
            e2.chars().take(100).collect::<String>()
        );
        assert!(e2.starts_with("CERT_MISMATCH|"), "{e2}");
        assert_eq!(
            e2.split('|').nth(1).unwrap().to_uppercase(),
            fp2.to_uppercase()
        );
    }
}
