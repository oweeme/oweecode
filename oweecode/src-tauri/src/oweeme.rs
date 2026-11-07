// Oweeme team chat — the "Oweeme" side panel talks to https://oweeme.com/api
// (or a local oweeme_pod) with a personal `owc_` developer token.
//
// The token is created in the Oweeme web dashboard (Team chat → Tokens for
// OweeCode), stored in the OS keychain and only ever read here in Rust: it never
// reaches the WebView. Requests run from Rust too, so there are no CORS issues.
//
// API reference: oweeme repo, docs/oweecode-chat-api.md

use serde::{Deserialize, Serialize};
use serde_json::Value;

const TOKEN_KEY: &str = "oweeme:token";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OweemeRoom {
    pub uuid: String,
    pub name: String,
    pub repository_url: Option<String>,
    pub workspace_name: Option<String>,
    pub project_name: Option<String>,
    pub project_uuid: Option<String>,
    pub project_key: Option<String>,
    pub my_role: String,
    pub member_count: i64,
    pub message_count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OweemeMessage {
    pub uuid: String,
    pub sender_name: String,
    pub message: String,
    pub code_context: Option<String>,
    pub attachment: Option<OweemeAttachment>,
    pub is_mine: bool,
    pub created_at: String,
}

/// Something shared with a message: kind = code | file | flow | preview.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OweemeAttachment {
    pub kind: String,
    pub name: String,
    pub content: String,
}

/// `https://oweeme.com/api/` + `/team-chat/rooms` → one slash in between.
fn join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim().trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Oweeme answers `{ "ok": true, "data": … }` or `{ "ok": false, "error": "…" }`.
fn unwrap_envelope(status: u16, body: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(body)
        .map_err(|_| format!("HTTP {status}: respuesta no válida del servidor"))?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(v.get("data").cloned().unwrap_or(Value::Null));
    }
    let err = v
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("error desconocido");
    Err(match status {
        401 => "Token no válido o revocado. Crea uno nuevo en Oweeme → Chat de equipo → Tokens."
            .to_string(),
        403 => format!("Sin permiso: {err}"),
        _ => format!("HTTP {status}: {err}"),
    })
}

fn token() -> Result<String, String> {
    match crate::secrets::secret_get(TOKEN_KEY.to_string())? {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err("Conecta OweeCode con tu cuenta de Oweeme primero.".into()),
    }
}

async fn call(
    base: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: &str,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client.request(m, join(base, path)).bearer_auth(token);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con {base}: {e}"))?;
    let status = res.status().as_u16();
    let text = res.text().await.map_err(|e| e.to_string())?;
    unwrap_envelope(status, &text)
}

fn decode<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("Respuesta inesperada del servidor: {e}"))
}

/// Saves the token after checking it really works against `base`.
#[tauri::command]
pub async fn oweeme_connect(base: String, token: String) -> Result<Vec<OweemeRoom>, String> {
    let token = token.trim().to_string();
    if !token.starts_with("owc_") {
        return Err(
            "El token de OweeCode empieza por owc_. Créalo en Oweeme → Chat de equipo → Tokens."
                .into(),
        );
    }
    let rooms: Vec<OweemeRoom> =
        decode(call(&base, "GET", "/team-chat/rooms", None, &token).await?)?;
    crate::secrets::secret_set(TOKEN_KEY.to_string(), token)?;
    Ok(rooms)
}

#[tauri::command]
pub fn oweeme_disconnect() -> Result<(), String> {
    crate::secrets::secret_delete(TOKEN_KEY.to_string())
}

#[tauri::command]
pub fn oweeme_has_token() -> Result<bool, String> {
    Ok(token().is_ok())
}

#[tauri::command]
pub async fn oweeme_rooms(base: String) -> Result<Vec<OweemeRoom>, String> {
    decode(call(&base, "GET", "/team-chat/rooms", None, &token()?).await?)
}

#[tauri::command]
pub async fn oweeme_messages(base: String, room: String) -> Result<Vec<OweemeMessage>, String> {
    decode(
        call(
            &base,
            "GET",
            &format!("/team-chat/rooms/{room}/messages"),
            None,
            &token()?,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn oweeme_send(
    base: String,
    room: String,
    message: String,
    code_context: String,
) -> Result<OweemeMessage, String> {
    let body = serde_json::json!({ "message": message, "codeContext": code_context });
    decode(
        call(
            &base,
            "POST",
            &format!("/team-chat/rooms/{room}/messages"),
            Some(body),
            &token()?,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn oweeme_create_room(
    base: String,
    name: String,
    repository_url: String,
) -> Result<OweemeRoom, String> {
    let body = serde_json::json!({ "name": name, "repositoryUrl": repository_url, "workspaceUuid": null, "projectUuid": null });
    decode(call(&base, "POST", "/team-chat/rooms", Some(body), &token()?).await?)
}

// ── Project tasks (scope `sdlc` of the token) ──────────────────────────────
// Boards, kanban and comments are passed through as JSON: the panel renders
// exactly what Oweeme returns, so new fields need no Rust change.

#[tauri::command]
pub async fn oweeme_boards(base: String, project: String) -> Result<Value, String> {
    call(
        &base,
        "GET",
        &format!("/sdlc/projects/{project}/boards"),
        None,
        &token()?,
    )
    .await
}

#[tauri::command]
pub async fn oweeme_kanban(base: String, board: String) -> Result<Value, String> {
    call(
        &base,
        "GET",
        &format!("/sdlc/boards/{board}/kanban"),
        None,
        &token()?,
    )
    .await
}

#[tauri::command]
pub async fn oweeme_move_task(base: String, task: String, list: String) -> Result<Value, String> {
    call(
        &base,
        "PUT",
        &format!("/sdlc/tasks/{task}/move"),
        Some(serde_json::json!({ "listUuid": list })),
        &token()?,
    )
    .await
}

#[tauri::command]
pub async fn oweeme_create_task(
    base: String,
    list: String,
    title: String,
) -> Result<Value, String> {
    let body = serde_json::json!({ "title": title, "priority": "medium" });
    call(
        &base,
        "POST",
        &format!("/sdlc/lists/{list}/tasks"),
        Some(body),
        &token()?,
    )
    .await
}

#[tauri::command]
pub async fn oweeme_comments(base: String, task: String) -> Result<Value, String> {
    call(
        &base,
        "GET",
        &format!("/sdlc/tasks/{task}/comments"),
        None,
        &token()?,
    )
    .await
}

#[tauri::command]
pub async fn oweeme_add_comment(base: String, task: String, body: String) -> Result<Value, String> {
    call(
        &base,
        "POST",
        &format!("/sdlc/tasks/{task}/comments"),
        Some(serde_json::json!({ "body": body })),
        &token()?,
    )
    .await
}

/// This machine's address on the local network (e.g. 192.168.1.20), so a
/// teammate on the same network can open a shared preview. Connecting a UDP
/// socket sends no packet; it only asks the OS which interface it would use.
#[tauri::command]
pub fn oweeme_lan_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_loopback()).then(|| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_ip_is_never_loopback() {
        if let Some(ip) = oweeme_lan_ip() {
            assert!(!ip.starts_with("127."), "{ip}");
        }
    }

    #[test]
    fn joins_base_and_path_with_one_slash() {
        assert_eq!(
            join("https://oweeme.com/api/", "/team-chat/rooms"),
            "https://oweeme.com/api/team-chat/rooms"
        );
        assert_eq!(
            join(" http://localhost:8090/api ", "team-chat/rooms"),
            "http://localhost:8090/api/team-chat/rooms"
        );
    }

    #[test]
    fn unwraps_ok_and_error_envelopes() {
        let data = unwrap_envelope(200, r#"{"ok":true,"data":[{"a":1}],"message":"OK"}"#).unwrap();
        assert_eq!(data[0]["a"], 1);
        let e = unwrap_envelope(401, r#"{"ok":false,"error":"Unauthorized"}"#).unwrap_err();
        assert!(e.contains("Token no válido"));
        let e = unwrap_envelope(
            403,
            r#"{"ok":false,"error":"Professional account required"}"#,
        )
        .unwrap_err();
        assert!(e.contains("Professional account required"));
        assert!(unwrap_envelope(502, "<html>bad gateway</html>").is_err());
    }

    #[test]
    fn decodes_rooms_and_messages_as_sent_by_oweeme() {
        let rooms: Vec<OweemeRoom> = decode(serde_json::json!([{
            "uuid": "r1", "name": "bella-dev", "repositoryUrl": "https://github.com/oweeme/bella",
            "workspaceName": "Clientes 2026", "projectName": "Tienda", "projectUuid": "p1", "projectKey": "BELLA",
            "myRole": "owner", "memberCount": 1, "messageCount": 3
        }])).unwrap();
        assert_eq!(
            rooms[0].repository_url.as_deref(),
            Some("https://github.com/oweeme/bella")
        );
        let msgs: Vec<OweemeMessage> = decode(serde_json::json!([{
            "uuid": "m1", "senderName": "Héctor", "message": "hola", "codeContext": null, "isMine": true, "createdAt": "2026-11-07T17:49:33Z",
            "attachment": { "kind": "flow", "name": "Checkout", "content": "{}" }
        }, {
            "uuid": "m2", "senderName": "Ana", "message": "sin adjunto", "codeContext": null, "attachment": null, "isMine": false, "createdAt": "2026-11-07T17:50:00Z"
        }])).unwrap();
        assert!(msgs[0].is_mine);
        assert_eq!(
            msgs[0].attachment.as_ref().map(|a| a.kind.as_str()),
            Some("flow")
        );
        assert!(msgs[1].attachment.is_none());
    }

    /// Live check against a running oweeme_pod:
    ///   OWEEME_TEST_TOKEN=owc_… cargo test oweeme -- --nocapture
    #[tokio::test]
    async fn live_rooms_and_send_against_pod() {
        let Ok(tok) = std::env::var("OWEEME_TEST_TOKEN") else {
            println!("SKIP (sin OWEEME_TEST_TOKEN)");
            return;
        };
        let base = std::env::var("OWEEME_TEST_BASE")
            .unwrap_or_else(|_| "http://localhost:8090/api".into());
        let rooms: Vec<OweemeRoom> = decode(
            call(&base, "GET", "/team-chat/rooms", None, &tok)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!rooms.is_empty(), "the pod demo data has one room");
        let body = serde_json::json!({ "message": "prueba desde cargo test", "codeContext": "src-tauri/src/oweeme.rs · oweeme-chat" });
        let sent: OweemeMessage = decode(
            call(
                &base,
                "POST",
                &format!("/team-chat/rooms/{}/messages", rooms[0].uuid),
                Some(body),
                &tok,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert!(sent.is_mine);
        if let Some(project) = &rooms[0].project_uuid {
            let boards = call(
                &base,
                "GET",
                &format!("/sdlc/projects/{project}/boards"),
                None,
                &tok,
            )
            .await
            .unwrap();
            let board = boards[0]["uuid"].as_str().unwrap().to_string();
            let kanban = call(
                &base,
                "GET",
                &format!("/sdlc/boards/{board}/kanban"),
                None,
                &tok,
            )
            .await
            .unwrap();
            assert!(kanban["lists"]
                .as_array()
                .map(|l| !l.is_empty())
                .unwrap_or(false));
            let paid = call(
                &base,
                "GET",
                &format!("/sdlc/projects/{project}/payments"),
                None,
                &tok,
            )
            .await
            .unwrap_err();
            assert!(
                paid.contains("not allowed"),
                "payments must stay closed to owc_ tokens: {paid}"
            );
        }
        let att = serde_json::json!({ "message": "flujo adjunto", "codeContext": "", "attachment": { "kind": "flow", "name": "Checkout", "content": "{\"nodes\":[],\"edges\":[]}" } });
        let with_att: OweemeMessage = decode(
            call(
                &base,
                "POST",
                &format!("/team-chat/rooms/{}/messages", rooms[0].uuid),
                Some(att),
                &tok,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(with_att.attachment.map(|a| a.kind), Some("flow".into()));
        let bad = call(&base, "GET", "/quotes", None, &tok).await.unwrap_err();
        assert!(
            bad.contains("not allowed"),
            "owc_ tokens must be rejected outside team chat: {bad}"
        );
        println!("OK: {} salas, mensaje {}", rooms.len(), sent.uuid);
    }
}
