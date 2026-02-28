use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

#[allow(dead_code)]
pub struct DbState {
    connection: Mutex<Connection>,
}

#[allow(dead_code)]
impl DbState {
    pub fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .expect("database mutex should not be poisoned")
    }
}

pub fn init_db(app: &AppHandle) -> Result<DbState, Box<dyn Error>> {
    let app_data_dir: PathBuf = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;

    let db_path = app_data_dir.join("mini-agent.sqlite3");
    let connection = Connection::open(db_path)?;
    connection.execute_batch(include_str!("schema.sql"))?;

    Ok(DbState {
        connection: Mutex::new(connection),
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

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
}
