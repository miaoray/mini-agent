mod agent;
mod approval;
mod commands;
mod db;
pub mod llm;
pub mod tools;

use std::env;
use std::path::PathBuf;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use tauri::async_runtime;
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

/// Safety guard: loop ends primarily when LLM returns final Content; this cap prevents runaway.
const MAX_TOOL_LOOP_STEPS: usize = 20;
const MAX_IDENTICAL_TOOL_CALLS: usize = 2;
const DEFAULT_OPENAI_STYLE_MINIMAX_MODEL: &str = "abab6.5";
const DEFAULT_ANTHROPIC_RUNTIME_MODEL: &str = "MiniMax-M2.5";

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

#[allow(dead_code)]
fn chunk_assistant_content(content: &str, max_chars_per_chunk: usize) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    let max_chars = max_chars_per_chunk.max(1);
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_is_whitespace: Option<bool> = None;

    for ch in content.chars() {
        let is_whitespace = ch.is_whitespace();
        match current_is_whitespace {
            None => {
                current.push(ch);
                current_is_whitespace = Some(is_whitespace);
            }
            Some(kind) if kind == is_whitespace => {
                current.push(ch);
            }
            Some(_) => {
                tokens.push(current);
                current = String::new();
                current.push(ch);
                current_is_whitespace = Some(is_whitespace);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut chunk = String::new();
    let mut chunk_len = 0usize;

    for token in tokens {
        let token_len = token.chars().count();
        if token_len > max_chars {
            if !chunk.is_empty() {
                chunks.push(chunk);
                chunk = String::new();
                chunk_len = 0;
            }

            let mut piece = String::new();
            let mut piece_len = 0usize;
            for ch in token.chars() {
                if piece_len == max_chars {
                    chunks.push(piece);
                    piece = String::new();
                    piece_len = 0;
                }
                piece.push(ch);
                piece_len += 1;
            }
            if !piece.is_empty() {
                chunks.push(piece);
            }
            continue;
        }

        if chunk_len + token_len <= max_chars {
            chunk.push_str(&token);
            chunk_len += token_len;
        } else {
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
            chunk = token;
            chunk_len = token_len;
        }
    }

    if !chunk.is_empty() {
        chunks.push(chunk);
    }

    chunks
}

fn build_tool_dedupe_key(function_name: &str, function_arguments: &str) -> String {
    format!("{function_name}:{function_arguments}")
}

fn should_skip_redundant_tool_call(
    tool_call_counts: &mut HashMap<String, usize>,
    function_name: &str,
    function_arguments: &str,
) -> Option<String> {
    let dedupe_key = build_tool_dedupe_key(function_name, function_arguments);
    let seen = tool_call_counts
        .entry(dedupe_key.clone())
        .and_modify(|count| *count += 1)
        .or_insert(1);
    if *seen > MAX_IDENTICAL_TOOL_CALLS {
        return Some(format!(
            "The tool call {} was already executed {} times with identical arguments. Stop calling it again and provide the best final answer from existing tool results.",
            dedupe_key, MAX_IDENTICAL_TOOL_CALLS
        ));
    }
    None
}

fn build_loop_exhausted_fallback(latest_tool_result: Option<&str>) -> String {
    if let Some(last_result) = latest_tool_result {
        format!(
            "I completed tool calls but could not produce a final model response in time. Latest tool result: {last_result}"
        )
    } else {
        format!(
            "I could not produce a final model response after {MAX_TOOL_LOOP_STEPS} tool steps. Please refine your request and try again."
        )
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
fn clear_all_conversations(state: tauri::State<'_, db::DbState>) -> Result<(), String> {
    let conn = state.connection()?;
    db::conversation::clear_all_conversations(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_messages(
    state: tauri::State<'_, db::DbState>,
    conversation_id: String,
) -> Result<Vec<db::message::Message>, String> {
    let conn = state.connection()?;
    db::message::list_messages_by_conversation(&conn, &conversation_id).map_err(|e| e.to_string())
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
    assistant_message_id: Option<String>,
    user_message_id: Option<String>,
) -> Result<String, String> {
    let assistant_message_id = assistant_message_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let user_message_id = user_message_id.unwrap_or_else(|| Uuid::new_v4().to_string());
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
    async_runtime::spawn(async move {
        if let Err(err) = run_agent_turn(
            &app_handle,
            background_conversation_id.clone(),
            background_message_id.clone(),
            provider_runtime,
            true,
        )
        .await
        {
            eprintln!(
                "[mini-agent][send_message] run_agent_turn error conversation_id={} message_id={} err={}",
                background_conversation_id, background_message_id, err
            );
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

#[tauri::command]
async fn approve_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    approval_id: String,
) -> Result<(), String> {
    let base_dir = resolve_approval_base_dir(&app)?;
    let (resolved_event, provider_runtime) = {
        let conn = state.connection()?;
        let resolved_event = approval::approve_action(&conn, &approval_id, &base_dir)?;
        let provider_runtime =
            load_provider_runtime_for_conversation(&conn, &resolved_event.conversation_id)?;
        (resolved_event, provider_runtime)
    };
    let conversation_id = resolved_event.conversation_id.clone();
    let message_id = resolved_event.message_id.clone();

    app.emit("approval-resolved", resolved_event)
        .map_err(|e| e.to_string())?;

    spawn_resumed_agent_turn(app, conversation_id, message_id, provider_runtime);

    Ok(())
}

#[tauri::command]
async fn reject_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    approval_id: String,
) -> Result<(), String> {
    let (resolved_event, provider_runtime) = {
        let conn = state.connection()?;
        let resolved_event = approval::reject_action(&conn, &approval_id)?;
        let provider_runtime =
            load_provider_runtime_for_conversation(&conn, &resolved_event.conversation_id)?;
        (resolved_event, provider_runtime)
    };
    let conversation_id = resolved_event.conversation_id.clone();
    let message_id = resolved_event.message_id.clone();

    app.emit("approval-resolved", resolved_event)
        .map_err(|e| e.to_string())?;

    spawn_resumed_agent_turn(app, conversation_id, message_id, provider_runtime);

    Ok(())
}

/**
 * 获取 debug mode 状态
 * 
 * @param state 数据库状态
 * @return Result<bool, String> debug mode 是否开启
 */
#[tauri::command]
fn get_debug_mode(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.connection()?;
    db::debug::is_debug_mode(&conn)
}

/**
 * 设置 debug mode 状态
 * 
 * @param state 数据库状态
 * @param enabled 是否开启 debug mode
 * @return Result<(), String> 操作结果
 */
#[tauri::command]
fn set_debug_mode(
    state: tauri::State<'_, db::DbState>,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.connection()?;
    db::debug::set_debug_mode(&conn, enabled)
}

/**
 * 获取最近的 LLM debug logs
 * 
 * @param state 数据库状态
 * @param limit 限制数量
 * @return Result<Vec<DebugLogEntry>, String> debug logs 列表
 */
#[tauri::command]
fn list_debug_logs(
    state: tauri::State<'_, db::DbState>,
    limit: usize,
) -> Result<Vec<DebugLogEntry>, String> {
    let conn = state.connection()?;
    let logs = db::debug::list_recent_llm_debug_logs(&conn, limit)?;
    Ok(logs
        .into_iter()
        .map(|log| DebugLogEntry {
            id: log.id,
            turn_id: log.turn_id,
            request_json: log.request_json,
            response_json: log.response_json,
            created_at: log.created_at,
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugLogEntry {
    id: String,
    turn_id: String,
    request_json: String,
    response_json: Option<String>,
    created_at: i64,
}

async fn run_agent_turn(
    app_handle: &tauri::AppHandle,
    conversation_id: String,
    assistant_message_id: String,
    provider_runtime: agent::ProviderRuntime,
    insert_placeholder: bool,
) -> Result<(), String> {
    let mut llm_messages = {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        agent::r#loop::build_messages_for_llm(&conn, &conversation_id).map_err(|e| e.to_string())?
    };
    if insert_placeholder {
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
    let debug_mode = {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        db::debug::is_debug_mode(&conn)?
    };
    let mut final_assistant_content = String::new();
    let mut accumulated_content = String::new();
    let mut paused_for_approval = false;
    let mut had_thinking = false;
    let mut tool_call_counts: HashMap<String, usize> = HashMap::new();
    let mut latest_tool_result: Option<String> = None;

    for step in 0..MAX_TOOL_LOOP_STEPS {
        eprintln!(
            "[mini-agent][run_agent_turn] step={} conversation_id={} message_id={}",
            step, conversation_id, assistant_message_id
        );
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

        let request_summary: String = llm_messages
            .iter()
            .map(|m| format!("{}:{}", m.role, m.content.chars().count()))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "[mini-agent][run_agent_turn] step={} request messages_count={} roles_and_len={}",
            step,
            llm_messages.len(),
            request_summary
        );
        if llm_messages.len() <= 4 {
            for (i, m) in llm_messages.iter().enumerate() {
                let preview: String = m.content.chars().take(200).collect();
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} request_msg[{}] role={} content_preview={:?}",
                    step, i, m.role, preview
                );
            }
        } else {
            let last_3: Vec<_> = llm_messages.iter().rev().take(3).collect();
            for (i, m) in last_3.iter().enumerate() {
                let preview: String = m.content.chars().take(200).collect();
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} request_msg[last-{}] role={} content_preview={:?}",
                    step, 2 - i, m.role, preview
                );
            }
        }

        let result = match client
            .chat_completion_with_tools(&provider_runtime.model_id, &llm_messages, &tool_defs)
            .await
        {
            Ok(value) => value,
            Err(llm::minimax::MiniMaxError::MissingResponseContent) => {
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} llm returned missing content; injecting steering prompt",
                    step
                );
                llm_messages.push(llm::minimax::ChatMessage {
                    role: "user".to_string(),
                    content: "Your last response was empty. Based on existing tool results, provide a concise final answer now without additional tool calls unless strictly necessary.".to_string(),
                    content_blocks: None,
                });
                continue;
            }
            Err(err) => return Err(err.to_string()),
        };

        if debug_mode {
            if let (Some(raw_req), Some(raw_resp)) = (&result.raw_request, &result.raw_response) {
                let log_id = Uuid::new_v4().to_string();
                let db_state = app_handle.state::<db::DbState>();
                if let Ok(conn) = db_state.connection() {
                    let _ = db::debug::insert_llm_debug_log(&conn, &log_id, &turn_id, raw_req);
                    let _ = db::debug::update_llm_debug_log_response(&conn, &log_id, raw_resp);
                };
            }
        }

        if let Some(ref thinking) = result.thinking {
            had_thinking = true;
            eprintln!(
                "[mini-agent][chat-thinking] EMIT step={} conversation_id={} message_id={} thinking_len={} preview={:?}",
                step,
                conversation_id,
                assistant_message_id,
                thinking.len(),
                thinking.chars().take(80).collect::<String>()
            );
            let _ = app_handle.emit(
                "chat-thinking",
                agent::ChatThinkingEvent {
                    conversation_id: conversation_id.clone(),
                    message_id: assistant_message_id.clone(),
                    thinking: thinking.clone(),
                },
            );
        }

        match &result.output {
            llm::minimax::ChatCompletionOutput::ToolCalls(tc) => {
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} response type=ToolCalls count={}",
                    step, tc.len()
                );
                for (i, t) in tc.iter().enumerate() {
                    eprintln!(
                        "[mini-agent][run_agent_turn] step={} response_tool[{}] name={} args={}",
                        step, i, t.function_name, t.function_arguments
                    );
                }
            }
            llm::minimax::ChatCompletionOutput::Content(c) => {
                let preview: String = c.chars().take(200).collect();
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} response type=Content len={} preview={:?}",
                    step, c.len(), preview
                );
            }
        }

        let raw_content_blocks = result.raw_content_blocks.clone();
        match result.output {
            llm::minimax::ChatCompletionOutput::ToolCalls(tool_calls) => {
                let mut continue_loop = true;
                let mut executed_tools: Vec<(llm::minimax::ToolCall, String)> = Vec::new();
                for tool_call in &tool_calls {
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
                                        approval_id: approval_id.clone(),
                                        action_type: tool_call.function_name.clone(),
                                        payload: tool_args.clone(),
                                    },
                                )
                                .map_err(|e| e.to_string())?;
                            let pause_content = format!(
                                "Tool call paused for approval: {}({})",
                                tool_call.function_name, tool_call.function_arguments
                            );
                            accumulated_content.push_str(&pause_content);
                            llm_messages.push(llm::minimax::ChatMessage {
                                role: "assistant".to_string(),
                                content: pause_content,
                                content_blocks: None,
                            });
                            paused_for_approval = true;
                            continue_loop = false;
                            break;
                        }
                        ToolBranch::ExecuteImmediately => {
                            if let Some(steering_prompt) = should_skip_redundant_tool_call(
                                &mut tool_call_counts,
                                &tool_call.function_name,
                                &tool_call.function_arguments,
                            ) {
                                llm_messages.push(llm::minimax::ChatMessage {
                                    role: "user".to_string(),
                                    content: steering_prompt,
                                    content_blocks: None,
                                });
                                continue;
                            }

                            let tool_result = {
                                let tool_registry_state = app_handle.state::<ToolRegistryState>();
                                match tool_registry_state
                                    .registry
                                    .execute(&tool_call.function_name, tool_args.clone())
                                    .await
                                {
                                    Ok(s) => s,
                                    Err(e) => {
                                        eprintln!(
                                            "[mini-agent][run_agent_turn] step={} tool={} failed: {}",
                                            step, tool_call.function_name, e
                                        );
                                        format!(
                                            "Tool execution failed: {}. Please try again or proceed manually.",
                                            e
                                        )
                                    }
                                }
                            };
                            eprintln!(
                                "[mini-agent][run_agent_turn] step={} tool={} result_len={}",
                                step,
                                tool_call.function_name,
                                tool_result.len()
                            );
                            latest_tool_result = Some(format!(
                                "{} => {}",
                                tool_call.function_name, tool_result
                            ));
                            let called_tool_text = format!(
                                "Called tool {}({})",
                                tool_call.function_name, tool_call.function_arguments
                            );
                            let delta = format!("{}\n", called_tool_text);
                            accumulated_content.push_str(&delta);
                            executed_tools.push((tool_call.clone(), tool_result));
                        }
                    }
                }

                if continue_loop && !executed_tools.is_empty() {
                    let assistant_content_blocks = raw_content_blocks
                        .as_ref()
                        .map(|blocks| blocks.iter().cloned().collect::<Vec<_>>());
                    let assistant_content_str: String = if let Some(ref blocks) = assistant_content_blocks {
                        let text: String = blocks
                            .iter()
                            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("");
                        if text.is_empty() {
                            executed_tools
                                .iter()
                                .map(|(tc, _)| format!("Called tool {}({})", tc.function_name, tc.function_arguments))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            text
                        }
                    } else {
                        executed_tools
                            .iter()
                            .map(|(tc, _)| format!("Called tool {}({})", tc.function_name, tc.function_arguments))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    llm_messages.push(llm::minimax::ChatMessage {
                        role: "assistant".to_string(),
                        content: assistant_content_str,
                        content_blocks: assistant_content_blocks,
                    });
                    let tool_result_blocks: Vec<serde_json::Value> = executed_tools
                        .iter()
                        .map(|(tc, res)| {
                            serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": tc.id,
                                "content": res
                            })
                        })
                        .collect();
                    let tool_result_content: String = executed_tools
                        .iter()
                        .map(|(tc, res)| format!("Tool result from {}: {}", tc.function_name, res))
                        .collect::<Vec<_>>()
                        .join("\n");
                    llm_messages.push(llm::minimax::ChatMessage {
                        role: "user".to_string(),
                        content: tool_result_content,
                        content_blocks: Some(tool_result_blocks),
                    });
                }

                if !continue_loop {
                    break;
                }
            }
            llm::minimax::ChatCompletionOutput::Content(content) => {
                let trimmed = content.trim();
                let looks_like_tool_echo = trimmed.starts_with("Called tool");
                if content.is_empty() || looks_like_tool_echo {
                    eprintln!(
                        "[mini-agent][run_agent_turn] step={} received empty or tool-echo content (len={}); injecting steering prompt",
                        step, content.len()
                    );
                    llm_messages.push(llm::minimax::ChatMessage {
                        role: "user".to_string(),
                        content: "Your previous response was empty or echoed a tool call. Based on the tool results above, provide a concise final answer to the user now without repeating tool calls.".to_string(),
                        content_blocks: None,
                    });
                    continue;
                }
                let preview: String = content.chars().take(120).collect();
                eprintln!(
                    "[mini-agent][run_agent_turn] step={} received_final_content_len={} preview={:?}",
                    step, content.len(), preview
                );
                let delta = format!("\n\n{}", content);
                accumulated_content.push_str(&delta);
                final_assistant_content = content;
                break;
            }
        }
    }

    if !paused_for_approval && final_assistant_content.is_empty() {
        eprintln!(
            "[mini-agent][run_agent_turn] loop_exhausted conversation_id={} message_id={} latest_tool_result_present={}",
            conversation_id,
            assistant_message_id,
            latest_tool_result.is_some()
        );
        let fallback = build_loop_exhausted_fallback(latest_tool_result.as_deref());
        let delta = format!("\n\n{}", fallback);
        accumulated_content.push_str(&delta);
        final_assistant_content = fallback;
    }

    {
        let db_state = app_handle.state::<db::DbState>();
        let conn = db_state.connection()?;
        let now = now_unix_ts();
        let content_to_persist = if accumulated_content.is_empty() {
            final_assistant_content.clone()
        } else {
            accumulated_content.clone()
        };
        let rows = conn
            .execute(
                "UPDATE message SET content = ?1 WHERE id = ?2",
                params![content_to_persist, assistant_message_id.clone()],
            )
            .map_err(|e| e.to_string())?;
        if rows != 1 {
            eprintln!(
                "[mini-agent][run_agent_turn] WARNING: UPDATE message affected {} rows (expected 1) message_id={} content_len={}",
                rows, assistant_message_id, content_to_persist.len()
            );
        }
        conn.execute(
            "UPDATE conversation SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id.clone()],
        )
        .map_err(|e| e.to_string())?;
    }

    if paused_for_approval {
        return Ok(());
    }

    let content_to_emit = if accumulated_content.is_empty() {
        final_assistant_content.clone()
    } else {
        accumulated_content.clone()
    };
    eprintln!(
        "[mini-agent][run_agent_turn] EMITTING chat-done conversation_id={} message_id={} content_len={}",
        conversation_id,
        assistant_message_id,
        content_to_emit.len()
    );
    app_handle
        .emit(
            "chat-done",
            agent::ChatDoneEvent {
                conversation_id: conversation_id.clone(),
                message_id: assistant_message_id.clone(),
                content: content_to_emit,
                has_thinking: had_thinking,
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
    .map(|mut runtime| {
        runtime.model_id = resolve_runtime_model_id(&runtime.base_url, &runtime.model_id);
        runtime
    })
    .ok_or_else(|| format!("conversation not found or provider missing: {conversation_id}"))
}

fn resolve_runtime_model_id(base_url: &str, stored_model_id: &str) -> String {
    if is_anthropic_like_base_url(base_url) && is_openai_style_default_model(stored_model_id) {
        if let Ok(env_model_id) = env::var("MINIMAX_MODEL_ID") {
            let trimmed = env_model_id.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        return DEFAULT_ANTHROPIC_RUNTIME_MODEL.to_string();
    }

    stored_model_id.to_string()
}

fn is_anthropic_like_base_url(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("/anthropic")
}

fn is_openai_style_default_model(model_id: &str) -> bool {
    let normalized = model_id.trim().to_ascii_lowercase();
    normalized == DEFAULT_OPENAI_STYLE_MINIMAX_MODEL || normalized.starts_with("abab")
}

fn now_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn resolve_approval_base_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .home_dir()
        .or_else(|_| app.path().document_dir())
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("failed to resolve approval base directory: {e}"))
}

fn spawn_resumed_agent_turn(
    app: tauri::AppHandle,
    conversation_id: String,
    assistant_message_id: String,
    provider_runtime: agent::ProviderRuntime,
) {
    let app_handle = app.clone();
    async_runtime::spawn(async move {
        if let Err(err) = run_agent_turn(
            &app_handle,
            conversation_id.clone(),
            assistant_message_id.clone(),
            provider_runtime,
            false,
        )
        .await
        {
            let _ = app_handle.emit(
                "chat-error",
                agent::ChatErrorEvent {
                    conversation_id,
                    message_id: assistant_message_id,
                    message: err,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use rusqlite::Connection;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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

    #[test]
    fn chunk_assistant_content_returns_non_empty_chunks_and_reconstructs_original() {
        let content = "Hello   world!\nThis is a long-ish response with punctuation...";
        let chunks = super::chunk_assistant_content(content, 8);

        assert!(!chunks.is_empty(), "expected at least one chunk");
        assert!(chunks.iter().all(|c| !c.is_empty()));
        assert_eq!(chunks.join(""), content);
    }

    #[test]
    fn should_skip_redundant_tool_call_allows_then_blocks_identical_calls() {
        let mut counts = HashMap::new();
        let name = "web_search";
        let args = r#"{"query":"qcon2025 beijing"}"#;

        let first = super::should_skip_redundant_tool_call(&mut counts, name, args);
        let second = super::should_skip_redundant_tool_call(&mut counts, name, args);
        let third = super::should_skip_redundant_tool_call(&mut counts, name, args);

        assert!(first.is_none(), "first identical tool call should run");
        assert!(second.is_none(), "second identical tool call should run");
        assert!(
            third.is_some(),
            "third identical tool call should be blocked"
        );
        assert!(
            third
                .expect("third call should produce steering prompt")
                .contains("already executed 2 times")
        );
    }

    #[test]
    fn build_loop_exhausted_fallback_uses_latest_tool_result_when_present() {
        let msg = super::build_loop_exhausted_fallback(Some("web_search => found 3 links"));
        assert!(msg.contains("Latest tool result: web_search => found 3 links"));
    }

    #[test]
    fn build_loop_exhausted_fallback_handles_missing_tool_result() {
        let msg = super::build_loop_exhausted_fallback(None);
        assert!(msg.contains("after 20 tool steps"));
    }

    #[test]
    fn final_assistant_content_persists_to_message_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(include_str!("db/schema.sql"))
            .expect("schema should execute successfully");
        crate::db::provider::insert_default_provider(&conn).expect("provider insert should succeed");
        conn.execute(
            "INSERT INTO conversation (id, title, provider_id, user_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            rusqlite::params!["conv-1", "New Chat", "minimax", 1_i64, 1_i64],
        )
        .expect("conversation insert should succeed");

        let msg_id = "assistant-msg-1";
        conn.execute(
            "INSERT INTO message (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, 'assistant', '', ?3)",
            rusqlite::params![msg_id, "conv-1", 2_i64],
        )
        .expect("assistant message insert should succeed");

        let final_content = "QCon 2025北京三大要点：1.技术前沿 2.实践分享 3.生态共建";
        let rows = conn
            .execute(
                "UPDATE message SET content = ?1 WHERE id = ?2",
                rusqlite::params![final_content, msg_id],
            )
            .expect("update should succeed");
        assert_eq!(rows, 1, "exactly one message row should be updated");

        let stored: String = conn
            .query_row(
                "SELECT content FROM message WHERE id = ?1",
                rusqlite::params![msg_id],
                |row| row.get(0),
            )
            .expect("message should exist");
        assert_eq!(stored, final_content);
    }

    #[test]
    fn resolve_runtime_model_id_uses_anthropic_fallback_for_openai_style_default() {
        let _guard = env_lock().lock().expect("env lock should acquire");
        let _ = std::env::remove_var("MINIMAX_MODEL_ID");
        let resolved = super::resolve_runtime_model_id("https://api.minimaxi.com/anthropic", "abab6.5");
        assert_eq!(resolved, "MiniMax-M2.5");
    }

    #[test]
    fn resolve_runtime_model_id_prefers_env_override_in_anthropic_mode() {
        let _guard = env_lock().lock().expect("env lock should acquire");
        std::env::set_var("MINIMAX_MODEL_ID", "MiniMax-M2.5-Pro");
        let resolved = super::resolve_runtime_model_id("https://api.minimaxi.com/anthropic", "abab6.5");
        assert_eq!(resolved, "MiniMax-M2.5-Pro");
        let _ = std::env::remove_var("MINIMAX_MODEL_ID");
    }

    #[test]
    fn resolve_runtime_model_id_keeps_existing_model_for_openai_mode() {
        let resolved = super::resolve_runtime_model_id("https://api.minimax.chat/v1", "abab6.5");
        assert_eq!(resolved, "abab6.5");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();
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
            clear_all_conversations,
            list_messages,
            get_conversation,
            send_message,
            approve_action,
            reject_action,
            get_debug_mode,
            set_debug_mode,
            list_debug_logs,
            commands::check_config::check_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
