mod db;
pub mod llm;

use rusqlite::Connection;
use tauri::Manager;

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
            app.manage(db_state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            create_conversation,
            list_conversations,
            get_conversation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
