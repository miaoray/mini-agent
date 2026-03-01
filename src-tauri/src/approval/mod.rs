use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResolvedEvent {
    pub conversation_id: String,
    pub message_id: String,
    pub approval_id: String,
    pub status: String,
}

#[derive(Debug)]
struct PendingApprovalRow {
    approval_id: String,
    conversation_id: String,
    turn_id: String,
    action_type: String,
    payload_json: String,
    status: String,
}

pub fn approve_action(
    conn: &Connection,
    approval_id: &str,
    base_dir: &Path,
) -> Result<ApprovalResolvedEvent, String> {
    let row = load_pending_approval(conn, approval_id)?;
    if row.status != "pending" {
        return Err(format!(
            "approval is not pending (id: {}, status: {})",
            row.approval_id, row.status
        ));
    }

    let payload: Value =
        serde_json::from_str(&row.payload_json).map_err(|e| format!("invalid payload_json: {e}"))?;
    execute_approval_action(&row.action_type, &payload, base_dir)?;

    conn.execute(
        "UPDATE pending_approval SET status = 'approved' WHERE id = ?1",
        [row.approval_id.as_str()],
    )
    .map_err(|e| e.to_string())?;
    let message_id = resolve_assistant_message_id_for_turn(conn, &row.turn_id)?;

    Ok(ApprovalResolvedEvent {
        conversation_id: row.conversation_id,
        message_id,
        approval_id: row.approval_id,
        status: "approved".to_string(),
    })
}

pub fn reject_action(conn: &Connection, approval_id: &str) -> Result<ApprovalResolvedEvent, String> {
    let row = load_pending_approval(conn, approval_id)?;
    if row.status != "pending" {
        return Err(format!(
            "approval is not pending (id: {}, status: {})",
            row.approval_id, row.status
        ));
    }

    conn.execute(
        "UPDATE pending_approval SET status = 'rejected' WHERE id = ?1",
        [row.approval_id.as_str()],
    )
    .map_err(|e| e.to_string())?;
    insert_rejection_context_message(conn, &row)?;
    let message_id = resolve_assistant_message_id_for_turn(conn, &row.turn_id)?;

    Ok(ApprovalResolvedEvent {
        conversation_id: row.conversation_id,
        message_id,
        approval_id: row.approval_id,
        status: "rejected".to_string(),
    })
}

fn load_pending_approval(conn: &Connection, approval_id: &str) -> Result<PendingApprovalRow, String> {
    conn.query_row(
        "SELECT p.id, p.conversation_id, p.turn_id, p.action_type, p.payload_json, p.status
         FROM pending_approval p
         WHERE p.id = ?1",
        [approval_id],
        |row| {
            Ok(PendingApprovalRow {
                approval_id: row.get(0)?,
                conversation_id: row.get(1)?,
                turn_id: row.get(2)?,
                action_type: row.get(3)?,
                payload_json: row.get(4)?,
                status: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("approval not found: {approval_id}"))
}

fn resolve_assistant_message_id_for_turn(conn: &Connection, turn_id: &str) -> Result<String, String> {
    conn.query_row(
        "SELECT message_id FROM agent_turn WHERE id = ?1",
        [turn_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("agent turn not found for approval turn_id: {turn_id}"))
}

fn insert_rejection_context_message(conn: &Connection, row: &PendingApprovalRow) -> Result<(), String> {
    let path = serde_json::from_str::<Value>(&row.payload_json)
        .ok()
        .and_then(|payload| payload.get("path").and_then(Value::as_str).map(ToOwned::to_owned));
    let rejection_content = match path {
        Some(path) if !path.trim().is_empty() => {
            format!(
                "User rejected pending tool action: {} on path {}.",
                row.action_type, path
            )
        }
        _ => format!("User rejected pending tool action: {}.", row.action_type),
    };

    conn.execute(
        "INSERT INTO message (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, 'user', ?3, ?4)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            row.conversation_id.clone(),
            rejection_content,
            now_unix_ts()
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn now_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn execute_approval_action(action_type: &str, payload: &Value, base_dir: &Path) -> Result<(), String> {
    match action_type {
        "create_directory" => {
            let relative_path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "create_directory payload.path is required".to_string())?;
            let target = resolve_target_path(base_dir, relative_path)?;
            fs::create_dir_all(&target).map_err(|e| e.to_string())
        }
        "write_file" => {
            let relative_path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "write_file payload.path is required".to_string())?;
            let content = payload
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| "write_file payload.content is required".to_string())?;
            let target = resolve_target_path(base_dir, relative_path)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&target, content).map_err(|e| e.to_string())
        }
        other => Err(format!("unsupported approval action_type: {other}")),
    }
}

fn resolve_target_path(base_dir: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err("path cannot be empty".to_string());
    }

    let parsed = Path::new(trimmed);
    if parsed.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if parsed
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("path traversal is not allowed".to_string());
    }

    Ok(base_dir.join(parsed))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{Connection, params};

    fn now_unix_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn seed_pending_approval(
        conn: &Connection,
        approval_id: &str,
        action_type: &str,
        payload_json: &str,
    ) {
        let now = now_unix_ts();
        conn.execute_batch(include_str!("../db/schema.sql"))
            .expect("schema should execute");
        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["minimax", "MiniMax", "openai", "http://example", "m", now],
        )
        .expect("provider insert should succeed");
        conn.execute(
            "INSERT INTO conversation (id, title, provider_id, user_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            params!["conv-1", "New Chat", "minimax", now, now],
        )
        .expect("conversation insert should succeed");
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, 'assistant', '', ?3)",
            params!["msg-1", "conv-1", now],
        )
        .expect("message insert should succeed");
        conn.execute(
            "INSERT INTO agent_turn (id, message_id, provider_id, prompt_tokens, completion_tokens, created_at)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4)",
            params!["turn-1", "msg-1", "minimax", now],
        )
        .expect("agent_turn insert should succeed");
        conn.execute(
            "INSERT INTO pending_approval (id, conversation_id, turn_id, action_type, payload_json, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
            params![approval_id, "conv-1", "turn-1", action_type, payload_json, now],
        )
        .expect("pending approval insert should succeed");
    }

    fn make_temp_base_dir() -> std::path::PathBuf {
        let unique = format!(
            "mini-agent-approval-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let base = std::env::temp_dir().join(unique);
        if base.exists() {
            std::fs::remove_dir_all(&base).expect("stale temp directory should be removable");
        }
        std::fs::create_dir_all(&base).expect("temp base dir should be creatable");
        base
    }

    #[test]
    fn approve_create_directory_executes_action_and_sets_status() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        seed_pending_approval(
            &conn,
            "approval-create-dir",
            "create_directory",
            r#"{"path":"sandbox/new-dir"}"#,
        );
        let base_dir = make_temp_base_dir();
        let expected_dir = base_dir.join("sandbox/new-dir");
        assert!(!expected_dir.exists());

        let event = super::approve_action(&conn, "approval-create-dir", &base_dir)
            .expect("approve should execute create_directory");

        assert_eq!(event.approval_id, "approval-create-dir");
        assert_eq!(event.status, "approved");
        assert!(expected_dir.exists());
        let status: String = conn
            .query_row(
                "SELECT status FROM pending_approval WHERE id = ?1",
                ["approval-create-dir"],
                |row| row.get(0),
            )
            .expect("status query should succeed");
        assert_eq!(status, "approved");

        std::fs::remove_dir_all(base_dir).expect("temp base dir should be removable");
    }

    #[test]
    fn reject_sets_status_without_fs_side_effects() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        seed_pending_approval(
            &conn,
            "approval-reject",
            "create_directory",
            r#"{"path":"sandbox/rejected-dir"}"#,
        );

        let event = super::reject_action(&conn, "approval-reject")
            .expect("reject should set status to rejected");
        assert_eq!(event.approval_id, "approval-reject");
        assert_eq!(event.status, "rejected");

        let status: String = conn
            .query_row(
                "SELECT status FROM pending_approval WHERE id = ?1",
                ["approval-reject"],
                |row| row.get(0),
            )
            .expect("status query should succeed");
        assert_eq!(status, "rejected");
    }

    #[test]
    fn resolve_assistant_message_id_reads_message_from_turn_id() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        seed_pending_approval(
            &conn,
            "approval-turn-resolution",
            "create_directory",
            r#"{"path":"sandbox/path"}"#,
        );
        let pending = super::load_pending_approval(&conn, "approval-turn-resolution")
            .expect("pending approval should load");

        let message_id = super::resolve_assistant_message_id_for_turn(&conn, &pending.turn_id)
            .expect("message id should resolve from turn");
        assert_eq!(message_id, "msg-1");
    }

    #[test]
    fn reject_inserts_rejection_context_message_with_tool_and_path() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        seed_pending_approval(
            &conn,
            "approval-reject-context",
            "write_file",
            r#"{"path":"sandbox/note.txt","content":"hello"}"#,
        );

        super::reject_action(&conn, "approval-reject-context")
            .expect("reject should insert rejection context");

        let content: String = conn
            .query_row(
                "SELECT content FROM message WHERE conversation_id = ?1 AND role = 'user' AND content LIKE 'User rejected pending tool action:%'
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                ["conv-1"],
                |row| row.get(0),
            )
            .expect("rejection context message should be inserted");
        assert!(content.contains("write_file"));
        assert!(content.contains("sandbox/note.txt"));
    }
}
