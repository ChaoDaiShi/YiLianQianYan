use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Database;
use crate::config::types::ModelConfig;
use crate::secret::SecretRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelInput {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub model: String,
    pub base_url: String,
    pub api_format: String,
    pub api_key_ref: String,
    pub api_key_env: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub invoke_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelRow {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub model: String,
    pub base_url: String,
    pub api_format: String,
    pub api_key_ref: String,
    pub api_key_env: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub invoke_timeout_ms: u64,
    pub active: bool,
    pub verified_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl LlmModelRow {
    pub fn to_model_config(&self) -> ModelConfig {
        ModelConfig {
            provider: self.provider.clone(),
            name: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key: String::new(),
            api_key_env: self.api_key_env.clone(),
            api_key_ref: Some(SecretRef::new(self.api_key_ref.clone())),
            clear_api_key: false,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            invoke_timeout_ms: self.invoke_timeout_ms,
            ..ModelConfig::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsageInput {
    pub id: String,
    pub model_id: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub recorded_at: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmUsageDay {
    pub date: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LlmUsageReport {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub days: Vec<LlmUsageDay>,
}

impl Database {
    pub fn create_llm_model(&self, input: &LlmModelInput) -> Result<LlmModelRow, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO llm_models (
                id, provider, label, model, base_url, api_format, api_key_ref,
                api_key_env, temperature, max_tokens, invoke_timeout_ms,
                active, verified_at, last_error, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, NULL, NULL, ?12, ?12)",
            params![
                input.id,
                input.provider,
                input.label,
                input.model,
                input.base_url,
                input.api_format,
                input.api_key_ref,
                input.api_key_env,
                input.temperature,
                input.max_tokens,
                input.invoke_timeout_ms,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        self.get_llm_model(&input.id)
            .and_then(|model| model.ok_or_else(|| "model was not created".to_string()))
    }

    pub fn list_llm_models(&self) -> Result<Vec<LlmModelRow>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, provider, label, model, base_url, api_format, api_key_ref,
                        api_key_env, temperature, max_tokens, invoke_timeout_ms,
                        active, verified_at, last_error, created_at, updated_at
                 FROM llm_models ORDER BY active DESC, updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], map_llm_model)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    pub fn get_llm_model(&self, id: &str) -> Result<Option<LlmModelRow>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, provider, label, model, base_url, api_format, api_key_ref,
                        api_key_env, temperature, max_tokens, invoke_timeout_ms,
                        active, verified_at, last_error, created_at, updated_at
                 FROM llm_models WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        match stmt.query_row(params![id], map_llm_model) {
            Ok(model) => Ok(Some(model)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn update_llm_model(&self, input: &LlmModelInput) -> Result<LlmModelRow, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        let changed = conn
            .execute(
                "UPDATE llm_models SET provider = ?2, label = ?3, model = ?4,
                    base_url = ?5, api_format = ?6, api_key_ref = ?7, api_key_env = ?8,
                    temperature = ?9, max_tokens = ?10, invoke_timeout_ms = ?11,
                    verified_at = NULL, last_error = NULL, updated_at = ?12
                 WHERE id = ?1",
                params![
                    input.id,
                    input.provider,
                    input.label,
                    input.model,
                    input.base_url,
                    input.api_format,
                    input.api_key_ref,
                    input.api_key_env,
                    input.temperature,
                    input.max_tokens,
                    input.invoke_timeout_ms,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("model not found".to_string());
        }
        drop(conn);
        self.get_llm_model(&input.id)
            .and_then(|model| model.ok_or_else(|| "model was not updated".to_string()))
    }

    pub fn delete_llm_model(&self, id: &str) -> Result<(), String> {
        self.conn()
            .execute("DELETE FROM llm_models WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn activate_llm_model(&self, id: &str) -> Result<(), String> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM llm_models WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("model not found".to_string());
        }
        tx.execute("UPDATE llm_models SET active = 0", [])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE llm_models SET active = 1, updated_at = ?2 WHERE id = ?1",
            params![id, chrono::Utc::now().timestamp_millis()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn get_active_llm_model(&self) -> Result<Option<LlmModelRow>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, provider, label, model, base_url, api_format, api_key_ref,
                        api_key_env, temperature, max_tokens, invoke_timeout_ms,
                        active, verified_at, last_error, created_at, updated_at
                 FROM llm_models WHERE active = 1 ORDER BY updated_at DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        match stmt.query_row([], map_llm_model) {
            Ok(model) => Ok(Some(model)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set_llm_model_verification(
        &self,
        id: &str,
        verified_at: Option<i64>,
        last_error: Option<&str>,
    ) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE llm_models SET verified_at = ?2, last_error = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, verified_at, last_error, chrono::Utc::now().timestamp_millis()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_llm_usage(&self, input: &LlmUsageInput) -> Result<(), String> {
        self.conn()
            .execute(
                "INSERT INTO llm_usage_events (
                    id, model_id, provider, model, prompt_tokens, completion_tokens,
                    total_tokens, recorded_at, source
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    input.id,
                    input.model_id,
                    input.provider,
                    input.model,
                    input.prompt_tokens,
                    input.completion_tokens,
                    input.total_tokens,
                    input.recorded_at,
                    input.source,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn aggregate_llm_usage(
        &self,
        model_id: Option<&str>,
        from: i64,
        to: i64,
    ) -> Result<LlmUsageReport, String> {
        let conn = self.conn();
        let mut report = conn
            .query_row(
                "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0),
                        COALESCE(SUM(total_tokens), 0)
                 FROM llm_usage_events
                 WHERE recorded_at >= ?1 AND recorded_at < ?2
                   AND (?3 IS NULL OR model_id = ?3)",
                params![from, to, model_id],
                |row| {
                    Ok(LlmUsageReport {
                        prompt_tokens: row.get::<_, i64>(0)? as u64,
                        completion_tokens: row.get::<_, i64>(1)? as u64,
                        total_tokens: row.get::<_, i64>(2)? as u64,
                        days: Vec::new(),
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT strftime('%Y-%m-%d', recorded_at / 1000, 'unixepoch') AS day,
                        SUM(prompt_tokens), SUM(completion_tokens), SUM(total_tokens)
                 FROM llm_usage_events
                 WHERE recorded_at >= ?1 AND recorded_at < ?2
                   AND (?3 IS NULL OR model_id = ?3)
                 GROUP BY day ORDER BY day ASC LIMIT 366",
            )
            .map_err(|e| e.to_string())?;
        report.days = stmt
            .query_map(params![from, to, model_id], |row| {
                Ok(LlmUsageDay {
                    date: row.get(0)?,
                    prompt_tokens: row.get::<_, i64>(1)? as u64,
                    completion_tokens: row.get::<_, i64>(2)? as u64,
                    total_tokens: row.get::<_, i64>(3)? as u64,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(report)
    }
}

fn map_llm_model(row: &rusqlite::Row<'_>) -> rusqlite::Result<LlmModelRow> {
    Ok(LlmModelRow {
        id: row.get(0)?,
        provider: row.get(1)?,
        label: row.get(2)?,
        model: row.get(3)?,
        base_url: row.get(4)?,
        api_format: row.get(5)?,
        api_key_ref: row.get(6)?,
        api_key_env: row.get(7)?,
        temperature: row.get(8)?,
        max_tokens: row.get(9)?,
        invoke_timeout_ms: row.get(10)?,
        active: row.get::<_, i64>(11)? != 0,
        verified_at: row.get(12)?,
        last_error: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}
