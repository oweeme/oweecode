// SQL database connections (MySQL / Postgres / SQLite), query execution, the
// visual ER diagram (schema + foreign-key relationships), and SQL export/import.
// All commands here operate on the shared `DB_SESSIONS` registry, keyed by a
// connection id the frontend picks when it calls `db_connect`.

use std::fs;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::{Column, Row};

enum DbPool {
    MySql(sqlx::MySqlPool),
    Postgres(sqlx::PgPool),
    Sqlite(sqlx::SqlitePool),
}

struct DbSession { pool: Arc<DbPool> }

static DB_SESSIONS: Lazy<Mutex<HashMap<String, DbSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize)]
pub struct DbQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

/// Reads column `i` of a MySQL row, trying each concrete type sqlx supports
/// until one matches — sqlx has no single "give me this as a string" getter,
/// so every driver-specific type has to be tried in turn.
fn cell_mysql(row: &sqlx::mysql::MySqlRow, i: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<String>, _>(i)  { return v; }
    if let Ok(v) = row.try_get::<Option<i64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<u64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<i32>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<u32>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<i16>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<u16>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<i8>, _>(i)      { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<u8>, _>(i)      { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<f32>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i)    { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v.map(|b| String::from_utf8(b.clone()).unwrap_or_else(|_| format!("<blob {} bytes>", b.len())));
    }
    None
}

/// Same idea as `cell_mysql` but for Postgres's type set (includes `uuid`, no unsigned ints).
fn cell_pg(row: &sqlx::postgres::PgRow, i: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<String>, _>(i)  { return v; }
    if let Ok(v) = row.try_get::<Option<i64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<i32>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<i16>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<f32>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i)    { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<uuid::Uuid>, _>(i)            { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v.map(|b| String::from_utf8(b.clone()).unwrap_or_else(|_| format!("<bytea {} bytes>", b.len())));
    }
    None
}

/// Same idea as `cell_mysql` but for SQLite's much smaller type set.
fn cell_sqlite(row: &sqlx::sqlite::SqliteRow, i: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<String>, _>(i)  { return v; }
    if let Ok(v) = row.try_get::<Option<i64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i)     { return v.map(|x| x.to_string()); }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v.map(|b| String::from_utf8(b.clone()).unwrap_or_else(|_| format!("<blob {} bytes>", b.len())));
    }
    None
}

/// Runs an arbitrary SQL query and collects the result into a driver-agnostic
/// `DbQueryResult` (column names + string-rendered cells) the frontend can render.
async fn run_query(pool: &DbPool, sql: &str) -> Result<DbQueryResult, String> {
    match pool {
        DbPool::MySql(p) => {
            // Use raw_sql (COM_QUERY / text protocol) to avoid MariaDB binary-protocol
            // type-mismatch hangs on FLOAT, TEXT, DATE and other non-trivial columns.
            let rows = sqlx::raw_sql(sql).fetch_all(p).await.map_err(|e| e.to_string())?;
            if rows.is_empty() { return Ok(DbQueryResult { columns: vec![], rows: vec![] }); }
            let columns = rows[0].columns().iter().map(|c| c.name().to_string()).collect::<Vec<_>>();
            let result_rows = rows.iter().map(|row| (0..columns.len()).map(|i| cell_mysql(row, i)).collect()).collect();
            Ok(DbQueryResult { columns, rows: result_rows })
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(|e| e.to_string())?;
            if rows.is_empty() { return Ok(DbQueryResult { columns: vec![], rows: vec![] }); }
            let columns = rows[0].columns().iter().map(|c| c.name().to_string()).collect::<Vec<_>>();
            let result_rows = rows.iter().map(|row| (0..columns.len()).map(|i| cell_pg(row, i)).collect()).collect();
            Ok(DbQueryResult { columns, rows: result_rows })
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(|e| e.to_string())?;
            if rows.is_empty() { return Ok(DbQueryResult { columns: vec![], rows: vec![] }); }
            let columns = rows[0].columns().iter().map(|c| c.name().to_string()).collect::<Vec<_>>();
            let result_rows = rows.iter().map(|row| (0..columns.len()).map(|i| cell_sqlite(row, i)).collect()).collect();
            Ok(DbQueryResult { columns, rows: result_rows })
        }
    }
}

/// Lists table names for whichever driver `pool` is — each driver exposes this differently.
async fn list_tables_pool(pool: &DbPool) -> Result<Vec<String>, String> {
    match pool {
        // SHOW TABLES is order-of-magnitude faster than querying information_schema in MySQL
        DbPool::MySql(p) => {
            let rows = sqlx::query("SHOW TABLES").fetch_all(p).await.map_err(|e| e.to_string())?;
            let mut tables: Vec<String> = rows.iter()
                .filter_map(|row| row.try_get::<String, usize>(0).ok())
                .collect();
            tables.sort_unstable();
            Ok(tables)
        }
        DbPool::Postgres(_) | DbPool::Sqlite(_) => {
            let (sql, first_col): (&str, usize) = match pool {
                DbPool::Postgres(_) => ("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name", 0),
                _                   => ("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name", 0),
            };
            let result = run_query(pool, sql).await?;
            Ok(result.rows.into_iter().filter_map(|row| row.into_iter().nth(first_col).flatten()).collect())
        }
    }
}

/// Opens a pooled connection for `id`, replacing any previous connection under the same id.
#[tauri::command]
pub async fn db_connect(id: String, driver: String, url: String) -> Result<(), String> {
    let pool = match driver.as_str() {
        "mysql" => DbPool::MySql(
            sqlx::mysql::MySqlPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(std::time::Duration::from_secs(15))
                .connect(&url).await.map_err(|e| e.to_string())?
        ),
        "postgres" => DbPool::Postgres(
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(std::time::Duration::from_secs(15))
                .connect(&url).await.map_err(|e| e.to_string())?
        ),
        "sqlite" => DbPool::Sqlite(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(5)
                .connect(&url).await.map_err(|e| e.to_string())?
        ),
        _ => return Err(format!("Unknown driver: {}", driver)),
    };
    let old = DB_SESSIONS.lock().unwrap().remove(&id).map(|s| s.pool);
    if let Some(p) = old {
        match p.as_ref() {
            DbPool::MySql(x)    => x.close().await,
            DbPool::Postgres(x) => x.close().await,
            DbPool::Sqlite(x)   => x.close().await,
        }
    }
    DB_SESSIONS.lock().unwrap().insert(id, DbSession { pool: Arc::new(pool) });
    Ok(())
}

/// Creates a new database on the server (MySQL/Postgres only — SQLite databases are just files).
#[tauri::command]
pub async fn db_create_database(
    driver: String, host: String, port: u16, user: String, password: String, db_name: String,
) -> Result<(), String> {
    // Basic guard against SQL injection via the identifier — database names don't
    // need anything beyond alphanumerics/underscore in practice.
    if db_name.is_empty() || !db_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("El nombre de la base de datos solo puede tener letras, números y guion bajo".into());
    }
    let pass = urlencoding_encode(&password);
    match driver.as_str() {
        "mysql" => {
            let url = format!("mysql://{}:{}@{}:{}/", user, pass, host, port);
            let pool = sqlx::mysql::MySqlPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(std::time::Duration::from_secs(15))
                .connect(&url).await.map_err(|e| e.to_string())?;
            sqlx::raw_sql(&format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name))
                .execute(&pool).await.map_err(|e| e.to_string())?;
            pool.close().await;
            Ok(())
        }
        "postgres" => {
            let url = format!("postgres://{}:{}@{}:{}/postgres", user, pass, host, port);
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(std::time::Duration::from_secs(15))
                .connect(&url).await.map_err(|e| e.to_string())?;
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&db_name).fetch_one(&pool).await.map_err(|e| e.to_string())?;
            if !exists {
                sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
                    .execute(&pool).await.map_err(|e| e.to_string())?;
            }
            pool.close().await;
            Ok(())
        }
        d => Err(format!("db_create_database no soporta el driver: {}", d)),
    }
}

fn urlencoding_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Closes and forgets the pooled connection for `id` (no-op if not connected).
#[tauri::command]
pub async fn db_disconnect(id: String) -> Result<(), String> {
    let old = DB_SESSIONS.lock().unwrap().remove(&id).map(|s| s.pool);
    if let Some(p) = old {
        match p.as_ref() {
            DbPool::MySql(x)    => x.close().await,
            DbPool::Postgres(x) => x.close().await,
            DbPool::Sqlite(x)   => x.close().await,
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn db_list_tables(id: String) -> Result<Vec<String>, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    list_tables_pool(&pool).await
}

#[tauri::command]
pub async fn db_query(id: String, sql: String) -> Result<DbQueryResult, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    run_query(&pool, &sql).await
}

/// Runs a non-SELECT statement (INSERT/UPDATE/DELETE/DDL) and returns affected row count.
#[tauri::command]
pub async fn db_execute(id: String, sql: String) -> Result<u64, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    match pool.as_ref() {
        DbPool::MySql(p)     => sqlx::raw_sql(&sql).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string()),
        DbPool::Postgres(p)  => sqlx::query(&sql).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string()),
        DbPool::Sqlite(p)    => sqlx::query(&sql).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string()),
    }
}

/// Returns column metadata for `table` (name/type/nullable/default), like SQL's DESCRIBE.
#[tauri::command]
pub async fn db_describe_table(id: String, table: String, driver: String) -> Result<DbQueryResult, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    let sql = match driver.as_str() {
        "postgres" => format!(
            "SELECT column_name AS \"Field\", data_type AS \"Type\", is_nullable AS \"Null\", \
             column_default AS \"Default\", '' AS \"Key\", '' AS \"Extra\" \
             FROM information_schema.columns WHERE table_schema='public' AND table_name='{}' ORDER BY ordinal_position",
            table
        ),
        "sqlite" => format!("PRAGMA table_info(`{}`)", table),
        _ => format!("DESCRIBE `{}`", table),
    };
    run_query(&pool, &sql).await
}

// ── Visual ER diagram: schema + relationships ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct ErColumn {
    pub name: String,
    pub data_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Serialize, Clone)]
pub struct ErTable {
    pub name: String,
    pub columns: Vec<ErColumn>,
}

#[derive(Serialize, Clone)]
pub struct ErRelationship {
    pub name: String,
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

#[derive(Serialize)]
pub struct ErSchema {
    pub tables: Vec<ErTable>,
    pub relationships: Vec<ErRelationship>,
}

/// Reads column definitions (name, type, PK flag, nullability) for `table`.
async fn get_er_columns(pool: &DbPool, table: &str) -> Result<Vec<ErColumn>, String> {
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT column_name, data_type, column_key, is_nullable FROM information_schema.columns \
                 WHERE table_schema = DATABASE() AND table_name = ? ORDER BY ordinal_position"
            ).bind(table).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|r| {
                let name: String = r.try_get("column_name").unwrap_or_default();
                let data_type: String = r.try_get("data_type").unwrap_or_default();
                let key: String = r.try_get("column_key").unwrap_or_default();
                let nullable: String = r.try_get("is_nullable").unwrap_or_default();
                ErColumn { name, data_type, pk: key == "PRI", nullable: nullable == "YES" }
            }).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT c.column_name, c.data_type, c.is_nullable, \
                 EXISTS (SELECT 1 FROM information_schema.table_constraints tc \
                         JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name \
                         WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_name = c.table_name AND kcu.column_name = c.column_name) as is_pk \
                 FROM information_schema.columns c WHERE c.table_schema = 'public' AND c.table_name = $1 ORDER BY c.ordinal_position"
            ).bind(table).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|r| {
                let name: String = r.try_get("column_name").unwrap_or_default();
                let data_type: String = r.try_get("data_type").unwrap_or_default();
                let nullable: String = r.try_get("is_nullable").unwrap_or_default();
                let pk: bool = r.try_get("is_pk").unwrap_or(false);
                ErColumn { name, data_type, pk, nullable: nullable == "YES" }
            }).collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(&format!("PRAGMA table_info(\"{}\")", table))
                .fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|r| {
                let name: String = r.try_get("name").unwrap_or_default();
                let data_type: String = r.try_get("type").unwrap_or_default();
                let notnull: i64 = r.try_get("notnull").unwrap_or(0);
                let pk: i64 = r.try_get("pk").unwrap_or(0);
                ErColumn { name, data_type, pk: pk > 0, nullable: notnull == 0 }
            }).collect())
        }
    }
}

/// Reads all foreign-key relationships in the current database/schema.
async fn get_er_relationships(pool: &DbPool) -> Result<Vec<ErRelationship>, String> {
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT constraint_name, table_name, column_name, referenced_table_name, referenced_column_name \
                 FROM information_schema.key_column_usage \
                 WHERE table_schema = DATABASE() AND referenced_table_name IS NOT NULL"
            ).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|r| ErRelationship {
                name: r.try_get("constraint_name").unwrap_or_default(),
                from_table: r.try_get("table_name").unwrap_or_default(),
                from_column: r.try_get("column_name").unwrap_or_default(),
                to_table: r.try_get("referenced_table_name").unwrap_or_default(),
                to_column: r.try_get("referenced_column_name").unwrap_or_default(),
            }).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT tc.constraint_name, kcu.table_name, kcu.column_name, \
                        ccu.table_name AS foreign_table_name, ccu.column_name AS foreign_column_name \
                 FROM information_schema.table_constraints tc \
                 JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name \
                 JOIN information_schema.constraint_column_usage ccu ON tc.constraint_name = ccu.constraint_name \
                 WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = 'public'"
            ).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|r| ErRelationship {
                name: r.try_get("constraint_name").unwrap_or_default(),
                from_table: r.try_get("table_name").unwrap_or_default(),
                from_column: r.try_get("column_name").unwrap_or_default(),
                to_table: r.try_get("foreign_table_name").unwrap_or_default(),
                to_column: r.try_get("foreign_column_name").unwrap_or_default(),
            }).collect())
        }
        DbPool::Sqlite(p) => {
            let mut rels = Vec::new();
            for t in &list_tables_pool(pool).await? {
                let rows = sqlx::query(&format!("PRAGMA foreign_key_list(\"{}\")", t))
                    .fetch_all(p).await.map_err(|e| e.to_string())?;
                for r in &rows {
                    let id_: i64 = r.try_get("id").unwrap_or(0);
                    let to_table: String = r.try_get("table").unwrap_or_default();
                    let from_column: String = r.try_get("from").unwrap_or_default();
                    let to_column: String = r.try_get("to").unwrap_or_default();
                    rels.push(ErRelationship {
                        name: format!("{}__{}__{}", t, to_table, id_),
                        from_table: t.clone(), from_column, to_table, to_column,
                    });
                }
            }
            Ok(rels)
        }
    }
}

/// Builds the full ER diagram: every table's columns plus every foreign-key relationship.
#[tauri::command]
pub async fn db_get_er_schema(id: String) -> Result<ErSchema, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    let table_names = list_tables_pool(&pool).await?;
    let mut tables = Vec::with_capacity(table_names.len());
    for t in &table_names {
        tables.push(ErTable { name: t.clone(), columns: get_er_columns(&pool, t).await? });
    }
    let relationships = get_er_relationships(&pool).await?;
    Ok(ErSchema { tables, relationships })
}

/// Adds a foreign-key constraint via the ER diagram UI (drag a connection between tables).
#[tauri::command]
pub async fn db_add_relationship(
    id: String, driver: String, table: String, column: String,
    ref_table: String, ref_column: String, constraint_name: String,
) -> Result<(), String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    match (driver.as_str(), pool.as_ref()) {
        ("mysql", DbPool::MySql(p)) => {
            let sql = format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
                quote_ident("mysql", &table), quote_ident("mysql", &constraint_name),
                quote_ident("mysql", &column), quote_ident("mysql", &ref_table), quote_ident("mysql", &ref_column)
            );
            sqlx::raw_sql(&sql).execute(p).await.map_err(|e| e.to_string())?;
        }
        ("postgres", DbPool::Postgres(p)) => {
            let sql = format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
                quote_ident("postgres", &table), quote_ident("postgres", &constraint_name),
                quote_ident("postgres", &column), quote_ident("postgres", &ref_table), quote_ident("postgres", &ref_column)
            );
            sqlx::query(&sql).execute(p).await.map_err(|e| e.to_string())?;
        }
        ("sqlite", _) => return Err(
            "SQLite no permite agregar relaciones a una tabla existente sin recrearla. \
             Defínelas con FOREIGN KEY al crear la tabla.".into()
        ),
        _ => return Err(format!("Driver no soportado: {}", driver)),
    }
    Ok(())
}

/// Removes a foreign-key constraint (the inverse of `db_add_relationship`).
#[tauri::command]
pub async fn db_drop_relationship(id: String, driver: String, table: String, constraint_name: String) -> Result<(), String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    match (driver.as_str(), pool.as_ref()) {
        ("mysql", DbPool::MySql(p)) => {
            let sql = format!("ALTER TABLE {} DROP FOREIGN KEY {}", quote_ident("mysql", &table), quote_ident("mysql", &constraint_name));
            sqlx::raw_sql(&sql).execute(p).await.map_err(|e| e.to_string())?;
        }
        ("postgres", DbPool::Postgres(p)) => {
            let sql = format!("ALTER TABLE {} DROP CONSTRAINT {}", quote_ident("postgres", &table), quote_ident("postgres", &constraint_name));
            sqlx::query(&sql).execute(p).await.map_err(|e| e.to_string())?;
        }
        ("sqlite", _) => return Err(
            "SQLite no permite quitar relaciones de una tabla existente sin recrearla.".into()
        ),
        _ => return Err(format!("Driver no soportado: {}", driver)),
    }
    Ok(())
}

fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

fn quote_ident(driver: &str, ident: &str) -> String {
    if driver == "mysql" { format!("`{}`", ident.replace('`', "``")) }
    else { format!("\"{}\"", ident.replace('"', "\"\"")) }
}

/// Renders `CREATE TABLE ...` DDL for `table` (used by `db_export`).
async fn export_table_schema(pool: &DbPool, _driver: &str, table: &str) -> Result<String, String> {
    match pool {
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!("SHOW CREATE TABLE `{}`", table))
                .fetch_one(p).await.map_err(|e| e.to_string())?;
            let create: String = row.try_get(1).map_err(|e| e.to_string())?;
            Ok(format!("{};\n", create))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT sql FROM sqlite_master WHERE type='table' AND name = ?")
                .bind(table).fetch_one(p).await.map_err(|e| e.to_string())?;
            let create: String = row.try_get(0).map_err(|e| e.to_string())?;
            Ok(format!("{};\n", create))
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable, column_default, character_maximum_length \
                 FROM information_schema.columns WHERE table_schema='public' AND table_name = $1 ORDER BY ordinal_position"
            ).bind(table).fetch_all(p).await.map_err(|e| e.to_string())?;
            let mut cols = Vec::new();
            for r in &rows {
                let name: String = r.try_get("column_name").map_err(|e| e.to_string())?;
                let dtype: String = r.try_get("data_type").map_err(|e| e.to_string())?;
                let nullable: String = r.try_get("is_nullable").map_err(|e| e.to_string())?;
                let default: Option<String> = r.try_get("column_default").ok();
                let maxlen: Option<i32> = r.try_get("character_maximum_length").ok();
                let mut def = if let Some(l) = maxlen {
                    format!("\"{}\" {}({})", name, dtype, l)
                } else {
                    format!("\"{}\" {}", name, dtype)
                };
                if nullable == "NO" { def.push_str(" NOT NULL"); }
                if let Some(d) = default { def.push_str(&format!(" DEFAULT {}", d)); }
                cols.push(def);
            }
            Ok(format!("CREATE TABLE \"{}\" (\n  {}\n);\n", table, cols.join(",\n  ")))
        }
    }
}

/// Renders `INSERT INTO ...` statements for every row currently in `table` (used by `db_export`).
async fn export_table_data(pool: &DbPool, driver: &str, table: &str) -> Result<String, String> {
    let q_table = quote_ident(driver, table);
    let result = run_query(pool, &format!("SELECT * FROM {}", q_table)).await?;
    if result.rows.is_empty() { return Ok(String::new()); }
    let cols_quoted: Vec<String> = result.columns.iter().map(|c| quote_ident(driver, c)).collect();
    let mut out = String::new();
    for row in &result.rows {
        let vals: Vec<String> = row.iter().map(|v| match v {
            Some(s) => format!("'{}'", sql_escape(s)),
            None => "NULL".to_string(),
        }).collect();
        out.push_str(&format!("INSERT INTO {} ({}) VALUES ({});\n", q_table, cols_quoted.join(", "), vals.join(", ")));
    }
    Ok(out)
}

/// Exports schema + data for the given tables (or all tables) to a single .sql file.
#[tauri::command]
pub async fn db_export(id: String, driver: String, tables: Option<Vec<String>>, path: String) -> Result<(), String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    let table_list = match tables {
        Some(t) if !t.is_empty() => t,
        _ => list_tables_pool(&pool).await?,
    };
    let mut out = String::new();
    out.push_str(&format!("-- OweemeIDE SQL export ({})\n-- {}\n\n", driver, chrono::Utc::now().to_rfc3339()));
    for t in &table_list {
        out.push_str(&format!("-- Table: {}\n", t));
        out.push_str(&export_table_schema(&pool, &driver, &t).await?);
        out.push('\n');
        out.push_str(&export_table_data(&pool, &driver, &t).await?);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|e| e.to_string())
}

// Splits a SQL script into individual statements, respecting single-quoted
// strings (with '' escaping) and skipping `--` line comments, since a naive
// split on ';' breaks on any semicolon inside a text value.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            current.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') { current.push(chars.next().unwrap()); }
                else { in_string = false; }
            }
            continue;
        }
        match c {
            '\'' => { in_string = true; current.push(c); }
            '-' if chars.peek() == Some(&'-') => {
                while let Some(&nc) = chars.peek() { if nc == '\n' { break; } chars.next(); }
            }
            ';' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() { statements.push(trimmed.to_string()); }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() { statements.push(trimmed.to_string()); }
    statements
}

/// Runs every statement in a .sql file against the connection, returning how many succeeded.
#[tauri::command]
pub async fn db_import(id: String, path: String) -> Result<u64, String> {
    let pool = { DB_SESSIONS.lock().unwrap().get(&id).ok_or("Not connected")?.pool.clone() };
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let statements = split_sql_statements(&content);
    let mut count = 0u64;
    for stmt in &statements {
        let result = match pool.as_ref() {
            DbPool::MySql(p)    => sqlx::raw_sql(stmt).execute(p).await.map(|_| ()),
            DbPool::Postgres(p) => sqlx::query(stmt).execute(p).await.map(|_| ()),
            DbPool::Sqlite(p)   => sqlx::query(stmt).execute(p).await.map(|_| ()),
        };
        result.map_err(|e| format!("{}\n\nEn declaración #{}: {}", e, count + 1, stmt.chars().take(160).collect::<String>()))?;
        count += 1;
    }
    Ok(count)
}

/// Hashes/encrypts a value with the chosen algorithm — used by a small "Herramientas" utility panel.
#[tauri::command]
pub fn hash_value(algo: String, value: String) -> Result<String, String> {
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha512};
    match algo.as_str() {
        "md5" => Ok(format!("{:x}", md5::compute(value.as_bytes()))),
        "sha1" => {
            let mut h = Sha1::new();
            h.update(value.as_bytes());
            Ok(format!("{:x}", h.finalize()))
        }
        "sha256" => {
            let mut h = Sha256::new();
            h.update(value.as_bytes());
            Ok(format!("{:x}", h.finalize()))
        }
        "sha512" => {
            let mut h = Sha512::new();
            h.update(value.as_bytes());
            Ok(format!("{:x}", h.finalize()))
        }
        "bcrypt" => bcrypt::hash(&value, bcrypt::DEFAULT_COST).map_err(|e| e.to_string()),
        "argon2" => {
            use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
            use argon2::Argon2;
            let salt = SaltString::generate(&mut OsRng);
            Argon2::default()
                .hash_password(value.as_bytes(), &salt)
                .map(|h| h.to_string())
                .map_err(|e| e.to_string())
        }
        other => Err(format!("Algoritmo de cifrado no soportado: {}", other)),
    }
}

