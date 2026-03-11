use rusqlite::{Connection, params};

#[test]
fn build_messages_for_llm_reads_sqlite_rows_in_created_at_order() {
    let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
    conn.execute_batch(include_str!("../src/db/schema.sql"))
        .expect("schema should execute successfully");

    conn.execute(
        "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!["minimax", "MiniMax", "openai", "http://example", "abab6.5", 1_i64],
    )
    .expect("provider insert should succeed");
    conn.execute(
        "INSERT INTO conversation (id, title, user_id, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?4)",
        params!["conv-main", "Main Chat", 1_i64, 1_i64],
    )
    .expect("main conversation insert should succeed");
    conn.execute(
        "INSERT INTO conversation (id, title, user_id, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?4)",
        params!["conv-other", "Other Chat", 1_i64, 1_i64],
    )
    .expect("other conversation insert should succeed");

    conn.execute(
        "INSERT INTO message (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["main-1", "conv-main", "assistant", "Second", 2_i64],
    )
    .expect("main message 1 insert should succeed");
    conn.execute(
        "INSERT INTO message (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["main-2", "conv-main", "user", "First", 1_i64],
    )
    .expect("main message 2 insert should succeed");
    conn.execute(
        "INSERT INTO message (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["main-3", "conv-main", "user", "Third", 3_i64],
    )
    .expect("main message 3 insert should succeed");
    conn.execute(
        "INSERT INTO message (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["other-1", "conv-other", "assistant", "Ignore me", 0_i64],
    )
    .expect("other conversation message insert should succeed");

    let llm_messages = tauri_app_lib::test_support::build_messages_for_llm(&conn, "conv-main")
        .expect("building llm messages should succeed");

    let role_content_pairs: Vec<(&str, &str)> = llm_messages
        .iter()
        .map(|m| (m.role.as_str(), m.content.as_str()))
        .collect();

    // First message should be system message with time info
    assert_eq!(role_content_pairs.len(), 4);
    assert_eq!(role_content_pairs[0].0, "system");
    assert!(role_content_pairs[0].1.contains("Current date and time:"));
    
    // Then user/assistant messages in created_at order
    assert_eq!(
        role_content_pairs[1..],
        [
            ("user", "First"),
            ("assistant", "Second"),
            ("user", "Third"),
        ]
    );
}
