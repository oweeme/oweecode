// Container management (Podman / Docker): shells out to whichever CLI is
// installed (Podman preferred — lighter, daemonless) rather than talking to
// a socket/SDK, mirroring the git integration.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;

#[derive(Serialize)]
pub struct PodmanStatus {
    pub installed: bool,
    pub version: Option<String>,
    /// Detected Linux package manager to install with, if any.
    pub linux_pkg_manager: Option<String>,
    /// Whether Homebrew is available (the only scriptable macOS install path).
    pub has_brew: bool,
}

#[tauri::command]
pub async fn podman_check_installed() -> PodmanStatus {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.arg("--version");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(p) = crate::util::resolve_login_shell_path().await {
        cmd.env("PATH", p.trim());
    }
    let version = cmd.output().await.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let installed = version.is_some();

    let mut linux_pkg_manager: Option<String> = None;
    #[cfg(target_os = "linux")]
    {
        for pm in ["apt-get", "dnf", "pacman", "zypper"] {
            let found = tokio::process::Command::new("which").arg(pm).output().await
                .map(|o| o.status.success()).unwrap_or(false);
            if found { linux_pkg_manager = Some(pm.to_string()); break; }
        }
    }

    #[allow(unused_mut)]
    let mut has_brew = false;
    #[cfg(target_os = "macos")]
    {
        has_brew = tokio::process::Command::new("which").arg("brew").output().await
            .map(|o| o.status.success()).unwrap_or(false);
    }

    PodmanStatus { installed, version, linux_pkg_manager, has_brew }
}

async fn detect_container_runtime() -> Result<String, String> {
    for rt in ["podman", "docker"] {
        let ok = tokio::process::Command::new(rt)
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok { return Ok(rt.to_string()); }
    }
    Err("No se encontró Podman ni Docker instalado en el sistema".into())
}

async fn run_container_cmd(runtime: &str, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new(runtime)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar {}: {}", runtime, e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else if !stdout.is_empty() { stdout } else { format!("{} {} falló", runtime, args.join(" ")) };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Serialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub image: String,
    pub name: String,
    pub status: String,
    pub ports: String,
    pub created: String,
    pub running: bool,
}

#[tauri::command]
pub async fn container_runtime() -> Result<String, String> {
    detect_container_runtime().await
}

fn parse_ps_output(out: &str) -> Vec<ContainerInfo> {
    let mut list = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 6 { continue; }
        let status = parts[3].to_string();
        list.push(ContainerInfo {
            id: parts[0].to_string(),
            image: parts[1].to_string(),
            name: parts[2].to_string(),
            running: status.starts_with("Up"),
            status,
            ports: parts[4].to_string(),
            created: parts[5].to_string(),
        });
    }
    list
}

const PS_FORMAT: &str = "{{.ID}}\t{{.Image}}\t{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}";

#[tauri::command]
pub async fn container_list(all: bool) -> Result<Vec<ContainerInfo>, String> {
    let runtime = detect_container_runtime().await?;
    let mut args: Vec<&str> = vec!["ps"];
    if all { args.push("-a"); }
    args.push("--format");
    args.push(PS_FORMAT);
    let out = run_container_cmd(&runtime, &args).await?;
    Ok(parse_ps_output(&out))
}

/// Same as `container_list` but scoped to containers the agent created for a
/// given project — matched by the `oweecode.project` label `container_create`
/// stamps on every container it makes when the caller passes a `label`
/// (see `ContainerCreateOpts::label` / `build_run_args`). Lets the agent find
/// "my project's containers" instead of trawling every container on the
/// system, which today are otherwise indistinguishable from any other.
#[tauri::command]
pub async fn container_list_for_project(root: String, all: bool) -> Result<Vec<ContainerInfo>, String> {
    let runtime = detect_container_runtime().await?;
    let filter = format!("label=oweecode.project={}", root);
    let mut args: Vec<&str> = vec!["ps"];
    if all { args.push("-a"); }
    args.push("--filter");
    args.push(&filter);
    args.push("--format");
    args.push(PS_FORMAT);
    let out = run_container_cmd(&runtime, &args).await?;
    Ok(parse_ps_output(&out))
}

#[tauri::command]
pub async fn container_start(id: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["start", &id]).await.map(|_| ())
}

#[tauri::command]
pub async fn container_stop(id: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["stop", &id]).await.map(|_| ())
}

#[tauri::command]
pub async fn container_restart(id: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["restart", &id]).await.map(|_| ())
}

#[tauri::command]
pub async fn container_remove(id: String, force: bool) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let mut args: Vec<&str> = vec!["rm"];
    if force { args.push("-f"); }
    args.push(&id);
    run_container_cmd(&runtime, &args).await.map(|_| ())
}

/// Removes every *stopped* container — running ones are never touched by
/// `container prune`, so this is safe to call without inspecting state first.
/// `label` scopes it to one project the same way `container_list_for_project`
/// does, mirroring the panel's "this project / all projects" filter.
#[tauri::command]
pub async fn container_prune(label: Option<String>) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let mut args = vec!["container".to_string(), "prune".to_string(), "-f".to_string()];
    if let Some(l) = label {
        if !l.trim().is_empty() {
            args.push("--filter".to_string());
            args.push(format!("label=oweecode.project={}", l.trim()));
        }
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_container_cmd(&runtime, &args_ref).await.map(|_| ())
}

/// Commits the container's current state (not just its original image — any
/// files/config changed since it started) to a temp image, then saves that
/// image as a portable .tar a coworker can load with `podman/docker load -i`.
#[tauri::command]
pub async fn container_export(id: String, name: String, out_path: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let tag = format!(
        "oweecode-export/{}:latest",
        name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>()
    );
    run_container_cmd(&runtime, &["commit", &id, &tag]).await?;
    let result = run_container_cmd(&runtime, &["save", "-o", &out_path, &tag]).await;
    let _ = run_container_cmd(&runtime, &["rmi", &tag]).await;
    result.map(|_| ())
}

#[tauri::command]
pub async fn container_logs(id: String, tail: u32) -> Result<String, String> {
    let runtime = detect_container_runtime().await?;
    let tail_s = tail.to_string();
    let output = tokio::process::Command::new(&runtime)
        .args(["logs", "--tail", &tail_s, &id])
        .output()
        .await
        .map_err(|e| format!("No se pudo ejecutar {}: {}", runtime, e))?;
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !combined.is_empty() { combined.push('\n'); }
        combined.push_str(&stderr);
    }
    if !output.status.success() && combined.trim().is_empty() {
        return Err(format!("No se pudieron obtener los logs del contenedor {}", id));
    }
    Ok(combined)
}

#[derive(Serialize, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
}

#[tauri::command]
pub async fn image_list() -> Result<Vec<ImageInfo>, String> {
    let runtime = detect_container_runtime().await?;
    let out = run_container_cmd(&runtime, &["images", "--format", "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}"]).await?;
    let mut list = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 { continue; }
        list.push(ImageInfo {
            id: parts[0].to_string(), repository: parts[1].to_string(),
            tag: parts[2].to_string(), size: parts[3].to_string(),
        });
    }
    Ok(list)
}

#[tauri::command]
pub async fn image_pull(image: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["pull", &image]).await.map(|_| ())
}

/// Creates the named network if it doesn't exist yet — idempotent, so callers
/// can invoke this before every linked create without checking state first.
#[tauri::command]
pub async fn network_ensure(name: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let exists = tokio::process::Command::new(&runtime)
        .args(["network", "inspect", &name])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if exists { return Ok(()); }
    run_container_cmd(&runtime, &["network", "create", &name]).await.map(|_| ())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PortMapping { pub host: String, pub container: String, pub protocol: String }

// ── Pods ─────────────────────────────────────────────────────────────────
// Pods are a Podman-only concept (no Docker equivalent) — a group of
// containers sharing one network/pid/ipc namespace, so they can reach each
// other over `localhost` without any of the custom-network wiring a linked
// container setup otherwise needs. All pod commands short-circuit to an
// empty/no-op result under Docker instead of erroring, so the UI can just
// hide the feature rather than every call site needing its own runtime check.

#[derive(Serialize, Clone)]
pub struct PodContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Serialize, Clone)]
pub struct PodInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created: String,
    pub containers: Vec<PodContainerInfo>,
}

fn parse_pod_ps_json(out: &str) -> Result<Vec<PodInfo>, String> {
    let parsed: Value = serde_json::from_str(out).map_err(|e| e.to_string())?;
    let arr = parsed.as_array().ok_or_else(|| "Respuesta inesperada de podman pod ps".to_string())?;
    Ok(arr.iter().map(|p| {
        let containers = p["Containers"].as_array().map(|cs| {
            cs.iter().map(|c| PodContainerInfo {
                id: c["Id"].as_str().unwrap_or("").to_string(),
                name: c["Names"].as_str().unwrap_or("").to_string(),
                status: c["Status"].as_str().unwrap_or("").to_string(),
            }).collect()
        }).unwrap_or_default();
        PodInfo {
            id: p["Id"].as_str().unwrap_or("").to_string(),
            name: p["Name"].as_str().unwrap_or("").to_string(),
            status: p["Status"].as_str().unwrap_or("").to_string(),
            created: p["Created"].as_str().unwrap_or("").to_string(),
            containers,
        }
    }).collect())
}

#[tauri::command]
pub async fn pod_list() -> Result<Vec<PodInfo>, String> {
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" { return Ok(Vec::new()); }
    let out = run_container_cmd(&runtime, &["pod", "ps", "--format", "json"]).await?;
    parse_pod_ps_json(&out)
}

/// Same as `pod_list` but scoped to pods labeled for a given project — see
/// `container_list_for_project` for the matching container-level filter.
#[tauri::command]
pub async fn pod_list_for_project(root: String) -> Result<Vec<PodInfo>, String> {
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" { return Ok(Vec::new()); }
    let filter = format!("label=oweecode.project={}", root);
    let out = run_container_cmd(&runtime, &["pod", "ps", "--filter", &filter, "--format", "json"]).await?;
    parse_pod_ps_json(&out)
}

#[tauri::command]
pub async fn pod_create(name: String, ports: Vec<PortMapping>, label: String) -> Result<String, String> {
    if name.trim().is_empty() { return Err("El pod necesita un nombre".into()); }
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" {
        return Err("Los pods son una función de Podman — no están disponibles corriendo sobre Docker".into());
    }
    let mut args = vec!["pod".to_string(), "create".to_string(), "--name".to_string(), name.trim().to_string()];
    for p in &ports {
        if p.host.trim().is_empty() || p.container.trim().is_empty() { continue; }
        let suffix = if p.protocol == "udp" { "/udp" } else { "" };
        args.push("-p".to_string());
        args.push(format!("{}:{}{}", p.host.trim(), p.container.trim(), suffix));
    }
    if !label.trim().is_empty() {
        args.push("--label".to_string());
        args.push(format!("oweecode.project={}", label.trim()));
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_container_cmd(&runtime, &args_ref).await?;
    Ok(out.trim().to_string())
}

#[tauri::command]
pub async fn pod_start(id: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["pod", "start", &id]).await.map(|_| ())
}

#[tauri::command]
pub async fn pod_stop(id: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    run_container_cmd(&runtime, &["pod", "stop", &id]).await.map(|_| ())
}

#[tauri::command]
pub async fn pod_remove(id: String, force: bool) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let mut args: Vec<&str> = vec!["pod", "rm"];
    if force { args.push("-f"); }
    args.push(&id);
    run_container_cmd(&runtime, &args).await.map(|_| ())
}

/// Removes pods left with zero containers — the pod itself doesn't run
/// anything, so an empty one is always leftover cruft, never an in-use
/// resource. No-op under Docker (no pod concept there).
#[tauri::command]
pub async fn pod_prune() -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" { return Ok(()); }
    run_container_cmd(&runtime, &["pod", "prune", "-f"]).await.map(|_| ())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VolumeMapping { pub host: String, pub container: String }
#[derive(Serialize, Deserialize, Clone)]
pub struct EnvVar { pub key: String, pub value: String }

#[derive(Serialize, Deserialize, Clone)]
pub struct ContainerCreateOpts {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMapping>,
    pub env: Vec<EnvVar>,
    pub command: String,
    pub restart_policy: String,
    pub network: String,
    #[serde(default)]
    pub workdir: String,
    /// Project root path, if this container belongs to one — stamped as the
    /// `oweecode.project` label so `container_list_for_project` can find it
    /// again later. Empty/omitted (the existing container creation form
    /// never sets it) means no label, same behavior as before this field
    /// existed.
    #[serde(default)]
    pub label: String,
    /// Name of an existing pod to join (Podman only) — the container then
    /// shares the pod's network namespace instead of getting its own, so it
    /// can reach pod-mates over `localhost` with no custom network needed.
    #[serde(default)]
    pub pod: String,
}

#[derive(Serialize)]
pub struct ContainerDetail {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMapping>,
    pub env: Vec<EnvVar>,
    pub command: String,
    pub restart_policy: String,
    pub network: String,
    pub workdir: String,
    /// Pod this container belongs to, if any — read back on edit so
    /// `container_update` (which recreates the container from scratch)
    /// doesn't silently drop it from its pod. Empty under Docker or for a
    /// standalone container.
    pub pod: String,
}

fn build_run_args(opts: &ContainerCreateOpts) -> Vec<String> {
    let mut args = vec!["run".to_string(), "-d".to_string(), "--name".to_string(), opts.name.trim().to_string()];
    for p in &opts.ports {
        if p.host.trim().is_empty() || p.container.trim().is_empty() { continue; }
        let suffix = if p.protocol == "udp" { "/udp" } else { "" };
        args.push("-p".to_string());
        args.push(format!("{}:{}{}", p.host.trim(), p.container.trim(), suffix));
    }
    for v in &opts.volumes {
        if v.host.trim().is_empty() || v.container.trim().is_empty() { continue; }
        args.push("-v".to_string());
        args.push(format!("{}:{}", v.host.trim(), v.container.trim()));
    }
    for e in &opts.env {
        if e.key.trim().is_empty() { continue; }
        args.push("-e".to_string());
        args.push(format!("{}={}", e.key.trim(), e.value));
    }
    if !opts.restart_policy.trim().is_empty() && opts.restart_policy != "no" {
        args.push("--restart".to_string());
        args.push(opts.restart_policy.trim().to_string());
    }
    if !opts.network.trim().is_empty() {
        args.push("--network".to_string());
        args.push(opts.network.trim().to_string());
    }
    if !opts.label.trim().is_empty() {
        args.push("--label".to_string());
        args.push(format!("oweecode.project={}", opts.label.trim()));
    }
    if !opts.pod.trim().is_empty() {
        args.push("--pod".to_string());
        args.push(opts.pod.trim().to_string());
    }
    if !opts.workdir.trim().is_empty() {
        args.push("-w".to_string());
        args.push(opts.workdir.trim().to_string());
    }
    args.push(opts.image.trim().to_string());
    if !opts.command.trim().is_empty() {
        for part in opts.command.split_whitespace() {
            args.push(part.to_string());
        }
    }
    args
}

#[tauri::command]
pub async fn container_create(opts: ContainerCreateOpts) -> Result<String, String> {
    if opts.name.trim().is_empty() { return Err("El contenedor necesita un nombre".into()); }
    if opts.image.trim().is_empty() { return Err("El contenedor necesita una imagen".into()); }
    let runtime = detect_container_runtime().await?;
    let args = build_run_args(&opts);
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_container_cmd(&runtime, &args_ref).await?;
    Ok(out.trim().to_string())
}

#[tauri::command]
pub async fn container_update(id: String, opts: ContainerCreateOpts) -> Result<String, String> {
    if opts.name.trim().is_empty() { return Err("El contenedor necesita un nombre".into()); }
    if opts.image.trim().is_empty() { return Err("El contenedor necesita una imagen".into()); }
    let runtime = detect_container_runtime().await?;
    let _ = run_container_cmd(&runtime, &["stop", &id]).await;
    let _ = run_container_cmd(&runtime, &["rm", "-f", &id]).await;
    let args = build_run_args(&opts);
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_container_cmd(&runtime, &args_ref).await?;
    Ok(out.trim().to_string())
}

#[tauri::command]
pub async fn container_inspect(id: String) -> Result<ContainerDetail, String> {
    let runtime = detect_container_runtime().await?;
    let out = run_container_cmd(&runtime, &["inspect", &id]).await?;
    let parsed: Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let d = parsed.get(0).ok_or("Contenedor no encontrado")?;

    let name = d.get("Name").and_then(|v| v.as_str()).unwrap_or("").trim_start_matches('/').to_string();
    let config = d.get("Config").cloned().unwrap_or(Value::Null);
    let host_config = d.get("HostConfig").cloned().unwrap_or(Value::Null);

    let image = config.get("Image").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let env: Vec<EnvVar> = config.get("Env").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|e| e.as_str()).map(|s| {
            let mut parts = s.splitn(2, '=');
            let key = parts.next().unwrap_or("").to_string();
            let value = parts.next().unwrap_or("").to_string();
            EnvVar { key, value }
        }).collect()
    }).unwrap_or_default();

    let command = config.get("Cmd").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|c| c.as_str()).collect::<Vec<_>>().join(" ")
    }).unwrap_or_default();

    let mut ports = Vec::new();
    if let Some(pb) = host_config.get("PortBindings").and_then(|v| v.as_object()) {
        for (key, bindings) in pb {
            let mut parts = key.splitn(2, '/');
            let container_port = parts.next().unwrap_or("").to_string();
            let protocol = parts.next().unwrap_or("tcp").to_string();
            if let Some(arr) = bindings.as_array() {
                for b in arr {
                    let host_port = b.get("HostPort").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    ports.push(PortMapping { host: host_port, container: container_port.clone(), protocol: protocol.clone() });
                }
            }
        }
    }

    let mut volumes = Vec::new();
    if let Some(binds) = host_config.get("Binds").and_then(|v| v.as_array()) {
        for b in binds {
            if let Some(s) = b.as_str() {
                let parts: Vec<&str> = s.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    volumes.push(VolumeMapping { host: parts[0].to_string(), container: parts[1].to_string() });
                }
            }
        }
    }

    let restart_policy = host_config.get("RestartPolicy").and_then(|v| v.get("Name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let network = host_config.get("NetworkMode").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let workdir = config.get("WorkingDir").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let workdir = if workdir == "/" { String::new() } else { workdir };
    let pod = d.get("PodName").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(ContainerDetail { name, image, ports, volumes, env, command, restart_policy, network, workdir, pod })
}

// ── Pod export / import ─────────────────────────────────────────────────
// A pod isn't one thing you can `podman save` — it's a name, a set of
// published ports, and N containers each with their own image/volumes/env.
// Two export modes:
//   - "full": commit + save each member container as its own image tar
//     (same trick `container_export` uses), fully offline-restorable but
//     large — it bundles the base image *and* whatever the container's own
//     writable layer holds (its actual data, if not on a bind-mounted host
//     path).
//   - "light": keep each container's original image reference instead of
//     committing it (podman pulls it fresh on import — needs internet on the
//     destination machine), and export only its *named* volumes (the ones
//     Podman manages internally — bind-mounted host folders aren't touched,
//     since that data already lives on the host and travels with it). Much
//     smaller, since it skips re-bundling base images the destination can
//     just pull itself.

#[derive(Serialize, Deserialize)]
pub struct PodExportContainer {
    /// Image tar to `load`, only present in "full" mode — absent in "light"
    /// mode, where `opts.image` is the original registry reference instead
    /// and `container_create`'s underlying `podman run` pulls it itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tar_file: Option<String>,
    pub opts: ContainerCreateOpts,
}

#[derive(Serialize, Deserialize)]
pub struct PodExportVolume {
    pub name: String,
    pub tar_file: String,
}

#[derive(Serialize, Deserialize)]
pub struct PodExportManifest {
    pub pod_name: String,
    pub ports: Vec<PortMapping>,
    pub containers: Vec<PodExportContainer>,
    #[serde(default)]
    pub volumes: Vec<PodExportVolume>,
}

fn safe_tag_name(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

#[tauri::command]
pub async fn pod_export(
    id: String,
    pod_name: String,
    container_ids: Vec<String>,
    out_dir: String,
    full: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" {
        return Err("Exportar pods es una función de Podman — no está disponible corriendo sobre Docker".into());
    }

    let pod_json = run_container_cmd(&runtime, &["pod", "inspect", &id]).await?;
    let parsed: Value = serde_json::from_str(&pod_json).map_err(|e| e.to_string())?;
    let mut ports = Vec::new();
    if let Some(pb) = parsed.get("InfraConfig").and_then(|v| v.get("PortBindings")).and_then(|v| v.as_object()) {
        for (key, bindings) in pb {
            let mut parts = key.splitn(2, '/');
            let container_port = parts.next().unwrap_or("").to_string();
            let protocol = parts.next().unwrap_or("tcp").to_string();
            if let Some(arr) = bindings.as_array() {
                for b in arr {
                    let host_port = b.get("HostPort").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    ports.push(PortMapping { host: host_port, container: container_port.clone(), protocol: protocol.clone() });
                }
            }
        }
    }

    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_dir_path = std::path::Path::new(&out_dir);

    let total = container_ids.len();
    let mut containers = Vec::new();
    let mut volumes = Vec::new();
    let mut exported_volume_names = std::collections::HashSet::new();

    for (i, cid) in container_ids.iter().enumerate() {
        let detail = container_inspect(cid.clone()).await?;

        let (tar_file, image) = if full {
            let tag = format!("oweecode-export/{}:latest", safe_tag_name(&detail.name));
            run_container_cmd(&runtime, &["commit", cid, &tag]).await?;
            let tar_file = format!("{}.tar", safe_tag_name(&detail.name));
            let tar_path = out_dir_path.join(&tar_file);
            let save_result = run_container_cmd(&runtime, &["save", "-o", tar_path.to_str().unwrap_or(""), &tag]).await;
            let _ = run_container_cmd(&runtime, &["rmi", &tag]).await;
            save_result?;
            (Some(tar_file), tag)
        } else {
            // A bind mount's `host` side is an absolute path (already lives
            // on the host, travels with it); anything else is a Podman-
            // managed named volume, whose actual data only exists inside
            // Podman's own storage and needs exporting explicitly.
            for v in &detail.volumes {
                let vol_name = v.host.trim();
                if vol_name.is_empty() || vol_name.starts_with('/') { continue; }
                if !exported_volume_names.insert(vol_name.to_string()) { continue; }
                let vol_tar = format!("{}.tar", safe_tag_name(vol_name));
                let vol_tar_path = out_dir_path.join(&vol_tar);
                run_container_cmd(&runtime, &["volume", "export", vol_name, "-o", vol_tar_path.to_str().unwrap_or("")]).await?;
                volumes.push(PodExportVolume { name: vol_name.to_string(), tar_file: vol_tar });
            }
            (None, detail.image)
        };

        containers.push(PodExportContainer {
            tar_file,
            opts: ContainerCreateOpts {
                name: detail.name,
                image,
                ports: detail.ports,
                volumes: detail.volumes,
                env: detail.env,
                command: detail.command,
                restart_policy: detail.restart_policy,
                network: String::new(),
                workdir: detail.workdir,
                label: String::new(),
                pod: String::new(),
            },
        });

        app.emit("pod-export-progress", format!("{}/{}", i + 1, total)).ok();
    }

    let manifest = PodExportManifest { pod_name, ports, containers, volumes };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(out_dir_path.join("pod-manifest.json"), manifest_json).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn pod_import(manifest_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let runtime = detect_container_runtime().await?;
    if runtime != "podman" {
        return Err("Importar pods es una función de Podman — no está disponible corriendo sobre Docker".into());
    }

    let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let manifest: PodExportManifest = serde_json::from_str(&manifest_str).map_err(|e| e.to_string())?;
    let base_dir = std::path::Path::new(&manifest_path).parent()
        .ok_or_else(|| "Ruta de manifiesto inválida".to_string())?.to_path_buf();

    let mut pod_args = vec!["pod".to_string(), "create".to_string(), "--name".to_string(), manifest.pod_name.clone()];
    for p in &manifest.ports {
        if p.host.trim().is_empty() || p.container.trim().is_empty() { continue; }
        let suffix = if p.protocol == "udp" { "/udp" } else { "" };
        pod_args.push("-p".to_string());
        pod_args.push(format!("{}:{}{}", p.host.trim(), p.container.trim(), suffix));
    }
    let pod_args_ref: Vec<&str> = pod_args.iter().map(|s| s.as_str()).collect();
    run_container_cmd(&runtime, &pod_args_ref).await?;

    // Restore named volumes before the containers that mount them exist, so
    // `podman run` finds them already populated instead of creating them
    // empty.
    for v in &manifest.volumes {
        let _ = run_container_cmd(&runtime, &["volume", "create", &v.name]).await;
        let tar_path = base_dir.join(&v.tar_file);
        run_container_cmd(&runtime, &["volume", "import", &v.name, tar_path.to_str().unwrap_or("")]).await?;
    }

    let total = manifest.containers.len();
    for (i, c) in manifest.containers.iter().enumerate() {
        if let Some(tar_file) = &c.tar_file {
            let tar_path = base_dir.join(tar_file);
            run_container_cmd(&runtime, &["load", "-i", tar_path.to_str().unwrap_or("")]).await?;
        }
        // "light" mode containers have no tar — build_run_args below points
        // at the original registry image, which `podman run` pulls itself.

        let mut opts = c.opts.clone();
        opts.pod = manifest.pod_name.clone();
        let args = build_run_args(&opts);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_container_cmd(&runtime, &args_ref).await?;

        app.emit("pod-import-progress", format!("{}/{}", i + 1, total)).ok();
    }

    Ok(manifest.pod_name)
}
