// ============================================================
// Settings persistence
// ============================================================

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::Database;
use crate::config::types::AppConfig;

const SETTINGS_KEY: &str = "app_config";
const REVISION_KEY: &str = "app_config_revision";

#[derive(Debug, thiserror::Error)]
pub(crate) enum SettingsSaveError {
    #[error("全局配置已有更新，请重新载入后保存")]
    RevisionConflict { current: u64 },
    #[error("{0}")]
    Storage(String),
}

impl From<rusqlite::Error> for SettingsSaveError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

fn settings_revision_on(conn: &Connection) -> rusqlite::Result<u64> {
    Ok(conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM settings WHERE key=?1",
            [REVISION_KEY],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

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

    pub(crate) fn get_settings_revision(&self) -> Result<u64, String> {
        settings_revision_on(&self.conn()).map_err(|error| error.to_string())
    }

    pub fn save_settings(&self, config: &AppConfig) -> Result<(), String> {
        self.save_settings_with_revision(config, None)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn save_settings_with_revision(
        &self,
        config: &AppConfig,
        expected_revision: Option<u64>,
    ) -> Result<u64, SettingsSaveError> {
        // Defense-in-depth: refuse to persist plaintext API keys. Values must
        // live in the SecretStore behind a SecretRef.
        if !config.model.api_key.is_empty()
            || !config.model.embedding_api_key.is_empty()
            || !config.voice.stt.api_key.is_empty()
            || !config.voice.tts.api_key.is_empty()
        {
            return Err(SettingsSaveError::Storage(
                "SecretPersistenceViolation: API keys must be stored via SecretRef".to_string(),
            ));
        }
        let json = serde_json::to_string(config)
            .map_err(|error| SettingsSaveError::Storage(error.to_string()))?;
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = settings_revision_on(&tx)?;
        if expected_revision.is_some_and(|expected| expected != current) {
            return Err(SettingsSaveError::RevisionConflict { current });
        }
        let revision = current + 1;
        tx.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![SETTINGS_KEY, json],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![REVISION_KEY, revision],
        )?;
        tx.commit()?;
        Ok(revision)
    }
}
