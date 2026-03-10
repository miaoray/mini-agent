use rusqlite::{Connection, OptionalExtension};

const DEBUG_MODE_KEY: &str = "debug_mode";

/**
 * 检查 debug mode 是否开启
 * 
 * @param conn 数据库连接
 * @return bool 是否开启 debug mode
 */
pub fn is_debug_mode(conn: &Connection) -> Result<bool, String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [DEBUG_MODE_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(value.as_deref() == Some("true"))
}

/**
 * 设置 debug mode 开关
 * 
 * @param conn 数据库连接
 * @param enabled 是否开启 debug mode
 * @return Result<(), String> 操作结果
 */
pub fn set_debug_mode(conn: &Connection, enabled: bool) -> Result<(), String> {
    let value = if enabled { "true" } else { "false" };
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        [DEBUG_MODE_KEY, value],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/**
 * LLM debug log 记录
 */
pub struct LlmDebugLog {
    pub id: String,
    pub turn_id: String,
    pub request_json: String,
    pub response_json: Option<String>,
    pub created_at: i64,
}

/**
 * 插入 LLM debug log
 * 
 * @param conn 数据库连接
 * @param id 日志 ID
 * @param turn_id agent turn ID
 * @param request_json 请求 JSON
 * @return Result<(), String> 操作结果
 */
pub fn insert_llm_debug_log(
    conn: &Connection,
    id: &str,
    turn_id: &str,
    request_json: &str,
) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO llm_debug_log (id, turn_id, request_json, response_json, created_at)
         VALUES (?1, ?2, ?3, NULL, ?4)",
        rusqlite::params![id, turn_id, request_json, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/**
 * 更新 LLM debug log 的 response
 * 
 * @param conn 数据库连接
 * @param id 日志 ID
 * @param response_json 响应 JSON
 * @return Result<(), String> 操作结果
 */
pub fn update_llm_debug_log_response(
    conn: &Connection,
    id: &str,
    response_json: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE llm_debug_log SET response_json = ?1 WHERE id = ?2",
        rusqlite::params![response_json, id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/**
 * 获取指定 turn 的 debug logs
 *
 * @param conn 数据库连接
 * @param turn_id agent turn ID
 * @return Result<Vec<LlmDebugLog>, String> debug logs 列表
 */
#[allow(dead_code)]
pub fn list_llm_debug_logs_by_turn(
    conn: &Connection,
    turn_id: &str,
) -> Result<Vec<LlmDebugLog>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, turn_id, request_json, response_json, created_at
             FROM llm_debug_log
             WHERE turn_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([turn_id], |row| {
            Ok(LlmDebugLog {
                id: row.get(0)?,
                turn_id: row.get(1)?,
                request_json: row.get(2)?,
                response_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/**
 * 获取最近的 debug logs
 * 
 * @param conn 数据库连接
 * @param limit 限制数量
 * @return Result<Vec<LlmDebugLog>, String> debug logs 列表
 */
pub fn list_recent_llm_debug_logs(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<LlmDebugLog>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, turn_id, request_json, response_json, created_at
             FROM llm_debug_log
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok(LlmDebugLog {
                id: row.get(0)?,
                turn_id: row.get(1)?,
                request_json: row.get(2)?,
                response_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");

        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES ('minimax', 'MiniMax', 'openai', 'http://example', 'abab6.5', 1)",
            [],
        )
        .expect("provider insert should succeed");

        conn
    }

    #[test]
    fn debug_mode_defaults_to_false() {
        let conn = setup_test_db();
        let result = is_debug_mode(&conn).expect("query should succeed");
        assert!(!result);
    }

    #[test]
    fn set_debug_mode_to_true() {
        let conn = setup_test_db();
        set_debug_mode(&conn, true).expect("set should succeed");
        let result = is_debug_mode(&conn).expect("query should succeed");
        assert!(result);
    }

    #[test]
    fn set_debug_mode_to_false() {
        let conn = setup_test_db();
        set_debug_mode(&conn, true).expect("set should succeed");
        set_debug_mode(&conn, false).expect("set should succeed");
        let result = is_debug_mode(&conn).expect("query should succeed");
        assert!(!result);
    }

    #[test]
    fn insert_and_update_llm_debug_log() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO conversation (id, title, created_at, updated_at)
             VALUES ('conv-1', 'Test', 1, 1)",
            [],
        )
        .expect("conversation insert should succeed");

        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES ('msg-1', 'conv-1', 'assistant', '', 1)",
            [],
        )
        .expect("message insert should succeed");

        conn.execute(
            "INSERT INTO agent_turn (id, message_id, provider_id, created_at)
             VALUES ('turn-1', 'msg-1', 'minimax', 1)",
            [],
        )
        .expect("agent_turn insert should succeed");

        insert_llm_debug_log(&conn, "log-1", "turn-1", r#"{"test": "request"}"#)
            .expect("insert should succeed");

        update_llm_debug_log_response(&conn, "log-1", r#"{"test": "response"}"#)
            .expect("update should succeed");

        let logs = list_llm_debug_logs_by_turn(&conn, "turn-1").expect("list should succeed");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].request_json, r#"{"test": "request"}"#);
        assert_eq!(logs[0].response_json, Some(r#"{"test": "response"}"#.to_string()));
    }
}
