// ============================================================
// Settings persistence
// ============================================================

use rusqlite::params;

use super::Database;
use crate::config::types::AppConfig;

const SETTINGS_KEY: &str = "app_config";

impl Database {
    pub fn get_settings(&self) -> Result<AppConfig, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| e.to_string())?;
        let result: Result<String, _> = stmt.query_row(params![SETTINGS_KEY], |row| row.get(0));
        match result {
            Ok(json) => {
                serde_json::from_str(&json).map_err(|e| format!("Failed to parse settings: {}", e))
            }
            Err(_) => Ok(AppConfig::default()),
        }
    }

    pub fn save_settings(&self, config: &AppConfig) -> Result<(), String> {
        let conn = self.conn();
        let json = serde_json::to_string(config)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![SETTINGS_KEY, json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
