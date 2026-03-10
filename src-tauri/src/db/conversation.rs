use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub user_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn create_conversation(conn: &Connection) -> Result<Conversation> {
    let id = Uuid::new_v4().to_string();
    let now = now_unix_ts();
    let title = "New Chat";

    conn.execute(
        "INSERT INTO conversation (id, title, user_id, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?4)",
        params![id, title, now, now],
    )?;

    Ok(Conversation {
        id,
        title: title.to_string(),
        user_id: None,
        created_at: now,
        updated_at: now,
    })
}

pub fn clear_all_conversations(conn: &Connection) -> Result<()> {
    // Use transaction to ensure atomicity
    let tx = conn.unchecked_transaction()?;
    
    // Delete in reverse dependency order to avoid foreign key constraint errors:
    // First delete from tables that reference other tables (bottom-up in dependency tree)
    tx.execute("DELETE FROM tool_invocation", [])?;
    tx.execute("DELETE FROM llm_debug_log", [])?;
    tx.execute("DELETE FROM pending_approval", [])?;
    tx.execute("DELETE FROM agent_turn", [])?;
    tx.execute("DELETE FROM message", [])?;
    tx.execute("DELETE FROM conversation", [])?;
    
    tx.commit()?;
    Ok(())
}

/// Clear all conversations except the specified one
pub fn clear_other_conversations(conn: &Connection, keep_conversation_id: &str) -> Result<()> {
    // Get all conversation IDs except the one to keep (outside transaction to avoid borrow issues)
    let mut stmt = conn.prepare("SELECT id FROM conversation WHERE id != ?1")?;
    let conversation_ids: Vec<String> = stmt
        .query_map([keep_conversation_id], |row| row.get(0))?
        .filter_map(|result| result.ok())
        .collect();
    drop(stmt); // Explicitly drop the statement to release the borrow
    
    if conversation_ids.is_empty() {
        // No other conversations to delete
        return Ok(());
    }
    
    // Use transaction to ensure atomicity
    let tx = conn.unchecked_transaction()?;
    
    // Delete each conversation one by one to avoid complex IN clause handling
    for conv_id in &conversation_ids {
        // tool_invocation has no conversation_id, delete via turn_id -> agent_turn -> message
        tx.execute(
            "DELETE FROM tool_invocation WHERE turn_id IN (SELECT id FROM agent_turn WHERE message_id IN (SELECT id FROM message WHERE conversation_id = ?1))",
            [conv_id],
        )?;
        
        tx.execute(
            "DELETE FROM llm_debug_log WHERE turn_id IN (SELECT id FROM agent_turn WHERE message_id IN (SELECT id FROM message WHERE conversation_id = ?1))",
            [conv_id],
        )?;
        
        tx.execute(
            "DELETE FROM pending_approval WHERE conversation_id = ?1",
            [conv_id],
        )?;
        
        tx.execute(
            "DELETE FROM agent_turn WHERE message_id IN (SELECT id FROM message WHERE conversation_id = ?1)",
            [conv_id],
        )?;
        
        tx.execute(
            "DELETE FROM message WHERE conversation_id = ?1",
            [conv_id],
        )?;
        
        tx.execute(
            "DELETE FROM conversation WHERE id = ?1",
            [conv_id],
        )?;
    }
    
    tx.commit()?;
    Ok(())
}

pub fn list_conversations(conn: &Connection) -> Result<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, user_id, created_at, updated_at
         FROM conversation
         ORDER BY updated_at DESC, created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Conversation {
            id: row.get(0)?,
            title: row.get(1)?,
            user_id: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;

    rows.collect()
}

pub fn get_conversation(conn: &Connection, id: &str) -> Result<Conversation> {
    conn.query_row(
        "SELECT id, title, user_id, created_at, updated_at
         FROM conversation
         WHERE id = ?1",
        [id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                user_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
}

#[allow(dead_code)]
pub fn update_conversation_title(conn: &Connection, id: &str, title: &str) -> Result<()> {
    let now = now_unix_ts();
    let rows_affected = conn.execute(
        "UPDATE conversation SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, now, id],
    )?;
    if rows_affected == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
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
    use rusqlite::{Connection, params};

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

        let created = super::create_conversation(&conn)
            .expect("create conversation should succeed");
        let list = super::list_conversations(&conn).expect("list conversations should succeed");

        assert!(!list.is_empty());
        assert!(list.iter().any(|c| c.id == created.id));
    }

    #[test]
    fn get_conversation_returns_saved_row() {
        let conn = test_conn();

        let created = super::create_conversation(&conn)
            .expect("create conversation should succeed");
        let loaded = super::get_conversation(&conn, &created.id)
            .expect("get conversation should succeed");

        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.title, "New Chat");
    }

    #[test]
    fn update_conversation_title_persists_changes() {
        let conn = test_conn();

        let created = super::create_conversation(&conn)
            .expect("create conversation should succeed");
        super::update_conversation_title(&conn, &created.id, "Renamed Chat")
            .expect("update title should succeed");

        let loaded = super::get_conversation(&conn, &created.id)
            .expect("get conversation should succeed");
        assert_eq!(loaded.title, "Renamed Chat");
    }

    #[test]
    fn update_conversation_title_errors_when_not_found() {
        let conn = test_conn();
    
        let err = super::update_conversation_title(&conn, "missing-id", "Renamed Chat")
            .expect_err("update title should fail when conversation is missing");
        assert!(matches!(err, rusqlite::Error::QueryReturnedNoRows));
    }
    
    #[test]
    fn clear_other_conversations_keeps_specified_conversation() {
        let conn = test_conn();
    
        // Create multiple conversations
        let conv1 = super::create_conversation(&conn)
            .expect("create conversation 1 should succeed");
        let conv2 = super::create_conversation(&conn)
            .expect("create conversation 2 should succeed");
        let conv3 = super::create_conversation(&conn)
            .expect("create conversation 3 should succeed");
    
        // Add messages to each conversation
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m1", &conv1.id, "user", "Hello conv1", 10],
        ).expect("insert message 1 should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m2", &conv2.id, "user", "Hello conv2", 20],
        ).expect("insert message 2 should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["m3", &conv3.id, "user", "Hello conv3", 30],
        ).expect("insert message 3 should succeed");
    
        // Clear all except conv2
        super::clear_other_conversations(&conn, &conv2.id)
            .expect("clear other conversations should succeed");
    
        // Verify only conv2 remains
        let list = super::list_conversations(&conn)
            .expect("list conversations should succeed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, conv2.id);
    
        // Verify only conv2's messages remain
        let mut stmt = conn.prepare("SELECT id FROM message ORDER BY id").expect("prepare should succeed");
        let message_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query map should succeed")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(message_ids, vec!["m2"]);
    }
    
    #[test]
    fn clear_other_conversations_with_nonexistent_conversation() {
        let conn = test_conn();
    
        // Create multiple conversations
        let _conv1 = super::create_conversation(&conn)
            .expect("create conversation 1 should succeed");
        let _conv2 = super::create_conversation(&conn)
            .expect("create conversation 2 should succeed");    
        // Try to clear all except a non-existent conversation ID
        super::clear_other_conversations(&conn, "non-existent-id")
            .expect("clear other conversations should succeed");
    
        // All conversations should be deleted
        let list = super::list_conversations(&conn)
            .expect("list conversations should succeed");
        assert_eq!(list.len(), 0);
    }
    
    #[test]
    fn clear_other_conversations_with_single_conversation() {
        let conn = test_conn();
    
        // Create one conversation
        let conv1 = super::create_conversation(&conn)
            .expect("create conversation should succeed");
    
        // Clear all except this one
        super::clear_other_conversations(&conn, &conv1.id)
            .expect("clear other conversations should succeed");
    
        // Conversation should still exist
        let list = super::list_conversations(&conn)
            .expect("list conversations should succeed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, conv1.id);
    }}
