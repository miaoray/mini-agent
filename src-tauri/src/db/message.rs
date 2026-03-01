use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

pub fn list_messages_by_conversation(conn: &Connection, conversation_id: &str) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at
         FROM message
         WHERE conversation_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([conversation_id], |row| {
        Ok(Message {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    #[test]
    fn list_messages_by_conversation_orders_by_created_at_ascending() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");

        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["minimax", "MiniMax", "openai", "http://example", "abab6.5", 1_i64],
        )
        .expect("provider insert should succeed");
        conn.execute(
            "INSERT INTO conversation (id, title, provider_id, user_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            params!["conv-1", "New Chat", "minimax", 1_i64, 1_i64],
        )
        .expect("conversation insert should succeed");
        conn.execute(
            "INSERT INTO conversation (id, title, provider_id, user_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            params!["conv-2", "New Chat", "minimax", 1_i64, 1_i64],
        )
        .expect("second conversation insert should succeed");

        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m-late", "conv-1", "assistant", "second", 20_i64],
        )
        .expect("late message insert should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m-other", "conv-2", "assistant", "other convo", 10_i64],
        )
        .expect("other conversation message insert should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m-early", "conv-1", "user", "first", 10_i64],
        )
        .expect("early message insert should succeed");

        let messages = super::list_messages_by_conversation(&conn, "conv-1")
            .expect("listing messages should succeed");

        let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["m-early", "m-late"]);
        assert!(messages.iter().all(|m| m.conversation_id == "conv-1"));
    }
}
