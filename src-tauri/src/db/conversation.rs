use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub user_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn create_conversation(conn: &Connection, provider_id: &str) -> Result<Conversation> {
    let id = Uuid::new_v4().to_string();
    let now = now_unix_ts();
    let title = "New Chat";

    conn.execute(
        "INSERT INTO conversation (id, title, provider_id, user_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
        params![id, title, provider_id, now, now],
    )?;

    Ok(Conversation {
        id,
        title: title.to_string(),
        provider_id: provider_id.to_string(),
        user_id: None,
        created_at: now,
        updated_at: now,
    })
}

pub fn list_conversations(conn: &Connection) -> Result<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, provider_id, user_id, created_at, updated_at
         FROM conversation
         ORDER BY updated_at DESC, created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Conversation {
            id: row.get(0)?,
            title: row.get(1)?,
            provider_id: row.get(2)?,
            user_id: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    rows.collect()
}

pub fn get_conversation(conn: &Connection, id: &str) -> Result<Conversation> {
    conn.query_row(
        "SELECT id, title, provider_id, user_id, created_at, updated_at
         FROM conversation
         WHERE id = ?1",
        [id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                provider_id: row.get(2)?,
                user_id: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
}

#[allow(dead_code)]
pub fn update_conversation_title(conn: &Connection, id: &str, title: &str) -> Result<()> {
    let now = now_unix_ts();
    conn.execute(
        "UPDATE conversation SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, now, id],
    )?;
    Ok(())
}

fn now_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");
        super::super::provider::insert_default_provider(&conn)
            .expect("default provider seed should succeed");
        conn
    }

    #[test]
    fn create_and_list_conversations() {
        let conn = test_conn();

        let created = super::create_conversation(&conn, super::super::provider::DEFAULT_PROVIDER_ID)
            .expect("create conversation should succeed");
        let list = super::list_conversations(&conn).expect("list conversations should succeed");

        assert!(!list.is_empty());
        assert!(list.iter().any(|c| c.id == created.id));
    }

    #[test]
    fn get_conversation_returns_saved_row() {
        let conn = test_conn();

        let created = super::create_conversation(&conn, super::super::provider::DEFAULT_PROVIDER_ID)
            .expect("create conversation should succeed");
        let loaded = super::get_conversation(&conn, &created.id)
            .expect("get conversation should succeed");

        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.provider_id, created.provider_id);
        assert_eq!(loaded.title, "New Chat");
    }

    #[test]
    fn update_conversation_title_persists_changes() {
        let conn = test_conn();

        let created = super::create_conversation(&conn, super::super::provider::DEFAULT_PROVIDER_ID)
            .expect("create conversation should succeed");
        super::update_conversation_title(&conn, &created.id, "Renamed Chat")
            .expect("update title should succeed");

        let loaded = super::get_conversation(&conn, &created.id)
            .expect("get conversation should succeed");
        assert_eq!(loaded.title, "Renamed Chat");
    }
}
