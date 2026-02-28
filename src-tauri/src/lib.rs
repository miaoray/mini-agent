mod agent;
mod db;
pub mod llm;
pub mod tools;

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use tauri::{Emitter, Manager};
use uuid::Uuid;

#[allow(dead_code)]
pub struct ToolRegistryState {
    registry: tools::ToolRegistry,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn create_conversation(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let conn = state.connection()?;
    let provider_id = select_provider_id(&conn)?;
    let conversation =
        db::conversation::create_conversation(&conn, &provider_id).map_err(|e| e.to_string())?;

    Ok(conversation.id)
}

#[tauri::command]
fn list_conversations(state: tauri::State<'_, db::DbState>) -> Result<Vec<db::conversation::Conversation>, String> {
    let conn = state.connection()?;
    db::conversation::list_conversations(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversation(
    state: tauri::State<'_, db::DbState>,
    id: String,
) -> Result<db::conversation::Conversation, String> {
    let conn = state.connection()?;
    db::conversation::get_conversation(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    conversation_id: String,
    content: String,
) -> Result<String, String> {
    let assistant_message_id = Uuid::new_v4().to_string();
    let user_message_id = Uuid::new_v4().to_string();
    let provider_runtime = {
        let conn = state.connection()?;
        let now = now_unix_ts();
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, 'user', ?3, ?4)",
            params![user_message_id, conversation_id.clone(), content, now],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE conversation SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id.clone()],
        )
        .map_err(|e| e.to_string())?;
        load_provider_runtime_for_conversation(&conn, &conversation_id)?
    };

    let app_handle = app.clone();
    let background_conversation_id = conversation_id.clone();
    let background_message_id = assistant_message_id.clone();
    tokio::spawn(async move {
        if let Err(err) = run_agent_turn(
            &app_handle,
            background_conversation_id.clone(),
            background_message_id.clone(),
            provider_runtime,
        )
        .await
        {
            let _ = app_handle.emit(
                "chat-error",
                agent::ChatErrorEvent {
                    conversation_id: background_conversation_id,
                    message_id: background_message_id,
                    message: err,
                },
            );
        }
    });

    Ok(assistant_message_id)
}

async fn run_agent_turn(
    app_handle: &tauri::AppHandle,
    conversation_id: String,
    assistant_message_id: String,
    provider_runtime: agent::ProviderRuntime,
) -> Result<(), String> {
    let llm_messages = {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        agent::r#loop::build_messages_for_llm(&conn, &conversation_id).map_err(|e| e.to_string())?
    };

    // TODO(task-8/task-9): pass tool definitions and consume tool_calls once llm layer supports them.
    let api_key = env::var("MINIMAX_API_KEY")
        .map_err(|_| "missing required environment variable: MINIMAX_API_KEY".to_string())?;
    let client = llm::minimax::MiniMaxClient::new(api_key, provider_runtime.base_url.clone());
    let streamed = client
        .chat_completion_stream_collect(&provider_runtime.model_id, &llm_messages)
        .await
        .map_err(|e| e.to_string())?;

    for delta in &streamed.deltas {
        app_handle
            .emit(
                "chat-delta",
                agent::ChatDeltaEvent {
                    conversation_id: conversation_id.clone(),
                    message_id: assistant_message_id.clone(),
                    delta: delta.clone(),
                },
            )
            .map_err(|e| e.to_string())?;
    }

    {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        let now = now_unix_ts();
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, 'assistant', ?3, ?4)",
            params![
                assistant_message_id.clone(),
                conversation_id.clone(),
                streamed.collected_text.clone(),
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO agent_turn (id, message_id, provider_id, prompt_tokens, completion_tokens, created_at)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4)",
            params![
                Uuid::new_v4().to_string(),
                assistant_message_id.clone(),
                provider_runtime.provider_id.clone(),
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE conversation SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id.clone()],
        )
        .map_err(|e| e.to_string())?;
    }

    app_handle
        .emit(
            "chat-done",
            agent::ChatDoneEvent {
                conversation_id,
                message_id: assistant_message_id,
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn select_provider_id(conn: &Connection) -> Result<String, String> {
    if let Some(provider) =
        db::provider::get_provider_by_id(conn, db::provider::DEFAULT_PROVIDER_ID)
            .map_err(|e| e.to_string())?
    {
        return Ok(provider.id);
    }

    let providers = db::provider::list_providers(conn).map_err(|e| e.to_string())?;
    providers
        .first()
        .map(|provider| provider.id.clone())
        .ok_or_else(|| "no providers configured; configure at least one provider".to_string())
}

fn load_provider_runtime_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> Result<agent::ProviderRuntime, String> {
    conn.query_row(
        "SELECT p.id, p.base_url, p.model_id
         FROM conversation c
         JOIN provider p ON p.id = c.provider_id
         WHERE c.id = ?1",
        [conversation_id],
        |row| {
            Ok(agent::ProviderRuntime {
                provider_id: row.get(0)?,
                base_url: row.get(1)?,
                model_id: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("conversation not found or provider missing: {conversation_id}"))
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

    #[test]
    fn select_provider_id_prefers_minimax_when_present() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("db/schema.sql"))
            .expect("schema should execute successfully");
        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["other", "Other", "openai", "http://example", "m", 1_i64],
        )
        .expect("insert other provider should succeed");
        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["minimax", "MiniMax", "openai", "http://example", "m", 2_i64],
        )
        .expect("insert minimax provider should succeed");

        let selected = super::select_provider_id(&conn).expect("provider selection should succeed");
        assert_eq!(selected, "minimax");
    }

    #[test]
    fn select_provider_id_falls_back_to_first_provider() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("db/schema.sql"))
            .expect("schema should execute successfully");
        conn.execute(
            "INSERT INTO provider (id, name, type, base_url, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["other", "Other", "openai", "http://example", "m", 1_i64],
        )
        .expect("insert provider should succeed");

        let selected = super::select_provider_id(&conn).expect("provider selection should succeed");
        assert_eq!(selected, "other");
    }

    #[test]
    fn select_provider_id_errors_when_no_provider_exists() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("db/schema.sql"))
            .expect("schema should execute successfully");

        let err = super::select_provider_id(&conn)
            .expect_err("provider selection should fail when no provider exists");
        assert!(err.contains("no providers configured"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db_state = db::init_db(app.handle())?;
            let mut tool_registry = tools::ToolRegistry::new();
            tools::register_default_tools(&mut tool_registry).map_err(std::io::Error::other)?;
            app.manage(db_state);
            app.manage(ToolRegistryState {
                registry: tool_registry,
            });
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            create_conversation,
            list_conversations,
            get_conversation,
            send_message
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
