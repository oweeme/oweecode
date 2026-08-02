// Redis browser: connect, scan keys, and read/write every Redis value type
// (string/hash/list/set/zset). Named `redis_db` (not `redis`) so it doesn't
// shadow the external `redis` crate this whole file depends on.

use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde::Serialize;

static REDIS_SESSIONS: Lazy<Mutex<HashMap<String, redis::aio::MultiplexedConnection>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_redis_conn(id: &str) -> Result<redis::aio::MultiplexedConnection, String> {
    REDIS_SESSIONS.lock().unwrap().get(id).cloned().ok_or_else(|| "No conectado a Redis".to_string())
}

#[tauri::command]
pub async fn redis_connect(id: String, url: String) -> Result<(), String> {
    let client = redis::Client::open(url).map_err(|e| e.to_string())?;
    let con = client.get_multiplexed_async_connection().await.map_err(|e| e.to_string())?;
    REDIS_SESSIONS.lock().unwrap().insert(id, con);
    Ok(())
}

#[tauri::command]
pub async fn redis_disconnect(id: String) -> Result<(), String> {
    REDIS_SESSIONS.lock().unwrap().remove(&id);
    Ok(())
}

#[derive(Serialize)]
pub struct RedisKeyInfo {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ttl: i64,
}

/// Scans keys matching `pattern` (SCAN, not KEYS — safe on large datasets) up to `limit`.
#[tauri::command]
pub async fn redis_scan_keys(id: String, pattern: String, limit: usize) -> Result<Vec<RedisKeyInfo>, String> {
    let mut con = get_redis_conn(&id)?;
    let pat = if pattern.trim().is_empty() { "*".to_string() } else { pattern };
    let mut cursor: u64 = 0;
    let mut keys: Vec<String> = Vec::new();
    loop {
        let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor).arg("MATCH").arg(&pat).arg("COUNT").arg(200)
            .query_async(&mut con).await.map_err(|e| e.to_string())?;
        keys.extend(batch);
        cursor = next_cursor;
        if cursor == 0 || keys.len() >= limit { break; }
    }
    keys.sort_unstable();
    keys.dedup();
    keys.truncate(limit);
    let mut result = Vec::with_capacity(keys.len());
    for k in keys {
        let kind: String = redis::cmd("TYPE").arg(&k).query_async(&mut con).await.unwrap_or_else(|_| "none".into());
        let ttl: i64 = redis::cmd("TTL").arg(&k).query_async(&mut con).await.unwrap_or(-1);
        result.push(RedisKeyInfo { key: k, kind, ttl });
    }
    Ok(result)
}

#[tauri::command]
pub async fn redis_db_size(id: String) -> Result<i64, String> {
    let mut con = get_redis_conn(&id)?;
    redis::cmd("DBSIZE").query_async(&mut con).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum RedisValue {
    #[serde(rename = "string")] Str { value: String },
    #[serde(rename = "hash")]   Hash { fields: Vec<(String, String)> },
    #[serde(rename = "list")]   List { items: Vec<String> },
    #[serde(rename = "set")]    Set { members: Vec<String> },
    #[serde(rename = "zset")]   ZSet { members: Vec<(String, f64)> },
    #[serde(rename = "none")]   None,
}

/// Reads the full value for `key`, auto-detecting its Redis type first.
#[tauri::command]
pub async fn redis_get_value(id: String, key: String) -> Result<RedisValue, String> {
    let mut con = get_redis_conn(&id)?;
    let kind: String = redis::cmd("TYPE").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?;
    let value = match kind.as_str() {
        "string" => {
            let s: String = redis::cmd("GET").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?;
            RedisValue::Str { value: s }
        }
        "hash" => {
            let fields: Vec<(String, String)> = redis::cmd("HGETALL").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?;
            RedisValue::Hash { fields }
        }
        "list" => {
            let items: Vec<String> = redis::cmd("LRANGE").arg(&key).arg(0).arg(-1).query_async(&mut con).await.map_err(|e| e.to_string())?;
            RedisValue::List { items }
        }
        "set" => {
            let members: Vec<String> = redis::cmd("SMEMBERS").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?;
            RedisValue::Set { members }
        }
        "zset" => {
            let raw: Vec<String> = redis::cmd("ZRANGE").arg(&key).arg(0).arg(-1).arg("WITHSCORES").query_async(&mut con).await.map_err(|e| e.to_string())?;
            let mut members = Vec::new();
            let mut it = raw.into_iter();
            while let (Some(m), Some(s)) = (it.next(), it.next()) {
                members.push((m, s.parse::<f64>().unwrap_or(0.0)));
            }
            RedisValue::ZSet { members }
        }
        _ => RedisValue::None,
    };
    Ok(value)
}

#[tauri::command]
pub async fn redis_set_string(id: String, key: String, value: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("SET").arg(&key).arg(&value).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_delete_key(id: String, key: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_rename_key(id: String, old_key: String, new_key: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("RENAME").arg(&old_key).arg(&new_key).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Sets a TTL in seconds, or removes it entirely (PERSIST) when `seconds` is negative.
#[tauri::command]
pub async fn redis_set_ttl(id: String, key: String, seconds: i64) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = if seconds < 0 {
        redis::cmd("PERSIST").arg(&key).query_async(&mut con).await.map_err(|e| e.to_string())?
    } else {
        redis::cmd("EXPIRE").arg(&key).arg(seconds).query_async(&mut con).await.map_err(|e| e.to_string())?
    };
    Ok(())
}

#[tauri::command]
pub async fn redis_hash_set(id: String, key: String, field: String, value: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("HSET").arg(&key).arg(&field).arg(&value).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_hash_del(id: String, key: String, field: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("HDEL").arg(&key).arg(&field).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_list_push(id: String, key: String, value: String, front: bool) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let cmd_name = if front { "LPUSH" } else { "RPUSH" };
    let _: () = redis::cmd(cmd_name).arg(&key).arg(&value).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_list_set(id: String, key: String, index: i64, value: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("LSET").arg(&key).arg(index).arg(&value).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Redis has no direct "remove by index" command — LSET a unique sentinel then LREM it,
/// which is the standard idiom for positional deletes.
#[tauri::command]
pub async fn redis_list_remove_index(id: String, key: String, index: i64) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let sentinel = format!("__oweeme_deleted_{}__", uuid::Uuid::new_v4());
    let _: () = redis::cmd("LSET").arg(&key).arg(index).arg(&sentinel).query_async(&mut con).await.map_err(|e| e.to_string())?;
    let _: () = redis::cmd("LREM").arg(&key).arg(1).arg(&sentinel).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_set_add(id: String, key: String, member: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("SADD").arg(&key).arg(&member).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_set_remove(id: String, key: String, member: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("SREM").arg(&key).arg(&member).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_zset_add(id: String, key: String, member: String, score: f64) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("ZADD").arg(&key).arg(score).arg(&member).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_zset_remove(id: String, key: String, member: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = redis::cmd("ZREM").arg(&key).arg(&member).query_async(&mut con).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Creates a new key of the given type with a placeholder value, for the "new key" UI action.
#[tauri::command]
pub async fn redis_create_key(id: String, key: String, kind: String) -> Result<(), String> {
    let mut con = get_redis_conn(&id)?;
    let _: () = match kind.as_str() {
        "string" => redis::cmd("SET").arg(&key).arg("").query_async(&mut con).await.map_err(|e| e.to_string())?,
        "hash"   => redis::cmd("HSET").arg(&key).arg("field").arg("").query_async(&mut con).await.map_err(|e| e.to_string())?,
        "list"   => redis::cmd("RPUSH").arg(&key).arg("").query_async(&mut con).await.map_err(|e| e.to_string())?,
        "set"    => redis::cmd("SADD").arg(&key).arg("member").query_async(&mut con).await.map_err(|e| e.to_string())?,
        "zset"   => redis::cmd("ZADD").arg(&key).arg(0).arg("member").query_async(&mut con).await.map_err(|e| e.to_string())?,
        _ => return Err("Tipo de clave desconocido".into()),
    };
    Ok(())
}
