mod agent;
mod db;
pub mod llm;
pub mod tools;

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use tauri::{Emitter, Manager};
use uuid::Uuid;

pub mod test_support {
    use rusqlite::Connection;

    use crate::llm::minimax::ChatMessage;

    pub fn build_messages_for_llm(
        conn: &Connection,
        conversation_id: &str,
    ) -> Result<Vec<ChatMessage>, rusqlite::Error> {
        crate::agent::r#loop::build_messages_for_llm(conn, conversation_id)
    }
}

#[allow(dead_code)]
pub struct ToolRegistryState {
    registry: tools::ToolRegistry,
}

const MAX_TOOL_LOOP_STEPS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolBranch {
    NeedsApproval,
    ExecuteImmediately,
}

fn tool_branch_for_name(name: &str) -> ToolBranch {
    match name {
        "create_directory" | "write_file" => ToolBranch::NeedsApproval,
        _ => ToolBranch::ExecuteImmediately,
    }
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
    let mut llm_messages = {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        agent::r#loop::build_messages_for_llm(&conn, &conversation_id).map_err(|e| e.to_string())?
    };
    {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, 'assistant', '', ?3)",
            params![
                assistant_message_id.clone(),
                conversation_id.clone(),
                now_unix_ts()
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    let api_key = env::var("MINIMAX_API_KEY")
        .map_err(|_| "missing required environment variable: MINIMAX_API_KEY".to_string())?;
    let client = llm::minimax::MiniMaxClient::new(api_key, provider_runtime.base_url.clone());
    let tool_defs = {
        let tool_registry_state = app_handle.state::<ToolRegistryState>();
        tool_registry_state.registry.get_tools_for_llm()
    };
    let mut final_assistant_content = String::new();
    let mut paused_for_approval = false;

    for _step in 0..MAX_TOOL_LOOP_STEPS {
        let turn_id = Uuid::new_v4().to_string();
        {
            let db_state = app_handle.state::<db::DbState>();
            let conn = db_state.connection()?;
            let now = now_unix_ts();
            conn.execute(
                "INSERT INTO agent_turn (id, message_id, provider_id, prompt_tokens, completion_tokens, created_at)
                 VALUES (?1, ?2, ?3, NULL, NULL, ?4)",
                params![
                    turn_id.clone(),
                    assistant_message_id.clone(),
                    provider_runtime.provider_id.clone(),
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        let completion = client
            .chat_completion_with_tools(&provider_runtime.model_id, &llm_messages, &tool_defs)
            .await
            .map_err(|e| e.to_string())?;

        match completion {
            llm::minimax::ChatCompletionOutput::ToolCalls(tool_calls) => {
                let mut continue_loop = true;
                for tool_call in tool_calls {
                    let tool_args: Value = serde_json::from_str(&tool_call.function_arguments)
                        .map_err(|e| format!("failed to parse tool call arguments: {e}"))?;

                    match tool_branch_for_name(&tool_call.function_name) {
                        ToolBranch::NeedsApproval => {
                            let approval_id = Uuid::new_v4().to_string();
                            let now = now_unix_ts();
                            {
                                let db_state = app_handle.state::<db::DbState>();
                                let conn = db_state.connection()?;
                                conn.execute(
                                    "INSERT INTO pending_approval (id, conversation_id, turn_id, action_type, payload_json, status, created_at)
                                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                                    params![
                                        approval_id.clone(),
                                        conversation_id.clone(),
                                        turn_id.clone(),
                                        tool_call.function_name.clone(),
                                        tool_args.to_string(),
                                        now
                                    ],
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            app_handle
                                .emit(
                                    "pending-approval",
                                    agent::PendingApprovalEvent {
                                        conversation_id: conversation_id.clone(),
                                        message_id: assistant_message_id.clone(),
                                        approval_id,
                                        action_type: tool_call.function_name.clone(),
                                        payload: tool_args.clone(),
                                    },
                                )
                                .map_err(|e| e.to_string())?;
                            llm_messages.push(llm::minimax::ChatMessage {
                                role: "assistant".to_string(),
                                content: format!(
                                    "Tool call paused for approval: {}({})",
                                    tool_call.function_name, tool_call.function_arguments
                                ),
                            });
                            paused_for_approval = true;
                            continue_loop = false;
                            break;
                        }
                        ToolBranch::ExecuteImmediately => {
                            let tool_result = {
                                let tool_registry_state = app_handle.state::<ToolRegistryState>();
                                tool_registry_state
                                    .registry
                                    .execute(&tool_call.function_name, tool_args.clone())
                                    .await?
                            };
                            llm_messages.push(llm::minimax::ChatMessage {
                                role: "assistant".to_string(),
                                content: format!(
                                    "Called tool {}({})",
                                    tool_call.function_name, tool_call.function_arguments
                                ),
                            });
                            llm_messages.push(llm::minimax::ChatMessage {
                                role: "user".to_string(),
                                content: format!(
                                    "Tool result from {}: {}",
                                    tool_call.function_name, tool_result
                                ),
                            });
                        }
                    }
                }

                if !continue_loop {
                    break;
                }
            }
            llm::minimax::ChatCompletionOutput::Content(content) => {
                if content.is_empty() {
                    return Err("assistant response content was empty".to_string());
                }
                final_assistant_content = client
                    .chat_completion_stream_with_callback(
                        &provider_runtime.model_id,
                        &llm_messages,
                        |delta| {
                            app_handle
                                .emit(
                                    "chat-delta",
                                    agent::ChatDeltaEvent {
                                        conversation_id: conversation_id.clone(),
                                        message_id: assistant_message_id.clone(),
                                        delta: delta.to_string(),
                                    },
                                )
                                .map_err(|e| e.to_string())
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                break;
            }
        }
    }

    if !paused_for_approval && final_assistant_content.is_empty() {
        return Err(format!(
            "agent turn reached max tool loop steps ({MAX_TOOL_LOOP_STEPS}) without final assistant content"
        ));
    }

    {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        let now = now_unix_ts();
        conn.execute(
            "UPDATE message SET content = ?1 WHERE id = ?2",
            params![final_assistant_content.clone(), assistant_message_id.clone()],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE conversation SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id.clone()],
        )
        .map_err(|e| e.to_string())?;
    }

    if paused_for_approval {
        return Ok(());
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

    #[test]
    fn tool_branch_for_name_routes_approval_tools_to_pending_path() {
        assert_eq!(
            super::tool_branch_for_name("create_directory"),
            super::ToolBranch::NeedsApproval
        );
        assert_eq!(
            super::tool_branch_for_name("write_file"),
            super::ToolBranch::NeedsApproval
        );
        assert_eq!(
            super::tool_branch_for_name("web_search"),
            super::ToolBranch::ExecuteImmediately
        );
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
