use rusqlite::Connection;

use crate::agent::StoredMessage;
use crate::llm::minimax::ChatMessage;

pub fn build_messages_for_llm(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<ChatMessage>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT role, content
         FROM message
         WHERE conversation_id = ?1
         ORDER BY created_at ASC",
    )?;

    let rows = stmt.query_map([conversation_id], |row| {
        Ok(StoredMessage {
            role: row.get(0)?,
            content: row.get(1)?,
        })
    })?;

    // TODO(task-9): include tool invocation/tool result messages when tool_calls are wired in llm.
    rows.map(|row| {
        row.map(|message| ChatMessage {
            role: message.role,
            content: message.content,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    #[test]
    fn build_messages_for_llm_preserves_role_content_and_order() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("../db/schema.sql"))
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
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m1", "conv-1", "user", "Hello", 1_i64],
        )
        .expect("first message insert should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m2", "conv-1", "assistant", "Hi there", 2_i64],
        )
        .expect("second message insert should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m3", "conv-1", "user", "Plan a trip", 3_i64],
        )
        .expect("third message insert should succeed");

        let llm_messages = super::build_messages_for_llm(&conn, "conv-1")
            .expect("building llm messages should succeed");
        let roles: Vec<String> = llm_messages.iter().map(|m| m.role.clone()).collect();
        let contents: Vec<String> = llm_messages.iter().map(|m| m.content.clone()).collect();

        assert_eq!(roles, vec!["user", "assistant", "user"]);
        assert_eq!(contents, vec!["Hello", "Hi there", "Plan a trip"]);
    }
}
