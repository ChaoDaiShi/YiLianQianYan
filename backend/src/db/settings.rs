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
        // Defense-in-depth: refuse to persist plaintext API keys. Values must
        // live in the SecretStore behind a SecretRef.
        if !config.model.api_key.is_empty()
            || !config.model.embedding_api_key.is_empty()
            || !config.voice.stt.api_key.is_empty()
            || !config.voice.tts.api_key.is_empty()
        {
            return Err(
                "SecretPersistenceViolation: API keys must be stored via SecretRef".to_string(),
            );
        }
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
