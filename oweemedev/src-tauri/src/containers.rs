// Container management (Podman / Docker): shells out to whichever CLI is
// installed (Podman preferred — lighter, daemonless) rather than talking to
// a socket/SDK, mirroring the git integration.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[tauri::command]
pub async fn container_list(all: bool) -> Result<Vec<ContainerInfo>, String> {
    let runtime = detect_container_runtime().await?;
    let mut args: Vec<&str> = vec!["ps"];
    if all { args.push("-a"); }
    args.push("--format");
    args.push("{{.ID}}\t{{.Image}}\t{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}");
    let out = run_container_cmd(&runtime, &args).await?;
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
    Ok(list)
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

/// Commits the container's current state (not just its original image — any
/// files/config changed since it started) to a temp image, then saves that
/// image as a portable .tar a coworker can load with `podman/docker load -i`.
#[tauri::command]
pub async fn container_export(id: String, name: String, out_path: String) -> Result<(), String> {
    let runtime = detect_container_runtime().await?;
    let tag = format!(
        "oweeide-export/{}:latest",
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

    Ok(ContainerDetail { name, image, ports, volumes, env, command, restart_policy, network, workdir })
}
