// Helpers for the browser preview: which local servers are running, how this project is usually started,
// and a tiny static file server for plain HTML folders.

use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Clone, Debug)]
pub struct ListeningPort {
    pub port: u16,
    pub address: String,
    pub pid: Option<u32>,
    pub process: String,
    pub cmdline: String,
    /// looks like a web/dev server (as opposed to a database, ssh, dns…)
    pub dev: bool,
}

const WEB_PROCS: &[&str] = &[
    "node",
    "bun",
    "deno",
    "vite",
    "php",
    "php-fpm",
    "php-cgi",
    "python",
    "python3",
    "flask",
    "uvicorn",
    "gunicorn",
    "hypercorn",
    "go",
    "main",
    "ruby",
    "puma",
    "rails",
    "java",
    "dotnet",
    "cargo",
    "npm",
    "next-server",
    "nginx",
    "apache2",
    "httpd",
    "caddy",
    "docker-proxy",
    "hugo",
    "jekyll",
    "webpack",
    "esbuild",
];
const NOT_WEB_PORTS: &[u16] = &[
    22, 25, 53, 111, 631, 3306, 5432, 6379, 27017, 11211, 5353, 11434, 2222, 21,
];

fn looks_like_dev(port: u16, process: &str) -> bool {
    if NOT_WEB_PORTS.contains(&port) {
        return false;
    }
    let p = process.to_lowercase();
    WEB_PROCS.iter().any(|w| p == *w || p.starts_with(w))
        || matches!(port, 80 | 443 | 3000..=3999 | 4000..=4999 | 5000..=5999 | 8000..=8999 | 9000..=9099 | 1234 | 1420)
}

#[cfg(target_os = "linux")]
fn listening_ports_impl() -> Vec<ListeningPort> {
    // 1) listening sockets: inode -> (port, address)
    let mut socks: HashMap<u64, (u16, String)> = HashMap::new();
    for (file, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                continue;
            } // 0A = LISTEN
            let Some((ip_hex, port_hex)) = f[1].split_once(':') else {
                continue;
            };
            let Ok(port) = u16::from_str_radix(port_hex, 16) else {
                continue;
            };
            let Ok(inode) = f[9].parse::<u64>() else {
                continue;
            };
            let addr = if !v6 {
                let Ok(n) = u32::from_str_radix(ip_hex, 16) else {
                    continue;
                };
                let b = n.to_le_bytes();
                format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
            } else if ip_hex.chars().all(|c| c == '0') {
                "::".into()
            } else if ip_hex == "00000000000000000000000001000000" {
                "::1".into()
            } else if ip_hex.starts_with("0000000000000000FFFF0000") {
                let Ok(n) = u32::from_str_radix(&ip_hex[24..], 16) else {
                    continue;
                };
                let b = n.to_le_bytes();
                format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
            } else {
                continue;
            };
            // only what a browser on this machine can reach through localhost
            if addr.starts_with("127.") || addr == "0.0.0.0" || addr == "::" || addr == "::1" {
                socks.insert(inode, (port, addr));
            }
        }
    }
    // 2) which process owns each socket (only the user's own processes are readable)
    let mut owner: HashMap<u64, u32> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd.path()) {
                    let s = link.to_string_lossy();
                    if let Some(i) = s
                        .strip_prefix("socket:[")
                        .and_then(|x| x.strip_suffix(']'))
                        .and_then(|x| x.parse::<u64>().ok())
                    {
                        if socks.contains_key(&i) {
                            owner.entry(i).or_insert(pid);
                        }
                    }
                }
            }
        }
    }
    let mut by_port: HashMap<u16, ListeningPort> = HashMap::new();
    for (inode, (port, addr)) in socks {
        let pid = owner.get(&inode).copied();
        let process = pid
            .and_then(|p| std::fs::read_to_string(format!("/proc/{p}/comm")).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let cmdline = pid
            .and_then(|p| std::fs::read(format!("/proc/{p}/cmdline")).ok())
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .replace('\0', " ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let dev = looks_like_dev(port, &process);
        let item = ListeningPort {
            port,
            address: addr,
            pid,
            process,
            cmdline,
            dev,
        };
        match by_port.get(&port) {
            Some(old) if old.pid.is_some() => {}
            _ => {
                by_port.insert(port, item);
            }
        }
    }
    let mut v: Vec<_> = by_port.into_values().collect();
    v.sort_by_key(|p| (!p.dev, p.port));
    v
}

#[cfg(not(target_os = "linux"))]
fn listening_ports_impl() -> Vec<ListeningPort> {
    // macOS: lsof; Windows: netstat. Parsed loosely: port + process name.
    let mut out: Vec<ListeningPort> = Vec::new();
    #[cfg(target_os = "windows")]
    if let Ok(o) = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
    {
        for l in String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| l.contains("LISTENING"))
        {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() >= 5 {
                if let Some(port) = f[1].rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
                    out.push(ListeningPort {
                        port,
                        address: f[1].to_string(),
                        pid: f[4].parse().ok(),
                        process: String::new(),
                        cmdline: String::new(),
                        dev: looks_like_dev(port, ""),
                    });
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(o) = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN"])
        .output()
    {
        for l in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() >= 9 {
                if let Some(port) = f[8].rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
                    out.push(ListeningPort {
                        port,
                        address: f[8].to_string(),
                        pid: f[1].parse().ok(),
                        process: f[0].to_string(),
                        cmdline: String::new(),
                        dev: looks_like_dev(port, f[0]),
                    });
                }
            }
        }
    }
    out.sort_by_key(|p| (!p.dev, p.port));
    out.dedup_by_key(|p| p.port);
    out
}

/// Local TCP ports that are accepting connections right now (what a browser on this machine could open).
#[tauri::command]
pub async fn list_listening_ports() -> Vec<ListeningPort> {
    tokio::task::spawn_blocking(listening_ports_impl)
        .await
        .unwrap_or_default()
}

// ───────────────────────── how this project is usually started ─────────────────────────

#[derive(Serialize, Clone, Debug)]
pub struct DevSuggestion {
    pub label: String,
    pub command: String,
    pub port: Option<u16>,
    pub note: String,
    /// "node" | "php" | "python" | "go" | "rust" | "docker" | "static"
    pub kind: String,
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn port_in(text: &str, patterns: &[&str]) -> Option<u16> {
    for p in patterns {
        if let Some(c) = regex::Regex::new(p).ok().and_then(|r| r.captures(text)) {
            if let Ok(n) = c[1].parse::<u16>() {
                return Some(n);
            }
        }
    }
    None
}

fn default_port_for(script: &str) -> Option<u16> {
    let s = script;
    let table: &[(&str, u16)] = &[
        ("vite preview", 4173),
        ("quasar", 9000),
        ("vite", 5173),
        ("astro", 4321),
        ("nuxt", 3000),
        ("next", 3000),
        ("react-scripts", 3000),
        ("vue-cli-service", 8080),
        ("webpack serve", 8080),
        ("webpack-dev-server", 8080),
        ("ng serve", 4200),
        ("svelte-kit", 5173),
        ("remix", 3000),
        ("expo", 8081),
        ("parcel", 1234),
        ("http-server", 8080),
        ("nodemon", 3000),
    ];
    table.iter().find(|(k, _)| s.contains(k)).map(|(_, p)| *p)
}

fn config_port(dir: &Path) -> Option<u16> {
    for name in [
        "vite.config.ts",
        "vite.config.js",
        "vite.config.mts",
        "vite.config.mjs",
        "quasar.config.ts",
        "quasar.config.js",
        "quasar.config.cjs",
        "nuxt.config.ts",
        "astro.config.mjs",
        "astro.config.ts",
        "vue.config.js",
    ] {
        let t = read(&dir.join(name));
        if t.is_empty() {
            continue;
        }
        if let Some(p) = port_in(&t, &[r"port\s*:\s*(\d{2,5})"]) {
            return Some(p);
        }
    }
    port_in(
        &read(&dir.join(".env")),
        &[r"(?m)^\s*(?:VITE_)?PORT\s*=\s*(\d{2,5})"],
    )
}

fn package_manager(dir: &Path, root: &Path) -> (&'static str, &'static str) {
    for d in [dir, root] {
        if d.join("pnpm-lock.yaml").exists() {
            return ("pnpm", "pnpm");
        }
        if d.join("yarn.lock").exists() {
            return ("yarn", "yarn");
        }
        if d.join("bun.lockb").exists() || d.join("bun.lock").exists() {
            return ("bun", "bun run");
        }
        if d.join("package-lock.json").exists() {
            return ("npm", "npm run");
        }
    }
    ("npm", "npm run")
}

fn node_suggestions(dir: &Path, root: &Path, out: &mut Vec<DevSuggestion>) {
    let pkg = read(&dir.join("package.json"));
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&pkg) else {
        return;
    };
    let Some(scripts) = v.get("scripts").and_then(|s| s.as_object()) else {
        return;
    };
    let rel = dir
        .strip_prefix(root)
        .ok()
        .map(|r| r.to_string_lossy().to_string())
        .filter(|r| !r.is_empty());
    let (_, runner) = package_manager(dir, root);
    let cfg = config_port(dir);
    for key in ["dev", "start", "serve", "preview"] {
        let Some(script) = scripts.get(key).and_then(|s| s.as_str()) else {
            continue;
        };
        if key == "start" && scripts.contains_key("dev") {
            continue;
        } // `start` usually runs the built app
        if script.contains("tauri") && !script.contains("vite") {
            continue;
        }
        // `server.port` in the config is the dev server's; `vite preview` has its own default (4173)
        let cfg = if key == "preview" { None } else { cfg };
        let port = port_in(
            script,
            &[
                r"--port[ =](\d{2,5})",
                r"(?:^|\s)-p[ =]?(\d{2,5})",
                r"PORT=(\d{2,5})",
            ],
        )
        .or(cfg)
        .or_else(|| default_port_for(script))
        // `npm run dev -w apps/web` / `--prefix apps/web`: the port belongs to that workspace
        .or_else(|| {
            let ws = regex::Regex::new(r"(?:-w|--workspace[ =]|--prefix[ =]?)\s*([\w./@-]+)")
                .ok()?
                .captures(script)?[1]
                .to_string();
            let wdir = root.join(&ws);
            config_port(&wdir).or_else(|| {
                let pkg =
                    serde_json::from_str::<serde_json::Value>(&read(&wdir.join("package.json")))
                        .ok()?;
                let inner = pkg.get("scripts")?.get(key)?.as_str()?.to_string();
                default_port_for(&inner)
            })
        });
        let cmd = if runner == "pnpm" || runner == "yarn" {
            format!("{runner} {key}")
        } else {
            format!("{runner} {key}")
        };
        let command = match &rel {
            Some(r) => format!("cd {r} && {cmd}"),
            None => cmd,
        };
        out.push(DevSuggestion {
            label: format!(
                "{}{} ({})",
                rel.as_deref().map(|r| format!("{r}: ")).unwrap_or_default(),
                key,
                script.chars().take(40).collect::<String>()
            ),
            command,
            port,
            note: if port.is_some() {
                String::new()
            } else {
                "No pude saber el puerto: se detecta solo cuando arranque".into()
            },
            kind: "node".into(),
        });
    }
}

/// How this project is normally run in development, so the preview can offer "start it and show it".
#[tauri::command]
pub fn detect_dev_servers(root: String) -> Vec<DevSuggestion> {
    let root_p = PathBuf::from(&root);
    let mut out: Vec<DevSuggestion> = Vec::new();

    node_suggestions(&root_p, &root_p, &mut out);
    // monorepos: apps/*, packages/*, services/*, web/, frontend/
    for group in ["apps", "packages", "services", "web", "frontend", "client"] {
        let g = root_p.join(group);
        if g.join("package.json").exists() {
            node_suggestions(&g, &root_p, &mut out);
        }
        if let Ok(rd) = std::fs::read_dir(&g) {
            for e in rd.flatten().take(12) {
                if e.path().is_dir() {
                    node_suggestions(&e.path(), &root_p, &mut out);
                }
            }
        }
    }

    // PHP
    let composer = read(&root_p.join("composer.json"));
    if root_p.join("artisan").exists() {
        out.push(DevSuggestion {
            label: "Laravel: php artisan serve".into(),
            command: "php artisan serve".into(),
            port: Some(8000),
            note: String::new(),
            kind: "php".into(),
        });
    } else if !composer.is_empty()
        || root_p.join("index.php").exists()
        || root_p.join("public/index.php").exists()
    {
        let docroot = if root_p.join("public/index.php").exists() {
            " -t public"
        } else {
            ""
        };
        out.push(DevSuggestion {
            label: "PHP: servidor integrado".into(),
            command: format!("php -S 127.0.0.1:8000{docroot}"),
            port: Some(8000),
            note: "Sirve el proyecto con el servidor de PHP".into(),
            kind: "php".into(),
        });
    }
    // Python
    if root_p.join("manage.py").exists() {
        out.push(DevSuggestion {
            label: "Django: runserver".into(),
            command: "python3 manage.py runserver".into(),
            port: Some(8000),
            note: String::new(),
            kind: "python".into(),
        });
    } else {
        let reqs = format!(
            "{}{}",
            read(&root_p.join("requirements.txt")),
            read(&root_p.join("pyproject.toml"))
        )
        .to_lowercase();
        if reqs.contains("fastapi") {
            out.push(DevSuggestion {
                label: "FastAPI: uvicorn".into(),
                command: "uvicorn main:app --reload".into(),
                port: Some(8000),
                note: "Ajustá main:app si tu archivo se llama distinto".into(),
                kind: "python".into(),
            });
        } else if reqs.contains("flask") {
            out.push(DevSuggestion {
                label: "Flask: flask run".into(),
                command: "flask run --debug".into(),
                port: Some(5000),
                note: String::new(),
                kind: "python".into(),
            });
        }
    }
    // Go: port from ListenAndServe(":8080")
    if root_p.join("go.mod").exists() {
        let mut port = None;
        if let Ok(rd) = std::fs::read_dir(&root_p) {
            for e in rd.flatten().take(60) {
                if e.path().extension().map(|x| x == "go").unwrap_or(false) {
                    port = port.or(port_in(
                        &read(&e.path()),
                        &[r#"ListenAndServe\(\s*"[^":]*:(\d{2,5})""#],
                    ));
                }
            }
        }
        out.push(DevSuggestion {
            label: "Go: go run .".into(),
            command: "go run .".into(),
            port,
            note: if port.is_none() {
                "Puerto no detectado".into()
            } else {
                String::new()
            },
            kind: "go".into(),
        });
    }
    // Docker compose: published ports
    for name in [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ] {
        let t = read(&root_p.join(name));
        if t.is_empty() {
            continue;
        }
        let ports: Vec<u16> = regex::Regex::new(
            r#"(?m)^\s*-\s*["']?(?:\d+\.\d+\.\d+\.\d+:)?(\d{2,5}):\d{2,5}["']?\s*$"#,
        )
        .unwrap()
        .captures_iter(&t)
        .filter_map(|c| c[1].parse().ok())
        .filter(|p: &u16| !NOT_WEB_PORTS.contains(p))
        .collect();
        out.push(DevSuggestion {
            label: "Docker Compose: levantar servicios".into(),
            command: "docker compose up".into(),
            port: ports.first().copied(),
            note: if ports.is_empty() {
                String::new()
            } else {
                format!(
                    "Puertos publicados: {}",
                    ports
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            kind: "docker".into(),
        });
        break;
    }
    // Static folders the built-in server can show
    for d in ["", "dist", "build", "public", "site", "docs", "out"] {
        let dir = if d.is_empty() {
            root_p.clone()
        } else {
            root_p.join(d)
        };
        if dir.join("index.html").exists() {
            out.push(DevSuggestion {
                label: format!(
                    "Servir {} (HTML estático)",
                    if d.is_empty() {
                        "la carpeta del proyecto"
                    } else {
                        d
                    }
                ),
                command: format!("static:{d}"),
                port: None,
                note: "Servidor incluido en OweeCode, sin instalar nada".into(),
                kind: "static".into(),
            });
        }
    }
    // plain PHP / HTML projects keep their site in a subfolder (app/, site/, www/, htdocs/…)
    if !out.iter().any(|x| x.kind == "php" || x.kind == "static") || !composer.is_empty() == false {
        if let Ok(rd) = std::fs::read_dir(&root_p) {
            let mut n = 0;
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !e.path().is_dir()
                    || name.starts_with('.')
                    || [
                        "node_modules",
                        "vendor",
                        "dist",
                        "build",
                        "docs",
                        "tools",
                        "db",
                        "infra",
                    ]
                    .contains(&name.as_str())
                {
                    continue;
                }
                if e.path().join("index.php").exists() && n < 4 {
                    out.push(DevSuggestion {
                        label: format!("PHP: servir {name}/"),
                        command: format!("php -S 127.0.0.1:8000 -t {name}"),
                        port: Some(8000),
                        note: "Servidor integrado de PHP".into(),
                        kind: "php".into(),
                    });
                    n += 1;
                } else if e.path().join("index.html").exists() && n < 4 {
                    out.push(DevSuggestion {
                        label: format!("Servir {name}/ (HTML estático)"),
                        command: format!("static:{name}"),
                        port: None,
                        note: "Servidor incluido en OweeCode".into(),
                        kind: "static".into(),
                    });
                    n += 1;
                }
            }
        }
    }
    // no duplicates by command
    let mut seen = HashSet::new();
    out.retain(|s| seen.insert(s.command.clone()));
    out.truncate(14);
    out
}

// ───────────────────────── project change detector (for live reload) ─────────────────────────

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Fingerprint {
    pub count: usize,
    pub newest_ms: u64,
    pub newest_path: String,
}

const WATCH_EXT: &[&str] = &[
    "html", "htm", "css", "scss", "sass", "less", "js", "mjs", "ts", "jsx", "tsx", "vue", "svelte",
    "json", "php", "phtml", "twig", "blade", "py", "go", "rs", "md", "svg", "png", "jpg", "jpeg",
    "webp", "gif", "ico", "txt", "yml", "yaml", "env", "ini", "xml", "sql", "tpl", "hbs",
];
const WATCH_SKIP: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".quasar",
    ".next",
    ".nuxt",
    ".output",
    "vendor",
    "__pycache__",
    "venv",
    ".venv",
    "env",
    "storage",
    "cache",
    ".cache",
    "coverage",
    ".idea",
    ".vscode",
    "tmp",
    "logs",
    ".owee",
];

fn fingerprint_walk(dir: &Path, depth: u32, fp: &mut Fingerprint) {
    if depth > 9 || fp.count > 25_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if name.starts_with('.') && name != ".well-known" || WATCH_SKIP.contains(&name.as_str())
            {
                continue;
            }
            fingerprint_walk(&e.path(), depth + 1, fp);
        } else if ft.is_file() {
            let ext = e
                .path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_lowercase())
                .unwrap_or_default();
            if !WATCH_EXT.contains(&ext.as_str()) && !name.starts_with(".env") {
                continue;
            }
            fp.count += 1;
            let ms = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if ms > fp.newest_ms {
                fp.newest_ms = ms;
                fp.newest_path = e.path().to_string_lossy().to_string();
            }
        }
    }
}

/// Cheap "did anything in the project change?": number of source files and the newest modification time.
/// The preview polls this and reloads when it changes (no inotify limits, works on any OS).
#[tauri::command]
pub async fn project_fingerprint(root: String) -> Fingerprint {
    tokio::task::spawn_blocking(move || {
        let mut fp = Fingerprint {
            count: 0,
            newest_ms: 0,
            newest_path: String::new(),
        };
        fingerprint_walk(Path::new(&root), 0, &mut fp);
        fp
    })
    .await
    .unwrap_or(Fingerprint {
        count: 0,
        newest_ms: 0,
        newest_path: String::new(),
    })
}

// ───────────────────────── tiny static file server ─────────────────────────

static STATIC_SERVERS: Lazy<Mutex<HashMap<u16, tokio::task::JoinHandle<()>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        Some("mp4") => "video/mp4",
        Some("webmanifest") => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 + 0 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Maps a request path to a file inside `root`; None if it would leave the folder.
fn resolve_static(root: &Path, req_path: &str) -> Option<PathBuf> {
    let clean = percent_decode(req_path.split(['?', '#']).next().unwrap_or("/"));
    let rel = clean.trim_start_matches('/');
    if rel.split('/').any(|seg| seg == "..") {
        return None;
    }
    let joined = root.join(rel);
    let real = joined.canonicalize().ok()?;
    if !real.starts_with(root) {
        return None;
    }
    Some(real)
}

fn css_stamp_walk(dir: &Path, depth: u32, newest: &mut u128) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        if path.is_dir() {
            if name.starts_with('.') || WATCH_SKIP.contains(&name.as_str()) {
                continue;
            }
            css_stamp_walk(&path, depth + 1, newest);
        } else if name.ends_with(".css") {
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                let ms = m
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                if ms > *newest {
                    *newest = ms;
                }
            }
        }
    }
}

/// Tiny script added to the HTML pages of the static server: when a .css file changes it re-links the stylesheets, no page reload.
const CSS_LIVE: &str = "<script>(function(){var last=null;function tick(){fetch('/__owee/css',{cache:'no-store'}).then(function(r){return r.text()}).then(function(t){if(last!==null&&t!==last){document.querySelectorAll('link[rel~=stylesheet]').forEach(function(l){var u=new URL(l.href,location.href);if(u.origin!==location.origin)return;u.searchParams.set('_owee',Date.now());l.href=u.toString()})}last=t}).catch(function(){})}setInterval(tick,1000);tick()})()</script>";

fn inject_css_live(html: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(html).to_string();
    let lower = text.to_ascii_lowercase();
    let at = lower.rfind("</body>").unwrap_or(text.len());
    let mut out = String::with_capacity(text.len() + CSS_LIVE.len());
    out.push_str(&text[..at]);
    out.push_str(CSS_LIVE);
    out.push_str(&text[at..]);
    out.into_bytes()
}

async fn serve_one(mut sock: tokio::net::TcpStream, root: PathBuf) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 8192];
    let Ok(n) = tokio::time::timeout(std::time::Duration::from_secs(5), sock.read(&mut buf))
        .await
        .unwrap_or(Ok(0))
    else {
        return;
    };
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    let first = head.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let respond = |status: &str, ctype: &str, body: &[u8], head_only: bool| {
        let mut r = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        if !head_only {
            r.extend_from_slice(body);
        }
        r
    };
    let reply = if method != "GET" && method != "HEAD" {
        respond("405 Method Not Allowed", "text/plain", b"solo GET", false)
    } else {
        let head_only = method == "HEAD";
        if target.split(['?', '#']).next() == Some("/__owee/css") {
            let r = root.clone();
            let stamp = tokio::task::spawn_blocking(move || {
                let mut n = 0u128;
                css_stamp_walk(&r, 0, &mut n);
                n
            })
            .await
            .unwrap_or(0);
            let _ = sock
                .write_all(&respond(
                    "200 OK",
                    "text/plain",
                    stamp.to_string().as_bytes(),
                    head_only,
                ))
                .await;
            return;
        }
        match resolve_static(&root, target) {
            None => respond(
                "403 Forbidden",
                "text/plain; charset=utf-8",
                "Fuera de la carpeta servida".as_bytes(),
                head_only,
            ),
            Some(mut p) => {
                if p.is_dir() {
                    let idx = p.join("index.html");
                    if idx.is_file() {
                        p = idx;
                    } else {
                        let mut names: Vec<String> = std::fs::read_dir(&p)
                            .map(|rd| {
                                rd.flatten()
                                    .map(|e| {
                                        let n = e.file_name().to_string_lossy().to_string();
                                        if e.path().is_dir() {
                                            n + "/"
                                        } else {
                                            n
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        names.sort();
                        let base = target
                            .split(['?', '#'])
                            .next()
                            .unwrap_or("/")
                            .trim_end_matches('/');
                        let html = format!("<!doctype html><meta charset=utf-8><title>Índice</title><body style=\"font:14px system-ui;padding:20px\"><h3>Índice de {}/</h3><ul>{}</ul>", base,
                            names.iter().map(|n| format!("<li><a href=\"{base}/{n}\">{n}</a></li>")).collect::<String>());
                        let r = respond(
                            "200 OK",
                            "text/html; charset=utf-8",
                            html.as_bytes(),
                            head_only,
                        );
                        let _ = sock.write_all(&r).await;
                        return;
                    }
                }
                match tokio::fs::read(&p).await {
                    Ok(bytes) if mime(&p).starts_with("text/html") => {
                        respond("200 OK", mime(&p), &inject_css_live(&bytes), head_only)
                    }
                    Ok(bytes) => respond("200 OK", mime(&p), &bytes, head_only),
                    Err(_) => respond(
                        "404 Not Found",
                        "text/html; charset=utf-8",
                        "<h3>404</h3><p>No existe ese archivo.</p>".as_bytes(),
                        head_only,
                    ),
                }
            }
        }
    };
    let _ = sock.write_all(&reply).await;
}

/// Starts serving `folder` (inside the project) on 127.0.0.1. `preferred` or the first free port from 8080. Returns the port.
#[tauri::command]
pub async fn static_server_start(folder: String, preferred: Option<u16>) -> Result<u16, String> {
    let root = Path::new(&folder)
        .canonicalize()
        .map_err(|e| format!("Carpeta inválida: {e}"))?;
    if !root.is_dir() {
        return Err("No es una carpeta".into());
    }
    let mut candidates: Vec<u16> = preferred.into_iter().collect();
    candidates.extend(8080..8100);
    for port in candidates {
        if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            let root2 = root.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((sock, _)) = listener.accept().await else {
                        break;
                    };
                    let r = root2.clone();
                    tokio::spawn(serve_one(sock, r));
                }
            });
            STATIC_SERVERS.lock().unwrap().insert(port, handle);
            return Ok(port);
        }
    }
    Err("No encontré un puerto libre entre 8080 y 8099".into())
}

#[tauri::command]
pub fn static_server_stop(port: u16) {
    if let Some(h) = STATIC_SERVERS.lock().unwrap().remove(&port) {
        h.abort();
    }
}

#[tauri::command]
pub fn static_server_ports() -> Vec<u16> {
    STATIC_SERVERS.lock().unwrap().keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fingerprint_changes_with_edits_new_and_deleted_files_but_not_with_node_modules() {
        let dir = std::env::temp_dir().join("owee_fp_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("src/app.js"), "1").unwrap();
        let root = dir.to_string_lossy().to_string();
        let a = project_fingerprint(root.clone()).await;
        assert_eq!(a.count, 1);
        assert!(a.newest_path.ends_with("app.js"));
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        std::fs::write(dir.join("node_modules/x/big.js"), "ruido").unwrap();
        assert_eq!(
            project_fingerprint(root.clone()).await,
            a,
            "node_modules no cuenta"
        );
        std::fs::write(dir.join("src/app.js"), "2").unwrap();
        let b = project_fingerprint(root.clone()).await;
        assert!(b.newest_ms > a.newest_ms, "editar cambia la huella");
        std::fs::write(dir.join("index.html"), "x").unwrap();
        let c = project_fingerprint(root.clone()).await;
        assert_ne!(c, b, "un archivo nuevo cambia la huella");
        assert_eq!(c.count, 2);
        std::fs::remove_file(dir.join("index.html")).unwrap();
        assert_eq!(
            project_fingerprint(root).await.count,
            1,
            "borrar también cuenta"
        );
    }

    #[test]
    fn static_paths_cannot_escape_the_served_folder() {
        let base = std::env::temp_dir().join("owee_static_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("site/sub")).unwrap();
        std::fs::write(base.join("secret.txt"), "no").unwrap();
        std::fs::write(base.join("site/index.html"), "hola").unwrap();
        let root = base.join("site").canonicalize().unwrap();
        assert!(resolve_static(&root, "/index.html").is_some());
        assert!(resolve_static(&root, "/../secret.txt").is_none());
        assert!(resolve_static(&root, "/%2e%2e/secret.txt").is_none());
        assert!(resolve_static(&root, "/sub/../../secret.txt").is_none());
        assert!(resolve_static(&root, "/no-existe").is_none());
    }

    #[tokio::test]
    async fn static_server_serves_files_index_and_blocks_traversal() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let base = std::env::temp_dir().join("owee_static_test2");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("site/css")).unwrap();
        std::fs::write(base.join("secret.txt"), "TOP SECRET").unwrap();
        std::fs::write(base.join("site/index.html"), "<h1>Hola</h1>").unwrap();
        std::fs::write(base.join("site/css/a.css"), "body{}").unwrap();
        let port = static_server_start(base.join("site").to_string_lossy().to_string(), Some(8391))
            .await
            .unwrap();
        let get = |path: String| async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        };
        let root = get("/".into()).await;
        assert!(
            root.starts_with("HTTP/1.1 200")
                && root.contains("<h1>Hola</h1>")
                && root.contains("text/html"),
            "{root}"
        );
        assert!(
            root.contains("/__owee/css") && root.find("<h1>Hola</h1>") < root.find("<script>"),
            "script inyectado tras el contenido: {root}"
        );
        let stamp1 = get("/__owee/css".into()).await;
        assert!(stamp1.starts_with("HTTP/1.1 200"), "{stamp1}");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        std::fs::write(base.join("site/css/b.css"), "p{}").unwrap();
        let stamp2 = get("/__owee/css".into()).await;
        assert_ne!(
            stamp1.rsplit("\r\n\r\n").next(),
            stamp2.rsplit("\r\n\r\n").next(),
            "el sello cambia al tocar un .css"
        );
        let css = get("/css/a.css".into()).await;
        assert!(css.contains("text/css") && css.contains("body{}"));
        let esc = get("/../secret.txt".into()).await;
        assert!(!esc.contains("TOP SECRET") && esc.contains("403"), "{esc}");
        let esc2 = get("/%2e%2e/secret.txt".into()).await;
        assert!(!esc2.contains("TOP SECRET"), "{esc2}");
        let nf = get("/nada.html".into()).await;
        assert!(
            nf.starts_with("HTTP/1.1 403") || nf.starts_with("HTTP/1.1 404"),
            "{nf}"
        );
        assert!(static_server_ports().contains(&port));
        static_server_stop(port);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err(),
            "debe cerrar el puerto al detenerlo"
        );
    }

    #[test]
    fn detects_listening_ports_and_dev_commands() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let ports = listening_ports_impl();
        let me = ports.iter().find(|p| p.port == port);
        println!(
            "PUERTOS: {} escuchando; el de prueba {port} -> {:?}",
            ports.len(),
            me.map(|m| (&m.process, m.pid.is_some()))
        );
        #[cfg(target_os = "linux")]
        assert!(me.is_some(), "el puerto abierto por el test debe aparecer");
        let proj = std::env::temp_dir().join("owee_dev_detect");
        let _ = std::fs::remove_dir_all(&proj);
        std::fs::create_dir_all(proj.join("apps/web")).unwrap();
        std::fs::write(
            proj.join("apps/web/package.json"),
            r#"{"scripts":{"dev":"quasar dev","build":"quasar build"}}"#,
        )
        .unwrap();
        std::fs::write(
            proj.join("apps/web/quasar.config.ts"),
            "devServer: { port: 9100 }",
        )
        .unwrap();
        std::fs::write(
            proj.join("package.json"),
            r#"{"scripts":{"dev":"vite --port 3001","start":"node server.js"}}"#,
        )
        .unwrap();
        std::fs::write(proj.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(proj.join("index.html"), "<h1>x</h1>").unwrap();
        std::fs::write(proj.join("docker-compose.yml"), "services:\n  db:\n    ports:\n      - \"3306:3306\"\n  web:\n    ports:\n      - \"8088:80\"\n").unwrap();
        let s = detect_dev_servers(proj.to_string_lossy().to_string());
        for x in &s {
            println!(
                "SUGERENCIA: {:<48} | {:<38} | {:?}",
                x.label, x.command, x.port
            );
        }
        assert!(
            s.iter()
                .any(|x| x.command == "pnpm dev" && x.port == Some(3001)),
            "puerto de --port 3001"
        );
        assert!(
            s.iter()
                .any(|x| x.command.contains("apps/web") && x.port == Some(9100)),
            "puerto de quasar.config"
        );
        assert!(s.iter().any(|x| x.kind == "docker" && x.port == Some(8088)));
        assert!(s.iter().any(|x| x.kind == "static"));
        assert!(
            !s.iter().any(|x| x.label.starts_with("start")),
            "start se omite cuando existe dev"
        );
        drop(l);
    }
}

#[cfg(test)]
mod adhoc {
    #[test]
    fn real_projects() {
        let Ok(list) = std::env::var("OWEE_DEV_PROJECTS") else {
            return;
        };
        for p in list.split(':') {
            println!("PROYECTO {p}");
            for x in super::detect_dev_servers(p.to_string()) {
                println!("   {:<52} | {:<40} | {:?}", x.label, x.command, x.port);
            }
        }
        for p in super::listening_ports_impl() {
            println!(
                "ESCUCHANDO :{:<5} {:<18} {} {}",
                p.port,
                p.process,
                if p.dev { "dev" } else { "   " },
                p.cmdline.chars().take(60).collect::<String>()
            );
        }
    }
}
