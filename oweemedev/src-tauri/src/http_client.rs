// Generic HTTP client backing the "REST client" panel (like a mini Postman) —
// runs the request in Rust instead of the browser fetch so it isn't subject
// to CORS restrictions.

use serde::Serialize;

#[derive(Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
    pub time_ms: u64,
}

#[tauri::command]
pub async fn http_request(
    method: String,
    url: String,
    headers: std::collections::HashMap<String, String>,
    body: Option<String>,
) -> Result<HttpResponse, String> {
    use reqwest::Client;
    let client = Client::builder()
        .danger_accept_invalid_certs(false)
        .build()
        .map_err(|e| e.to_string())?;

    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client.request(m, &url);
    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(b) = body {
        if !b.is_empty() { req = req.body(b); }
    }

    let start = std::time::Instant::now();
    let res = req.send().await.map_err(|e| e.to_string())?;
    let time_ms = start.elapsed().as_millis() as u64;

    let status = res.status().as_u16();
    let status_text = res.status().canonical_reason().unwrap_or("").to_string();
    let mut hdrs = std::collections::HashMap::new();
    for (k, v) in res.headers() {
        hdrs.insert(k.to_string(), v.to_str().unwrap_or("").to_string());
    }
    let body_bytes = res.bytes().await.map_err(|e| e.to_string())?;
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    Ok(HttpResponse { status, status_text, headers: hdrs, body: body_str, time_ms })
}
