// Developer-tool helpers for Preferences and the Flows view:
//  - `install_dev_tool`: installs/updates a language server or runtime from a
//    fixed whitelist (the frontend sends an id, never a raw command line)
//  - `scan_project_graph`: reads a project's source imports and returns files
//    and their dependencies so the Flows view can draw the architecture

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn install_script(id: &str) -> Option<&'static str> {
    Some(match id {
        "typescript" => "npm install -g typescript-language-server typescript@latest",
        // Pinned: lsp.rs speaks the Volar 1.x protocol (tsdk init option);
        // 2.x/3.x need a different TypeScript-plugin handshake.
        "vue" => "npm install -g @vue/language-server@1.8.27",
        "go" => "go install golang.org/x/tools/gopls@latest",
        "rust" => "rustup component add rust-analyzer",
        "php" => "composer global require phpactor/phpactor",
        "python" => "pipx install python-lsp-server || pipx upgrade python-lsp-server",
        "node" => "mise use -g node@latest",
        "cpp" => "echo 'clangd: instalalo con tu gestor de paquetes (sudo apt install clangd) y generá compile_commands.json con: cmake -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON' >&2; exit 1",
        "qml" => "echo 'qmlls viene con Qt 6.3+: instalá Qt con el Qt Online Installer (queda en ~/Qt/<versión>/<kit>/bin, OweeCode ya lo busca ahí).' >&2; exit 1",
        "dart" => "echo 'El servidor de Dart viene con el SDK. Instalá Flutter (https://docs.flutter.dev/get-started/install) o el SDK de Dart (https://dart.dev/get-dart) y reiniciá OweeCode.' >&2; exit 1",
        "kotlin" => "echo 'Descargá kotlin-language-server de https://github.com/fwcd/kotlin-language-server/releases, descomprimilo y poné su carpeta bin en el PATH.' >&2; exit 1",
        _ => return None,
    })
}

/// Installs or updates a known developer tool and returns its console output.
#[tauri::command]
pub async fn install_dev_tool(id: String) -> Result<String, String> {
    let script = install_script(&id).ok_or_else(|| format!("Herramienta desconocida: {id}"))?;
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(script);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.arg("-c").arg(script);
        c
    };
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = crate::util::resolve_login_shell_path().await {
        cmd.env("PATH", p.trim());
    }
    cmd.stdin(std::process::Stdio::null());

    let out = tokio::time::timeout(std::time::Duration::from_secs(600), cmd.output())
        .await
        .map_err(|_| "Tardó más de 10 minutos y se canceló".to_string())?
        .map_err(|e| e.to_string())?;
    let text = format!(
        "$ {script}\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(text)
    } else if out.status.code() == Some(127) {
        Err(format!("{text}\nNo se encontró el programa necesario (npm, go, rustup, composer, pipx o mise). Instalalo primero."))
    } else {
        Err(text)
    }
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub lines: usize,
    pub lang: String,
}
#[derive(Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}
#[derive(Serialize)]
pub struct ProjectGraph {
    pub framework: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub truncated: bool,
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    ".quasar",
    ".next",
    ".nuxt",
    "coverage",
    ".output",
    "vendor",
    "src-tauri",
    "__pycache__",
    "venv",
    ".venv",
    "env",
    "site-packages",
    "storage",
    ".idea",
    ".gradle",
];
const EXTS: &[&str] = &[
    "vue", "js", "ts", "jsx", "tsx", "mjs", "cjs", "svelte", "py", "php", "go", "rs", "html",
    "htm", "css", "scss", "kt", "kts", "java", "dart", "gd", "c", "cc", "cpp", "cxx", "h", "hh",
    "hpp", "qml",
];
const JS_EXTS: &[&str] = &["vue", "js", "ts", "jsx", "tsx", "mjs", "cjs", "svelte"];
const MAX_FILES: usize = 2000;

fn lang_of(ext: &str) -> &'static str {
    match ext {
        "vue" | "js" | "jsx" | "mjs" | "cjs" | "svelte" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "py" => "Python",
        "php" => "PHP",
        "go" => "Go",
        "rs" => "Rust",
        "kt" | "kts" => "Kotlin",
        "java" => "Java",
        "dart" => "Dart",
        "gd" => "GDScript",
        "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" => "C++",
        "qml" => "QML",
        "html" | "htm" => "HTML",
        "css" | "scss" => "CSS",
        _ => "Otro",
    }
}

fn kind_of(rel: &str, ext: &str) -> &'static str {
    let l = rel.to_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| l.contains(k));
    if matches!(ext, "css" | "scss") {
        return "asset";
    }
    if ext == "qml" {
        let stem = l.rsplit('/').next().unwrap_or("").trim_end_matches(".qml");
        let big = stem == "main"
            || ["page", "view", "window", "screen", "dialog"]
                .iter()
                .any(|k| stem.contains(k));
        return if big { "page" } else { "component" };
    }
    if matches!(ext, "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp") {
        let has2 = |ks: &[&str]| ks.iter().any(|k| l.contains(k));
        return if has2(&["network", "/api", "client", "service", "http", "rest"]) {
            "service"
        } else if has2(&[
            "model",
            "entity",
            "database",
            "/db",
            "repository",
            "storage",
        ]) {
            "model"
        } else if has2(&["controller", "viewmodel", "presenter", "handler"]) {
            "controller"
        } else if has2(&["/ui/", "widget", "dialog", "window", "view"]) {
            "component"
        } else if has2(&["util", "helper", "common", "/lib/"]) {
            "logic"
        } else {
            "module"
        };
    }
    if has(&["/router", "routes", "urls.py", "web.php", "api.php"]) {
        "router"
    } else if has(&["/controller", "/handler", "/endpoint", "/http/", "views.py"]) {
        "controller"
    } else if has(&[
        "/model",
        "/entit",
        "/schema",
        "/domain/",
        "/data/",
        "/migration",
        "/repositor",
        "/repo/",
        "/db/",
        "models.py",
    ]) {
        "model"
    } else if has(&["/layouts/"]) {
        "layout"
    } else if has(&[
        "/activit",
        "/screens/",
        "/screen/",
        "/pages/",
        "/views/",
        "/templates/",
    ]) || matches!(ext, "html" | "htm")
    {
        "page"
    } else if has(&[
        "/stores/",
        "/store/",
        "/state/",
        "/viewmodel",
        "/bloc/",
        "/provider",
    ]) {
        "store"
    } else if has(&["/service", "/api/", "/client"]) {
        "service"
    } else if has(&["/components/", "/widgets/", "/fragment", "/adapter"]) {
        "component"
    } else if has(&[
        "/composables/",
        "/hooks/",
        "/utils/",
        "/util/",
        "/lib/",
        "/boot/",
        "/helpers/",
        "/middleware",
        "/internal/",
        "/pkg/",
    ]) {
        "logic"
    } else {
        "module"
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            walk(&p, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| EXTS.contains(&x))
            .unwrap_or(false)
            && !name.ends_with(".min.js")
            && !name.ends_with(".min.css")
        {
            out.push(p);
        }
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut norm = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                norm.pop();
            }
            std::path::Component::CurDir => {}
            o => norm.push(o),
        }
    }
    norm
}

struct Index {
    root: PathBuf,
    known: HashMap<PathBuf, String>,
    by_dir: HashMap<PathBuf, Vec<String>>,
    by_stem: HashMap<String, Vec<String>>,
    lower_ids: Vec<(String, String)>,
    go_module: Option<String>,
}

impl Index {
    /// base, base.<ext>, base/index.<ext> for the given extensions.
    fn file(&self, base: &Path, exts: &[&str]) -> Option<String> {
        let base = normalize(base);
        let cands = std::iter::once(base.clone())
            .chain(
                exts.iter()
                    .map(|e| PathBuf::from(format!("{}.{}", base.display(), e))),
            )
            .chain(exts.iter().map(|e| base.join(format!("index.{e}"))));
        for c in cands {
            if let Some(id) = self.known.get(&c) {
                return Some(id.clone());
            }
        }
        None
    }
}

fn resolve_js(spec: &str, from: &Path, ix: &Index) -> Vec<String> {
    let base: PathBuf = if spec.starts_with("./") || spec.starts_with("../") {
        match from.parent() {
            Some(d) => d.join(spec),
            None => return vec![],
        }
    } else if let Some(r) = spec
        .strip_prefix("@/")
        .or_else(|| spec.strip_prefix("~/"))
        .or_else(|| spec.strip_prefix("src/"))
    {
        ix.root.join("src").join(r)
    } else {
        return vec![];
    };
    ix.file(&base, JS_EXTS).into_iter().collect()
}

fn resolve_python(module: &str, from: &Path, ix: &Index) -> Vec<String> {
    let dots = module.chars().take_while(|c| *c == '.').count();
    let rest = module.trim_start_matches('.').replace('.', "/");
    let mut bases: Vec<PathBuf> = Vec::new();
    if dots > 0 {
        let mut d = from.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for _ in 1..dots {
            d.pop();
        }
        bases.push(d.join(&rest));
    } else {
        bases.push(ix.root.join(&rest));
        bases.push(ix.root.join("src").join(&rest));
        if let Some(d) = from.parent() {
            bases.push(d.join(&rest));
        }
    }
    for b in bases {
        if rest.is_empty() {
            continue;
        }
        if let Some(id) = ix.file(&b, &["py"]) {
            return vec![id];
        }
        if let Some(id) = ix.known.get(&normalize(&b.join("__init__.py"))) {
            return vec![id.clone()];
        }
    }
    vec![]
}

fn resolve_php_include(spec: &str, from: &Path, ix: &Index) -> Vec<String> {
    let rel = spec.trim_start_matches('/');
    let mut bases = vec![];
    if let Some(d) = from.parent() {
        bases.push(d.join(rel));
    }
    bases.push(ix.root.join(rel));
    bases
        .into_iter()
        .filter_map(|b| ix.file(&b, &["php"]))
        .take(1)
        .collect()
}

fn resolve_php_use(ns: &str, ix: &Index) -> Vec<String> {
    let path = ns.replace('\\', "/").to_lowercase();
    let stem = path.rsplit('/').next().unwrap_or("").to_string();
    let suffix = format!("{path}.php");
    // namespace folders usually mirror the path (PSR-4), so match by path suffix first
    let hits: Vec<String> = ix
        .lower_ids
        .iter()
        .filter(|(l, _)| l.ends_with(&suffix) || l.trim_start_matches("src/").ends_with(&suffix))
        .map(|(_, id)| id.clone())
        .collect();
    if !hits.is_empty() {
        return hits.into_iter().take(1).collect();
    }
    match ix.by_stem.get(&stem) {
        Some(v) if v.len() == 1 => v.clone(),
        _ => vec![],
    }
}

fn resolve_go(spec: &str, ix: &Index) -> Vec<String> {
    let Some(m) = &ix.go_module else {
        return vec![];
    };
    let rel = if spec == m {
        String::new()
    } else if let Some(r) = spec.strip_prefix(&format!("{m}/")) {
        r.to_string()
    } else {
        return vec![];
    };
    ix.by_dir
        .get(&normalize(&ix.root.join(rel)))
        .map(|v| {
            v.iter()
                .filter(|id| id.ends_with(".go") && !id.ends_with("_test.go"))
                .take(8)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_rust_mod(name: &str, from: &Path, ix: &Index) -> Vec<String> {
    let stem = from.file_stem().and_then(|x| x.to_str()).unwrap_or("");
    let parent = from.parent().unwrap_or(Path::new(""));
    let dir = if matches!(stem, "mod" | "lib" | "main") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    for c in [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
        parent.join(format!("{name}.rs")),
    ] {
        if let Some(id) = ix.known.get(&normalize(&c)) {
            return vec![id.clone()];
        }
    }
    vec![]
}

fn resolve_rust_use(path: &str, from: &Path, ix: &Index) -> Vec<String> {
    let src = from
        .ancestors()
        .find(|a| a.file_name().map(|n| n == "src").unwrap_or(false))
        .map(|a| a.to_path_buf())
        .unwrap_or_else(|| ix.root.join("src"));
    let segs: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
    for n in (1..=segs.len()).rev() {
        let base = src.join(segs[..n].join("/"));
        for c in [
            PathBuf::from(format!("{}.rs", base.display())),
            base.join("mod.rs"),
        ] {
            if let Some(id) = ix.known.get(&normalize(&c)) {
                return vec![id.clone()];
            }
        }
    }
    vec![]
}

/// Kotlin/Java: `import com.app.ui.Home` -> .../com/app/ui/Home.kt|.java (also nested classes).
fn resolve_jvm(fqcn: &str, ix: &Index) -> Vec<String> {
    let mut parts: Vec<&str> = fqcn.split('.').collect();
    while parts.len() > 1 {
        let suffix = format!("{}", parts.join("/")).to_lowercase();
        let stem = parts.last().unwrap().to_lowercase();
        if let Some(cands) = ix.by_stem.get(&stem) {
            let hits: Vec<String> = cands
                .iter()
                .filter(|id| {
                    let l = id.to_lowercase();
                    l.ends_with(&format!("/{suffix}.kt"))
                        || l.ends_with(&format!("/{suffix}.java"))
                        || l == format!("{suffix}.kt")
                        || l == format!("{suffix}.java")
                })
                .cloned()
                .collect();
            if !hits.is_empty() {
                return hits.into_iter().take(1).collect();
            }
        }
        parts.pop(); // `import a.b.Outer.Inner` -> try a.b.Outer
    }
    vec![]
}

/// Dart/Flutter: relative imports and `package:<this app>/x.dart` (-> lib/x.dart).
fn resolve_dart(spec: &str, from: &Path, ix: &Index, pkg: &str) -> Vec<String> {
    if let Some(rest) = spec.strip_prefix("package:") {
        let (name, path) = rest.split_once('/').unwrap_or((rest, ""));
        if name != pkg {
            return vec![];
        }
        return ix
            .known
            .get(&normalize(&ix.root.join("lib").join(path)))
            .cloned()
            .into_iter()
            .collect();
    }
    if spec.starts_with("dart:") || spec.starts_with("http") {
        return vec![];
    }
    from.parent()
        .and_then(|d| ix.known.get(&normalize(&d.join(spec))).cloned())
        .into_iter()
        .collect()
}

/// C/C++: `#include "x.h"` relative to the file, the project root, include/ or src/.
fn resolve_cpp_include(spec: &str, from: &Path, ix: &Index) -> Vec<String> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(d) = from.parent() {
        bases.push(d.join(spec));
    }
    for d in ["", "include", "src", "inc"] {
        bases.push(ix.root.join(d).join(spec));
    }
    for b in bases {
        if let Some(id) = ix.known.get(&normalize(&b)) {
            return vec![id.clone()];
        }
    }
    // last resort: a unique file with that exact name anywhere in the project
    let name = spec.rsplit('/').next().unwrap_or(spec).to_lowercase();
    let stem = name
        .rsplit_once('.')
        .map(|(s, _)| s.to_string())
        .unwrap_or(name.clone());
    match ix.by_stem.get(&stem) {
        Some(v) => {
            let hits: Vec<_> = v
                .iter()
                .filter(|id| {
                    id.to_lowercase().ends_with(&format!("/{name}")) || id.to_lowercase() == name
                })
                .cloned()
                .collect();
            if hits.len() == 1 {
                hits
            } else {
                vec![]
            }
        }
        None => vec![],
    }
}

/// QML: `import "dir"` / `import "x.js"` plus type usage (`Card { }` -> Card.qml in this or an imported folder).
fn resolve_qml(
    src: &str,
    from: &Path,
    ix: &Index,
    re_import: &regex::Regex,
    re_type: &regex::Regex,
) -> Vec<String> {
    let here = from.parent().unwrap_or(Path::new(""));
    let mut dirs: Vec<PathBuf> = vec![here.to_path_buf()];
    let mut out: Vec<String> = Vec::new();
    for c in re_import.captures_iter(src) {
        let spec = &c[1];
        if spec.starts_with("http") {
            continue;
        }
        if let Some(id) = ix.known.get(&normalize(&here.join(spec))) {
            out.push(id.clone());
        } else if here.join(spec).is_dir() || ix.root.join(spec).is_dir() {
            dirs.push(normalize(&here.join(spec)));
            dirs.push(normalize(&ix.root.join(spec)));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for c in re_type.captures_iter(src) {
        let ty = &c[1];
        if !seen.insert(ty.to_string()) {
            continue;
        }
        for d in &dirs {
            if let Some(id) = ix.known.get(&d.join(format!("{ty}.qml"))) {
                out.push(id.clone());
                break;
            }
        }
    }
    out
}

fn resolve_asset(spec: &str, from: &Path, ix: &Index, exts: &[&str]) -> Vec<String> {
    if spec.starts_with("http")
        || spec.starts_with("//")
        || spec.starts_with("data:")
        || spec.starts_with("mailto:")
    {
        return vec![];
    }
    let rel = spec.trim_start_matches('/');
    let mut bases = vec![];
    if let Some(d) = from.parent() {
        bases.push(d.join(rel));
    }
    bases.push(ix.root.join(rel));
    bases.push(ix.root.join("public").join(rel));
    bases
        .into_iter()
        .filter_map(|b| ix.file(&b, exts))
        .take(1)
        .collect()
}

/// Looks at the Python files at the top of a project (and one folder down) for PySide/PyQt and PyGObject (GTK) imports.
fn python_ui_toolkits(root: &Path) -> (bool, bool) {
    let mut files: Vec<PathBuf> = Vec::new();
    let collect = |dir: &Path, out: &mut Vec<PathBuf>| {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "py").unwrap_or(false) {
                    out.push(p);
                }
            }
        }
    };
    collect(root, &mut files);
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten().take(40) {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() && !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                collect(&p, &mut files);
            }
        }
    }
    let (mut qt, mut gtk) = (false, false);
    for f in files.iter().take(60) {
        let t = std::fs::read_to_string(f).unwrap_or_default();
        if t.contains("PySide6")
            || t.contains("PySide2")
            || t.contains("PyQt6")
            || t.contains("PyQt5")
        {
            qt = true;
        }
        if t.contains("gi.require_version") || t.contains("from gi.repository") {
            gtk = true;
        }
    }
    let reqs = std::fs::read_to_string(root.join("requirements.txt"))
        .unwrap_or_default()
        .to_lowercase();
    if reqs.contains("pyside") || reqs.contains("pyqt") {
        qt = true;
    }
    if reqs.contains("pygobject") {
        gtk = true;
    }
    (qt, gtk)
}

fn re(p: &str) -> regex::Regex {
    regex::Regex::new(p).expect("valid regex")
}

/// Reads import relations between a project's source files. Understands
/// JS/TS/Vue/Svelte, Python, PHP, Go, Rust, HTML and CSS/SCSS.
#[tauri::command]
pub fn scan_project_graph(root: String) -> Result<ProjectGraph, String> {
    let root_p = Path::new(&root).canonicalize().map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    walk(&root_p, &mut files);
    let truncated = files.len() >= MAX_FILES;

    let read = |n: &str| std::fs::read_to_string(root_p.join(n)).unwrap_or_default();
    let pkg = read("package.json");
    let mut frameworks: Vec<&str> = Vec::new();
    for (needle, name) in [
        ("\"quasar\"", "Quasar"),
        ("\"next\"", "Next.js"),
        ("\"nuxt\"", "Nuxt"),
        ("\"react\"", "React"),
        ("\"vue\"", "Vue"),
        ("\"svelte\"", "Svelte"),
        ("\"express\"", "Express"),
    ] {
        if pkg.contains(needle) {
            frameworks.push(name);
            break;
        }
    }
    let composer = read("composer.json");
    if composer.contains("laravel/framework") {
        frameworks.push("Laravel")
    } else if composer.contains("symfony/") {
        frameworks.push("Symfony")
    } else if !composer.is_empty() {
        frameworks.push("PHP")
    }
    let cmake = read("CMakeLists.txt");
    if root_p.join("project.godot").exists() {
        frameworks.push("Godot")
    }
    let pubspec = read("pubspec.yaml");
    let dart_pkg = pubspec
        .lines()
        .find_map(|l| {
            l.strip_prefix("name:")
                .map(|n| n.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        })
        .unwrap_or_default();
    if pubspec.contains("flutter") {
        frameworks.push("Flutter")
    } else if !pubspec.is_empty() {
        frameworks.push("Dart")
    }
    let has_any = |names: &[&str]| names.iter().any(|n| root_p.join(n).exists());
    if has_any(&[
        "capacitor.config.ts",
        "capacitor.config.json",
        "capacitor.config.js",
    ]) || pkg.contains("@capacitor/core")
    {
        frameworks.push("Capacitor")
    }
    if pkg.contains("\"react-native\"") {
        frameworks.push("React Native")
    }
    if has_any(&[
        "app/build.gradle",
        "app/build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]) && has_any(&["gradlew", "app/src/main/AndroidManifest.xml"])
    {
        frameworks.push("Android nativo")
    }
    if cmake.contains("Qt")
        || std::fs::read_dir(&root_p)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().extension().map(|x| x == "pro").unwrap_or(false))
            })
            .unwrap_or(false)
    {
        frameworks.push("Qt")
    }
    let (py_qt, py_gtk) = python_ui_toolkits(&root_p);
    if py_qt {
        frameworks.push("PySide/PyQt")
    }
    let meson = read("meson.build");
    if py_gtk
        || meson.to_lowercase().contains("gtk")
        || cmake.to_lowercase().contains("gtk")
        || read("Cargo.toml").contains("gtk4")
    {
        frameworks.push("GTK")
    }
    let go_mod = read("go.mod");
    if !go_mod.is_empty() {
        frameworks.push("Go");
    }
    if root_p.join("Cargo.toml").exists() {
        frameworks.push("Rust");
    }
    let py_deps = format!(
        "{}{}{}",
        read("requirements.txt"),
        read("pyproject.toml"),
        read("Pipfile")
    )
    .to_lowercase();
    if py_deps.contains("django") {
        frameworks.push("Django")
    } else if py_deps.contains("fastapi") {
        frameworks.push("FastAPI")
    } else if py_deps.contains("flask") {
        frameworks.push("Flask")
    } else if !py_deps.is_empty() {
        frameworks.push("Python")
    }

    let known: HashMap<PathBuf, String> = files
        .iter()
        .filter_map(|p| {
            p.strip_prefix(&root_p)
                .ok()
                .map(|r| (p.clone(), r.to_string_lossy().replace('\\', "/")))
        })
        .collect();
    let mut by_dir: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
    for (p, id) in &known {
        if let Some(d) = p.parent() {
            by_dir.entry(d.to_path_buf()).or_default().push(id.clone());
        }
        if let Some(st) = p.file_stem().and_then(|x| x.to_str()) {
            by_stem
                .entry(st.to_lowercase())
                .or_default()
                .push(id.clone());
        }
    }
    for v in by_dir.values_mut() {
        v.sort();
    }
    let lower_ids = known
        .values()
        .map(|id| (id.to_lowercase(), id.clone()))
        .collect();
    let go_module = go_mod.lines().find_map(|l| {
        l.trim()
            .strip_prefix("module ")
            .map(|m| m.trim().to_string())
    });
    let ix = Index {
        root: root_p.clone(),
        known,
        by_dir,
        by_stem,
        lower_ids,
        go_module,
    };

    let re_js = re(
        r#"(?m)(?:import\s+(?:[^'"]*?\s+from\s+)?|export\s+[^'"]*?\s+from\s+|require\(\s*|import\(\s*)['"]([^'"]+)['"]"#,
    );
    let re_py_from = re(r"(?m)^\s*from\s+([.\w]+)\s+import\s+([\w,\s*]+)");
    let re_py_imp = re(r"(?m)^\s*import\s+([\w.]+(?:\s*,\s*[\w.]+)*)");
    let re_php_inc =
        re(r#"(?:require|include)(?:_once)?\s*\(?\s*(?:__DIR__\s*\.\s*)?['"]([^'"]+)['"]"#);
    let re_php_use = re(r"(?m)^\s*use\s+([\w\\]+)(?:\s+as\s+\w+)?\s*;");
    let re_go_block = re(r"(?s)import\s*\((.*?)\)");
    let re_go_one = re(r#"(?m)^\s*import\s+(?:\w+\s+)?"([^"]+)""#);
    let re_quoted = re(r#""([^"]+)""#);
    let re_rs_mod = re(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;");
    let re_rs_use = re(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+crate::([\w:]+)");
    let re_html = re(r#"(?i)(?:src|href)\s*=\s*["']([^"'#?]+)["']"#);
    let re_jvm = re(r"(?m)^\s*import\s+(?:static\s+)?([\w.]+?)(?:\.\*)?\s*;?\s*$");
    let re_dart = re(r#"(?m)^\s*(?:import|export|part)\s+['"]([^'"]+)['"]"#);
    let re_cpp_inc = re(r#"(?m)^\s*#\s*include\s+"([^"]+)""#);
    let re_qml_import = re(r#"(?m)^\s*import\s+"([^"]+)""#);
    let re_qml_type = re(r"\b([A-Z][A-Za-z0-9_]*)\s*\{");
    let re_css = re(r#"@import\s+(?:url\()?['"]([^'"]+)['"]"#);

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut lang_count: HashMap<&str, usize> = HashMap::new();
    for p in &files {
        let Some(id) = ix.known.get(p) else { continue };
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        let src = std::fs::read_to_string(p).unwrap_or_default();
        let lang = lang_of(ext);
        *lang_count.entry(lang).or_default() += 1;
        nodes.push(GraphNode {
            id: id.clone(),
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            kind: kind_of(id, ext).to_string(),
            lines: src.lines().count(),
            lang: lang.to_string(),
        });

        let mut targets: Vec<String> = Vec::new();
        match ext {
            e if JS_EXTS.contains(&e) => {
                for c in re_js.captures_iter(&src) {
                    targets.extend(resolve_js(&c[1], p, &ix));
                }
            }
            "py" => {
                for c in re_py_from.captures_iter(&src) {
                    let found = resolve_python(&c[1], p, &ix);
                    if !found.is_empty() {
                        targets.extend(found);
                    }
                    // `from pkg import module` may name a submodule file
                    for name in c[2]
                        .split(',')
                        .map(|n| n.trim())
                        .filter(|n| !n.is_empty() && *n != "*")
                    {
                        targets.extend(resolve_python(
                            &format!("{}.{}", c[1].trim_end_matches('.'), name),
                            p,
                            &ix,
                        ));
                    }
                }
                for c in re_py_imp.captures_iter(&src) {
                    for m in c[1].split(',') {
                        targets.extend(resolve_python(m.trim(), p, &ix));
                    }
                }
            }
            "php" => {
                for c in re_php_inc.captures_iter(&src) {
                    targets.extend(resolve_php_include(&c[1], p, &ix));
                }
                for c in re_php_use.captures_iter(&src) {
                    targets.extend(resolve_php_use(&c[1], &ix));
                }
            }
            "go" => {
                for c in re_go_block.captures_iter(&src) {
                    for q in re_quoted.captures_iter(&c[1]) {
                        targets.extend(resolve_go(&q[1], &ix));
                    }
                }
                for c in re_go_one.captures_iter(&src) {
                    targets.extend(resolve_go(&c[1], &ix));
                }
            }
            "rs" => {
                for c in re_rs_mod.captures_iter(&src) {
                    targets.extend(resolve_rust_mod(&c[1], p, &ix));
                }
                for c in re_rs_use.captures_iter(&src) {
                    targets.extend(resolve_rust_use(&c[1], p, &ix));
                }
            }
            "kt" | "kts" | "java" => {
                for c in re_jvm.captures_iter(&src) {
                    targets.extend(resolve_jvm(&c[1], &ix));
                }
            }
            "dart" => {
                for c in re_dart.captures_iter(&src) {
                    targets.extend(resolve_dart(&c[1], p, &ix, &dart_pkg));
                }
            }
            "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" => {
                for c in re_cpp_inc.captures_iter(&src) {
                    targets.extend(resolve_cpp_include(&c[1], p, &ix));
                }
            }
            "qml" => targets.extend(resolve_qml(&src, p, &ix, &re_qml_import, &re_qml_type)),
            "html" | "htm" => {
                for c in re_html.captures_iter(&src) {
                    targets.extend(resolve_asset(
                        &c[1],
                        p,
                        &ix,
                        &["html", "htm", "css", "js", "ts", "mjs"],
                    ));
                }
            }
            "css" | "scss" => {
                for c in re_css.captures_iter(&src) {
                    let spec = c[1].to_string();
                    let mut found = resolve_asset(&spec, p, &ix, &["css", "scss"]);
                    if found.is_empty() {
                        // scss partials: @import "base" -> _base.scss
                        let (dir, name) = spec.rsplit_once('/').unwrap_or(("", &spec));
                        found = resolve_asset(
                            &format!("{}{}_{}", dir, if dir.is_empty() { "" } else { "/" }, name),
                            p,
                            &ix,
                            &["scss"],
                        );
                    }
                    targets.extend(found);
                }
            }
            _ => {}
        }
        let mut seen = std::collections::HashSet::new();
        for to in targets {
            if to != *id && seen.insert(to.clone()) {
                edges.push(GraphEdge {
                    from: id.clone(),
                    to,
                });
            }
        }
    }

    let mut framework: Vec<String> = frameworks.iter().map(|s| s.to_string()).collect();
    if framework.is_empty() {
        let mut langs: Vec<_> = lang_count.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        framework = langs
            .into_iter()
            .take(3)
            .map(|(l, _)| l.to_string())
            .collect();
    }
    Ok(ProjectGraph {
        framework: framework.join(" · "),
        nodes,
        edges,
        truncated,
    })
}

#[derive(Serialize)]
pub struct MobileCmd {
    pub label: String,
    pub cmd: String,
}
#[derive(Serialize)]
pub struct MobileKind {
    pub kind: String,
    pub label: String,
    pub commands: Vec<MobileCmd>,
}
#[derive(Serialize)]
pub struct MobileInfo {
    pub kinds: Vec<MobileKind>,
    pub tools: HashMap<String, Option<String>>,
}

fn mc(label: &str, cmd: &str) -> MobileCmd {
    MobileCmd {
        label: label.into(),
        cmd: cmd.into(),
    }
}

/// Detects how a project targets Android/mobile (Capacitor, Flutter, native Gradle,
/// React Native, Quasar-Capacitor, Tauri mobile, Qt) and lists ready-to-run commands,
/// plus which SDK tools are installed on this machine.
#[tauri::command]
pub async fn detect_mobile_project(root: String) -> Result<MobileInfo, String> {
    let r = Path::new(&root);
    let read = |n: &str| std::fs::read_to_string(r.join(n)).unwrap_or_default();
    let has = |n: &str| r.join(n).exists();
    let pkg = read("package.json");
    let mut kinds = Vec::new();

    if has("capacitor.config.ts")
        || has("capacitor.config.json")
        || has("capacitor.config.js")
        || pkg.contains("@capacitor/core")
    {
        let mut cmds = vec![];
        if has("src-capacitor") {
            // Quasar's Capacitor mode
            cmds.push(mc(
                "Desarrollo en Android (Quasar)",
                "npx quasar dev -m capacitor -T android",
            ));
            cmds.push(mc(
                "Compilar APK/AAB (Quasar)",
                "npx quasar build -m capacitor -T android",
            ));
        }
        cmds.extend([
            mc("Compilar la web", "npm run build"),
            mc("Sincronizar con Android", "npx cap sync android"),
            mc("Ejecutar en dispositivo/emulador", "npx cap run android"),
            mc("Abrir en Android Studio", "npx cap open android"),
            mc("Diagnóstico de Capacitor", "npx cap doctor"),
        ]);
        if !has("android") {
            cmds.insert(
                0,
                mc(
                    "Agregar plataforma Android",
                    "npm i @capacitor/android && npx cap add android",
                ),
            );
        }
        kinds.push(MobileKind {
            kind: "capacitor".into(),
            label: "Capacitor".into(),
            commands: cmds,
        });
    }
    if read("pubspec.yaml").contains("flutter") {
        kinds.push(MobileKind {
            kind: "flutter".into(),
            label: "Flutter".into(),
            commands: vec![
                mc("Dependencias", "flutter pub get"),
                mc("Ejecutar", "flutter run"),
                mc("Dispositivos", "flutter devices"),
                mc("Compilar APK", "flutter build apk --release"),
                mc("Compilar AAB (Play Store)", "flutter build appbundle"),
                mc("Analizar código", "flutter analyze"),
                mc("Diagnóstico", "flutter doctor -v"),
            ],
        });
    }
    if pkg.contains("\"react-native\"") {
        kinds.push(MobileKind {
            kind: "rn".into(),
            label: "React Native".into(),
            commands: vec![
                mc("Ejecutar en Android", "npx react-native run-android"),
                mc("Metro (bundler)", "npx react-native start"),
                mc("Expo", "npx expo start"),
            ],
        });
    }
    if has("src-tauri/gen/android") {
        kinds.push(MobileKind {
            kind: "tauri".into(),
            label: "Tauri móvil".into(),
            commands: vec![
                mc("Desarrollo en Android", "npx tauri android dev"),
                mc("Compilar APK", "npx tauri android build"),
            ],
        });
    }
    if (has("gradlew") || has("settings.gradle") || has("settings.gradle.kts"))
        && (has("app") || has("src/main/AndroidManifest.xml"))
    {
        kinds.push(MobileKind {
            kind: "android".into(),
            label: "Android nativo (Kotlin/Java)".into(),
            commands: vec![
                mc("Compilar debug", "./gradlew assembleDebug"),
                mc("Instalar en dispositivo", "./gradlew installDebug"),
                mc("Tests unitarios", "./gradlew test"),
                mc("Lint", "./gradlew lint"),
                mc("Limpiar", "./gradlew clean"),
                mc("Compilar release (AAB)", "./gradlew bundleRelease"),
                mc("Dispositivos", "adb devices"),
                mc("Logs (errores)", "adb logcat *:E"),
            ],
        });
    }
    if has("project.godot") {
        kinds.push(MobileKind {
            kind: "godot".into(),
            label: "Godot".into(),
            commands: vec![
                mc("Ejecutar el juego", "godot --path ."),
                mc("Abrir en el editor de Godot", "godot --editor --path ."),
                mc(
                    "Revisar scripts (sin ventana)",
                    "godot --headless --path . --quit",
                ),
                mc(
                    "Exportar (necesita presets en Godot)",
                    "godot --headless --path . --export-release \"Linux\" build/juego.x86_64",
                ),
                mc("Versión de Godot", "godot --version"),
            ],
        });
    }
    let (py_qt, py_gtk) = python_ui_toolkits(r);
    let main_py = ["main.py", "app.py", "src/main.py", "run.py"]
        .iter()
        .find(|f| has(f))
        .map(|f| f.to_string());
    let run_py = main_py.clone().unwrap_or_else(|| "main.py".into());
    if py_qt {
        kinds.push(MobileKind {
            kind: "pyside".into(),
            label: "Python + Qt (PySide/PyQt)".into(),
            commands: vec![
                mc("Ejecutar", &format!("python3 {run_py}")),
                mc("Instalar PySide6", "pip install PySide6   # o: pipx / venv"),
                mc("Abrir Qt Designer", "pyside6-designer"),
                mc(
                    "Convertir .ui a Python",
                    "pyside6-uic ui/interfaz.ui -o ui_interfaz.py",
                ),
                mc(
                    "Compilar recursos (.qrc)",
                    "pyside6-rcc resources.qrc -o resources_rc.py",
                ),
                mc("Empaquetar la app", "pyside6-deploy "),
            ],
        });
    }
    if py_gtk
        || read("meson.build").to_lowercase().contains("gtk")
        || read("Cargo.toml").contains("gtk4")
    {
        let mut cmds = vec![];
        if py_gtk {
            cmds.push(mc("Ejecutar (PyGObject)", &format!("python3 {run_py}")));
            cmds.push(mc(
                "Instalar GTK4 para Python (Ubuntu)",
                "sudo apt install python3-gi python3-gi-cairo gir1.2-gtk-4.0 gir1.2-adw-1",
            ));
        }
        if has("meson.build") {
            cmds.push(mc("Configurar (meson)", "meson setup build"));
            cmds.push(mc("Compilar", "meson compile -C build"));
            cmds.push(mc("Tests", "meson test -C build"));
        }
        if read("Cargo.toml").contains("gtk4") {
            cmds.push(mc("Ejecutar (gtk4-rs)", "cargo run"));
        }
        cmds.push(mc(
            "Probar un .ui con el visor",
            "gtk4-builder-tool validate interfaz.ui",
        ));
        cmds.push(mc("Abrir Glade / Cambalache", "cambalache"));
        kinds.push(MobileKind {
            kind: "gtk".into(),
            label: "GTK".into(),
            commands: cmds,
        });
    }
    let cmake = read("CMakeLists.txt");
    let pro_file = std::fs::read_dir(r).ok().and_then(|rd| {
        rd.flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "pro").unwrap_or(false))
    });
    if cmake.contains("Qt") || pro_file.is_some() {
        let name_re =
            regex::Regex::new(r"(?i)(?:qt_add_executable|add_executable)\s*\(\s*([\w.-]+)")
                .unwrap();
        let target = name_re
            .captures(&cmake)
            .map(|c| c[1].to_string())
            .or_else(|| {
                pro_file
                    .as_ref()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|t| {
                        t.lines().find_map(|l| {
                            l.trim()
                                .strip_prefix("TARGET")
                                .and_then(|x| x.split('=').nth(1))
                                .map(|v| v.trim().to_string())
                        })
                    })
            })
            .or_else(|| r.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "app".into());
        let mut cmds = vec![];
        if cmake.contains("Qt") {
            cmds.push(mc(
                "Configurar (Debug + compile_commands para clangd)",
                "cmake -B build -S . -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
            ));
            cmds.push(mc("Compilar", "cmake --build build -j"));
            cmds.push(mc(
                &format!("Compilar y ejecutar {target}"),
                &format!("cmake --build build -j && ./build/{target}"),
            ));
            cmds.push(mc(
                &format!("Ejecutar {target}"),
                &format!("./build/{target}"),
            ));
            cmds.push(mc(
                "Tests (ctest)",
                "ctest --test-dir build --output-on-failure",
            ));
            cmds.push(mc("Limpiar carpeta build", "rm -rf build"));
            cmds.push(mc(
                "Configurar para Android (Qt for Android + NDK)",
                "qt-cmake -B build-android -S . -DQT_ANDROID_BUILD_ALL_ABIS=OFF",
            ));
        } else {
            cmds.push(mc("qmake + compilar", "qmake && make -j"));
            cmds.push(mc(&format!("Ejecutar {target}"), &format!("./{target}")));
        }
        cmds.push(mc(
            "Empaquetar en Windows (Qt Deploy)",
            &format!("windeployqt build/{target}.exe"),
        ));
        kinds.push(MobileKind {
            kind: "qt".into(),
            label: "Qt / QML (escritorio)".into(),
            commands: cmds,
        });
    }

    let mut tools = HashMap::new();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    for t in [
        "adb",
        "java",
        "gradle",
        "flutter",
        "dart",
        "node",
        "npx",
        "cmake",
        "ninja",
        "qmake",
        "qmake6",
        "qt-cmake",
        "qmlls",
        "clangd",
        "windeployqt",
        "meson",
        "pyside6-designer",
        "pyside6-uic",
        "valac",
        "blueprint-compiler",
        "gtk4-builder-tool",
        "python3",
        "pipx",
        "godot",
    ] {
        tools.insert(t.to_string(), crate::util::find_binary(t).await);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = &mut tools;
    }
    tools.insert(
        "ANDROID_HOME".into(),
        std::env::var("ANDROID_HOME")
            .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
            .ok()
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| format!("{h}/Android/Sdk"))
                    .filter(|p| Path::new(p).exists())
            }),
    );
    Ok(MobileInfo { kinds, tools })
}

#[derive(Serialize)]
pub struct FlowRunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// Runs one flow step: a shell command (any language) inside the project, with
/// the current flow context as JSON on stdin and in $OWEE_INPUT. The cwd is
/// confined to the project root.
#[tauri::command]
pub async fn flow_run_command(
    root: String,
    cwd: Option<String>,
    command: String,
    input: String,
    timeout_secs: Option<u64>,
) -> Result<FlowRunResult, String> {
    use tokio::io::AsyncWriteExt;
    let cwd_path = crate::agent::confine(&root, cwd.as_deref().unwrap_or("."))?;
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&command);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.arg("-c").arg(&command);
        c
    };
    cmd.current_dir(&cwd_path)
        .env("OWEE_INPUT", &input)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = crate::util::resolve_login_shell_path().await {
        cmd.env("PATH", p.trim());
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes()).await;
    }
    let limit = std::time::Duration::from_secs(timeout_secs.unwrap_or(120).clamp(1, 900));
    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(o)) => Ok(FlowRunResult {
            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
            stderr: String::from_utf8_lossy(&o.stderr).to_string(),
            exit_code: o.status.code().unwrap_or(-1),
            timed_out: false,
        }),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Ok(FlowRunResult {
            stdout: String::new(),
            stderr: format!("Cancelado: superó {}s", limit.as_secs()),
            exit_code: -1,
            timed_out: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scans_vue_project() {
        let g = super::scan_project_graph("../src".to_string()).unwrap();
        println!(
            "fw={} nodes={} edges={}",
            g.framework,
            g.nodes.len(),
            g.edges.len()
        );
        assert!(g.nodes.len() > 20);
        assert!(g.edges.len() > 20, "imports should resolve to edges");
    }

    #[test]
    fn scans_rust_project() {
        let g = super::scan_project_graph(".".to_string()).unwrap();
        println!(
            "rust fw={} nodes={} edges={}",
            g.framework,
            g.nodes.len(),
            g.edges.len()
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.from == "src/lib.rs" || e.from.ends_with("agent.rs")),
            "mod/use edges expected"
        );
    }
}

#[cfg(test)]
mod adhoc {
    #[test]
    fn scan_env() {
        if let Ok(p) = std::env::var("OWEE_SCAN") {
            let g = super::scan_project_graph(p).unwrap();
            let mut langs: std::collections::BTreeMap<String, usize> = Default::default();
            for n in &g.nodes {
                *langs.entry(n.lang.clone()).or_default() += 1;
            }
            println!(
                "RESULT fw={} nodes={} edges={} langs={:?}",
                g.framework,
                g.nodes.len(),
                g.edges.len(),
                langs
            );
        }
    }
}

#[cfg(test)]
mod run_tests {
    #[tokio::test]
    async fn runs_python_and_node_steps_with_stdin_json() {
        let dir = std::env::temp_dir().join("owee_flow_test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("s.py"),
            "import sys,json\nd=json.load(sys.stdin)\nprint(json.dumps({'total': d['n']*2}))\n",
        )
        .unwrap();
        let r = super::flow_run_command(
            dir.to_string_lossy().to_string(),
            None,
            "python3 s.py".into(),
            "{\"n\":21}".into(),
            Some(20),
        )
        .await
        .unwrap();
        println!("PY: {} / {}", r.stdout.trim(), r.exit_code);
        assert_eq!(r.exit_code, 0);
        assert!(r.stdout.contains("42"));
        // cwd escaping the project must be rejected
        let bad = super::flow_run_command(
            dir.to_string_lossy().to_string(),
            Some("../..".into()),
            "ls".into(),
            "{}".into(),
            Some(5),
        )
        .await;
        assert!(bad.is_err(), "cwd outside project must be rejected");
    }
}

#[cfg(test)]
mod mobile_tests {
    use std::fs;
    fn w(root: &std::path::Path, rel: &str, c: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, c).unwrap();
    }

    #[tokio::test]
    async fn android_flutter_and_capacitor_projects() {
        let base = std::env::temp_dir().join("owee_mobile_test");
        let _ = fs::remove_dir_all(&base);
        // native Android (Kotlin)
        let a = base.join("android_app");
        w(&a, "settings.gradle.kts", "include(\":app\")");
        w(&a, "gradlew", "#!/bin/sh");
        w(&a, "app/src/main/AndroidManifest.xml", "<manifest/>");
        w(&a, "app/src/main/java/com/demo/ui/HomeActivity.kt", "package com.demo.ui\nimport com.demo.data.UserRepository\nimport com.demo.ui.widgets.Card\nclass HomeActivity");
        w(
            &a,
            "app/src/main/java/com/demo/data/UserRepository.kt",
            "package com.demo.data\nclass UserRepository",
        );
        w(
            &a,
            "app/src/main/java/com/demo/ui/widgets/Card.java",
            "package com.demo.ui.widgets;\npublic class Card {}",
        );
        let g = super::scan_project_graph(a.to_string_lossy().to_string()).unwrap();
        println!(
            "ANDROID fw={} nodes={} edges={:?}",
            g.framework,
            g.nodes.len(),
            g.edges
                .iter()
                .map(|e| format!(
                    "{}->{}",
                    e.from.rsplit('/').next().unwrap(),
                    e.to.rsplit('/').next().unwrap()
                ))
                .collect::<Vec<_>>()
        );
        assert!(g.framework.contains("Android"));
        assert_eq!(g.edges.len(), 2);
        let m = super::detect_mobile_project(a.to_string_lossy().to_string())
            .await
            .unwrap();
        assert!(m.kinds.iter().any(|k| k.kind == "android"));
        // Flutter
        let f = base.join("flutter_app");
        w(
            &f,
            "pubspec.yaml",
            "name: miapp\ndependencies:\n  flutter:\n    sdk: flutter",
        );
        w(
            &f,
            "lib/main.dart",
            "import 'package:miapp/screens/home.dart';\nimport 'util.dart';",
        );
        w(&f, "lib/screens/home.dart", "");
        w(&f, "lib/util.dart", "");
        let g = super::scan_project_graph(f.to_string_lossy().to_string()).unwrap();
        println!("FLUTTER fw={} edges={}", g.framework, g.edges.len());
        assert!(g.framework.contains("Flutter"));
        assert_eq!(g.edges.len(), 2);
        // Capacitor + Quasar
        let c = base.join("cap_app");
        w(
            &c,
            "package.json",
            "{\"dependencies\":{\"@capacitor/core\":\"6\",\"quasar\":\"2\"}}",
        );
        w(&c, "capacitor.config.ts", "");
        w(&c, "src-capacitor/x", "");
        let m = super::detect_mobile_project(c.to_string_lossy().to_string())
            .await
            .unwrap();
        let cap = m
            .kinds
            .iter()
            .find(|k| k.kind == "capacitor")
            .expect("capacitor detected");
        println!(
            "CAP cmds={:?}",
            cap.commands
                .iter()
                .map(|c| c.cmd.clone())
                .collect::<Vec<_>>()
        );
        assert!(cap
            .commands
            .iter()
            .any(|c| c.cmd.contains("quasar dev -m capacitor")));
        println!("TOOLS {:?}", m.tools);
    }

    #[tokio::test]
    async fn qt_desktop_project() {
        let q = std::env::temp_dir().join("owee_qt_test");
        let _ = fs::remove_dir_all(&q);
        w(&q, "CMakeLists.txt", "cmake_minimum_required(VERSION 3.20)\nfind_package(Qt6 REQUIRED COMPONENTS Quick Network)\nqt_add_executable(clienteapp main.cpp)");
        w(
            &q,
            "main.cpp",
            "#include \"src/network/apiclient.h\"\n#include <QGuiApplication>\nint main(){}",
        );
        w(
            &q,
            "src/network/apiclient.h",
            "#pragma once\n#include \"models/pedido.h\"",
        );
        w(&q, "src/network/apiclient.cpp", "#include \"apiclient.h\"");
        w(&q, "src/models/pedido.h", "#pragma once");
        w(
            &q,
            "qml/Main.qml",
            "import \"components\"\nimport \"logic.js\" as L\nWindow { PedidoCard { } Button { } }",
        );
        w(&q, "qml/components/PedidoCard.qml", "Rectangle { }");
        w(&q, "qml/logic.js", "");
        let g = super::scan_project_graph(q.to_string_lossy().to_string()).unwrap();
        let mut e: Vec<String> = g
            .edges
            .iter()
            .map(|e| {
                format!(
                    "{}->{}",
                    e.from.rsplit('/').next().unwrap(),
                    e.to.rsplit('/').next().unwrap()
                )
            })
            .collect();
        e.sort();
        println!("QT fw={} edges={:?}", g.framework, e);
        assert!(g.framework.contains("Qt"));
        for want in [
            "main.cpp->apiclient.h",
            "apiclient.cpp->apiclient.h",
            "apiclient.h->pedido.h",
            "Main.qml->PedidoCard.qml",
            "Main.qml->logic.js",
        ] {
            assert!(e.contains(&want.to_string()), "missing {want}");
        }
        let m = super::detect_mobile_project(q.to_string_lossy().to_string())
            .await
            .unwrap();
        let k = m.kinds.iter().find(|k| k.kind == "qt").unwrap();
        println!(
            "QTCMDS {:?}",
            k.commands.iter().map(|c| c.cmd.clone()).collect::<Vec<_>>()
        );
        assert!(k.commands.iter().any(|c| c.cmd == "./build/clienteapp"));
    }
}

// ── Project doctor: what the detected project types need, and what this machine has ──
#[derive(Serialize)]
pub struct DoctorItem {
    pub group: String,
    pub name: String,
    pub ok: bool,
    pub optional: bool,
    pub detail: String,
    pub hint: String,
}

async fn tool_version(bin: &str) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let path = crate::util::resolve_login_shell_path().await?;
        let full = crate::util::find_binary(bin).await?;
        let mut cmd = tokio::process::Command::new(&full);
        cmd.arg("--version")
            .env("PATH", path)
            .stdin(std::process::Stdio::null());
        let out = tokio::time::timeout(std::time::Duration::from_secs(4), cmd.output())
            .await
            .ok()?
            .ok()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let line = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        let line = if line.to_lowercase().starts_with("usage") {
            String::new()
        } else {
            line
        }; // tools without --version
        return Some(if line.is_empty() {
            full
        } else {
            format!("{line}  ·  {full}")
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = bin;
        None
    }
}

async fn py_check(code: &str) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let py = crate::util::find_binary("python3").await?;
        let path = crate::util::resolve_login_shell_path().await?;
        let out = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            tokio::process::Command::new(py)
                .args(["-c", code])
                .env("PATH", path)
                .stdin(std::process::Stdio::null())
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        if out.status.success() {
            return Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
        }
    }
    let _ = code;
    None
}

/// Checks the tools each detected project type needs and gives an install hint for what is missing.
#[tauri::command]
pub async fn project_doctor(root: String) -> Result<Vec<DoctorItem>, String> {
    let info = detect_mobile_project(root.clone()).await?;
    let kinds: Vec<String> = info.kinds.iter().map(|k| k.kind.clone()).collect();
    let has = |k: &str| kinds.iter().any(|x| x == k);
    let apt = |pkg: &str| format!("sudo apt install {pkg}");
    // (group, tool binary, display name, optional, hint)
    let mut want: Vec<(&str, &str, &str, bool, String)> = Vec::new();
    if has("capacitor") || has("rn") {
        want.push((
            "Web / Capacitor",
            "node",
            "Node.js",
            false,
            "mise use -g node@latest   (o https://nodejs.org)".into(),
        ));
        want.push((
            "Web / Capacitor",
            "npx",
            "npm / npx",
            false,
            "Viene con Node.js".into(),
        ));
    }
    if has("capacitor") || has("rn") || has("android") || has("flutter") || has("tauri") {
        want.push((
            "Android",
            "java",
            "Java (JDK 17+)",
            false,
            apt("openjdk-17-jdk"),
        ));
        want.push((
            "Android",
            "adb",
            "adb (Android platform-tools)",
            false,
            apt("adb"),
        ));
    }
    if has("android") {
        want.push((
            "Android",
            "gradle",
            "Gradle",
            true,
            "El proyecto trae ./gradlew; no hace falta instalarlo".into(),
        ));
    }
    if has("flutter") {
        want.push(("Flutter", "flutter", "Flutter SDK", false, "https://docs.flutter.dev/get-started/install   (o: sudo snap install flutter --classic)".into()));
        want.push(("Flutter", "dart", "Dart", true, "Viene con Flutter".into()));
    }
    if has("qt") {
        want.push(("Qt (C++)", "cmake", "CMake", false, apt("cmake")));
        want.push((
            "Qt (C++)",
            "g++",
            "Compilador C++ (g++)",
            false,
            apt("build-essential"),
        ));
        want.push(("Qt (C++)", "ninja", "Ninja", true, apt("ninja-build")));
        want.push(("Qt (C++)", "qt-cmake", "Qt 6 (qt-cmake)", true, "Instalá Qt con el Qt Online Installer (https://www.qt.io/download); OweeCode lo busca en ~/Qt".into()));
        want.push((
            "Qt (C++)",
            "clangd",
            "clangd (autocompletado C++)",
            true,
            apt("clangd"),
        ));
        want.push((
            "Qt (C++)",
            "qmlls",
            "qmlls (autocompletado QML)",
            true,
            "Viene con Qt 6.3+ (~/Qt/<versión>/<kit>/bin)".into(),
        ));
    }
    if has("pyside") || has("gtk") {
        want.push((
            "Python",
            "python3",
            "Python 3",
            false,
            apt("python3 python3-venv python3-pip"),
        ));
    }
    if has("godot") {
        want.push((
            "Godot",
            "godot",
            "Godot",
            false,
            "https://godotengine.org/download   (OweeCode lo busca en ~/.local/bin)".into(),
        ));
    }
    if has("gtk") {
        want.push(("GTK", "meson", "Meson", true, apt("meson")));
        want.push((
            "GTK",
            "gtk4-builder-tool",
            "gtk4-builder-tool",
            true,
            apt("libgtk-4-bin"),
        ));
    }
    if has("pyside") {
        want.push((
            "Python + Qt",
            "pyside6-designer",
            "Qt Designer (PySide6)",
            true,
            "pip install PySide6   (dentro de un entorno: python3 -m venv .venv)".into(),
        ));
    }

    let mut out: Vec<DoctorItem> = Vec::new();
    for (group, bin, name, optional, hint) in want {
        let v = tool_version(bin).await;
        out.push(DoctorItem {
            group: group.into(),
            name: name.into(),
            ok: v.is_some(),
            optional,
            detail: v.unwrap_or_default(),
            hint,
        });
    }
    // environment variables / libraries that are not binaries
    if has("capacitor") || has("android") || has("flutter") || has("rn") || has("tauri") {
        let sdk = info.tools.get("ANDROID_HOME").cloned().flatten();
        out.push(DoctorItem {
            group: "Android".into(),
            name: "Android SDK".into(),
            ok: sdk.is_some(),
            optional: false,
            detail: sdk.unwrap_or_default(),
            hint: "Instalá Android Studio (descarga el SDK) y definí ANDROID_HOME=~/Android/Sdk"
                .into(),
        });
    }
    if has("pyside") {
        let v = py_check("import PySide6; print(PySide6.__version__)").await;
        out.push(DoctorItem {
            group: "Python + Qt".into(),
            name: "PySide6".into(),
            ok: v.is_some(),
            optional: false,
            detail: v.unwrap_or_default(),
            hint: "python3 -m venv .venv && .venv/bin/pip install PySide6".into(),
        });
    }
    if has("gtk") {
        let v = py_check("import gi; gi.require_version('Gtk','4.0'); from gi.repository import Gtk; print('GTK %d.%d.%d' % (Gtk.get_major_version(), Gtk.get_minor_version(), Gtk.get_micro_version()))").await;
        out.push(DoctorItem {
            group: "GTK".into(),
            name: "GTK 4 para Python (PyGObject)".into(),
            ok: v.is_some(),
            optional: false,
            detail: v.unwrap_or_default(),
            hint: apt("python3-gi python3-gi-cairo gir1.2-gtk-4.0"),
        });
        let c = py_check("import cairo; print(cairo.version)").await;
        out.push(DoctorItem {
            group: "GTK".into(),
            name: "pycairo (dibujar en GTK)".into(),
            ok: c.is_some(),
            optional: true,
            detail: c.unwrap_or_default(),
            hint: apt("python3-gi-cairo"),
        });
    }
    if out.is_empty() {
        out.push(DoctorItem { group: "Proyecto".into(), name: "Sin tipo de proyecto detectado".into(), ok: true, optional: true, detail: "OweeCode reconoce Capacitor, Android, Flutter, Qt, GTK, PySide, Godot y React Native".into(), hint: String::new() });
    }
    Ok(out)
}

#[cfg(test)]
mod doctor_tests {
    #[tokio::test]
    async fn doctor_reports_found_and_missing_tools() {
        let base = std::env::temp_dir();
        let gtk = base.join("owee_doctor_gtk");
        let _ = std::fs::remove_dir_all(&gtk);
        std::fs::create_dir_all(&gtk).unwrap();
        std::fs::write(
            gtk.join("main.py"),
            "import gi\ngi.require_version('Gtk','4.0')\nfrom gi.repository import Gtk\n",
        )
        .unwrap();
        for (name, dir) in [
            ("Qt", base.join("owee_qt_test")),
            ("Capacitor", base.join("owee_mobile_test").join("cap_app")),
            ("GTK", gtk),
        ] {
            if !dir.exists() {
                println!("DOCTOR {name}: (sin proyecto de prueba)");
                continue;
            }
            let items = super::project_doctor(dir.to_string_lossy().to_string())
                .await
                .unwrap();
            println!("DOCTOR {name}:");
            for i in &items {
                println!(
                    "   {} {}{} — {}",
                    if i.ok {
                        "✓"
                    } else if i.optional {
                        "·"
                    } else {
                        "✗"
                    },
                    i.name,
                    if i.optional { " (opcional)" } else { "" },
                    if i.ok {
                        i.detail.chars().take(70).collect::<String>()
                    } else {
                        i.hint.clone()
                    }
                );
            }
            assert!(!items.is_empty());
        }
    }
}
