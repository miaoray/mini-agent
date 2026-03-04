use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

pub mod conversation;
pub mod debug;
pub mod message;
pub mod provider;

const DB_FILENAME: &str = "mini-agent.db";

#[allow(dead_code)]
pub struct DbState {
    connection: Mutex<Connection>,
}

#[allow(dead_code)]
impl DbState {
    pub fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "database mutex is poisoned".to_string())
    }
}

pub fn init_db(app: &AppHandle) -> Result<DbState, Box<dyn Error>> {
    let app_data_dir: PathBuf = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;

    let db_path = app_data_dir.join(DB_FILENAME);
    let connection = Connection::open(db_path)?;
    connection.execute_batch(include_str!("schema.sql"))?;
    provider::insert_default_provider(&connection)?;

    Ok(DbState {
        connection: Mutex::new(connection),
    })
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Mutex;

    use rusqlite::Connection;

    #[derive(Debug)]
    struct ColumnInfo {
        column_type: String,
        not_null: bool,
        default_value: Option<String>,
        is_primary_key: bool,
    }

    fn column_info(connection: &Connection, table: &str, column: &str) -> ColumnInfo {
        connection
            .query_row(
                &format!(
                    "SELECT type, \"notnull\", dflt_value, pk FROM pragma_table_info('{table}') WHERE name = ?1"
                ),
                [column],
                |row| {
                    Ok(ColumnInfo {
                        column_type: row.get(0)?,
                        not_null: row.get::<_, i64>(1)? == 1,
                        default_value: row.get(2)?,
                        is_primary_key: row.get::<_, i64>(3)? == 1,
                    })
                },
            )
            .expect("column should exist")
    }

    fn fk_count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .expect("foreign key query should succeed")
    }

    #[test]
    fn schema_creates_conversation_table() {
        let connection = Connection::open_in_memory().expect("in-memory sqlite should open");
        connection
            .execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");

        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'conversation'",
                [],
                |row| row.get(0),
            )
            .expect("table existence query should succeed");

        assert_eq!(table_count, 1);
    }

    #[test]
    fn schema_matches_task2_contract() {
        let connection = Connection::open_in_memory().expect("in-memory sqlite should open");
        connection
            .execute_batch(include_str!("schema.sql"))
            .expect("schema should execute successfully");

        let provider_id = column_info(&connection, "provider", "id");
        assert_eq!(provider_id.column_type, "TEXT");
        assert!(provider_id.is_primary_key);

        let provider_name = column_info(&connection, "provider", "name");
        assert!(provider_name.not_null);

        let provider_base_url = column_info(&connection, "provider", "base_url");
        assert_eq!(provider_base_url.column_type, "TEXT");
        assert!(provider_base_url.not_null);

        let conversation_title = column_info(&connection, "conversation", "title");
        assert_eq!(conversation_title.column_type, "TEXT");
        assert!(conversation_title.not_null);
        assert_eq!(conversation_title.default_value.as_deref(), Some("'New Chat'"));

        let message_created_at = column_info(&connection, "message", "created_at");
        assert_eq!(message_created_at.column_type, "INTEGER");
        assert!(message_created_at.not_null);

        let tool_description = column_info(&connection, "tool", "description");
        assert!(tool_description.not_null);

        let tool_schema_json = column_info(&connection, "tool", "schema_json");
        assert!(tool_schema_json.not_null);

        let tool_impl_ref = column_info(&connection, "tool", "impl_ref");
        assert!(tool_impl_ref.not_null);

        let pending_status = column_info(&connection, "pending_approval", "status");
        assert_eq!(pending_status.default_value.as_deref(), Some("'pending'"));

        assert_eq!(fk_count(&connection, "conversation"), 1);
        assert_eq!(fk_count(&connection, "message"), 1);
        assert_eq!(fk_count(&connection, "agent_turn"), 2);
        assert_eq!(fk_count(&connection, "tool_invocation"), 2);
        assert_eq!(fk_count(&connection, "pending_approval"), 2);
    }

    #[test]
    fn db_filename_matches_plan() {
        assert_eq!(super::DB_FILENAME, "mini-agent.db");
    }

    #[test]
    fn connection_returns_error_when_mutex_is_poisoned() {
        let state = super::DbState {
            connection: Mutex::new(
                Connection::open_in_memory().expect("in-memory sqlite should open"),
            ),
        };

        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = state
                .connection
                .lock()
                .expect("mutex lock should succeed before poison");
            panic!("intentional panic to poison mutex");
        }));

        let result = state.connection();
        assert!(result.is_err());
        assert_eq!(
            result.err().as_deref(),
            Some("database mutex is poisoned")
        );
    }
}
