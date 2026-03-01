use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROVIDER_ID: &str = "minimax";
const DEFAULT_PROVIDER_NAME: &str = "MiniMax M2.5";
const DEFAULT_PROVIDER_TYPE: &str = "openai";
const DEFAULT_BASE_URL: &str = "https://api.minimax.chat/v1";
const DEFAULT_MODEL_ID: &str = "abab6.5";
const DEFAULT_ANTHROPIC_MODEL_ID: &str = "MiniMax-M2.5";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub base_url: String,
    pub model_id: String,
    pub created_at: i64,
}

pub fn insert_default_provider(conn: &Connection) -> Result<()> {
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM provider", [], |row| row.get(0))?;
    if existing > 0 {
        return Ok(());
    }

    let base_url = env::var("MINIMAX_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let model_id = resolve_default_model_id_for_base_url(&base_url);
    let now = now_unix_ts();

    conn.execute(
        "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            DEFAULT_PROVIDER_ID,
            DEFAULT_PROVIDER_NAME,
            DEFAULT_PROVIDER_TYPE,
            base_url,
            model_id,
            now
        ],
    )?;

    Ok(())
}

#[allow(dead_code)]
pub fn get_provider_by_id(conn: &Connection, id: &str) -> Result<Option<Provider>> {
    conn.query_row(
        "SELECT id, name, type, base_url, model_id, created_at FROM provider WHERE id = ?1",
        [id],
        |row| {
            Ok(Provider {
                id: row.get(0)?,
                name: row.get(1)?,
                r#type: row.get(2)?,
                base_url: row.get(3)?,
                model_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
    .optional()
}

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>> {
    let mut stmt =
        conn.prepare("SELECT id, name, type, base_url, model_id, created_at FROM provider ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Provider {
            id: row.get(0)?,
            name: row.get(1)?,
            r#type: row.get(2)?,
            base_url: row.get(3)?,
            model_id: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;

    rows.collect()
}

fn now_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn resolve_default_model_id_for_base_url(base_url: &str) -> String {
    if let Ok(model_id) = env::var("MINIMAX_MODEL_ID") {
        if !model_id.trim().is_empty() {
            return model_id;
        }
    }

    if is_anthropic_like_base_url(base_url) {
        DEFAULT_ANTHROPIC_MODEL_ID.to_string()
    } else {
        DEFAULT_MODEL_ID.to_string()
    }
}

fn is_anthropic_like_base_url(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("/anthropic")
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use rusqlite::Connection;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn insert_default_provider_is_idempotent() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");

        super::insert_default_provider(&conn).expect("first insert should succeed");
        super::insert_default_provider(&conn).expect("second insert should be no-op");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider", [], |row| row.get(0))
            .expect("provider count query should succeed");
        assert_eq!(count, 1);

        let provider = super::get_provider_by_id(&conn, super::DEFAULT_PROVIDER_ID)
            .expect("provider query should succeed")
            .expect("default provider should exist");
        assert_eq!(provider.name, "MiniMax M2.5");

        let providers = super::list_providers(&conn).expect("list providers should succeed");
        assert_eq!(providers.len(), 1);
    }

    #[test]
    fn defaults_model_to_m2_5_when_anthropic_base_url_and_model_env_missing() {
        let _guard = env_lock().lock().expect("env lock should acquire");
        std::env::set_var("MINIMAX_BASE_URL", "https://api.minimaxi.com/anthropic");
        let _ = std::env::remove_var("MINIMAX_MODEL_ID");

        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");
        super::insert_default_provider(&conn).expect("default provider insert should succeed");
        let provider = super::get_provider_by_id(&conn, super::DEFAULT_PROVIDER_ID)
            .expect("provider query should succeed")
            .expect("default provider should exist");
        assert_eq!(provider.model_id, "MiniMax-M2.5");

        let _ = std::env::remove_var("MINIMAX_BASE_URL");
    }
}
