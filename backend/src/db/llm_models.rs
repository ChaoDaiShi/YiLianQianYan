use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Database;
use crate::config::types::ModelConfig;
use crate::integrations::secret::SecretRef;

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
pub struct LlmUsageModel {
    pub model_id: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub known_usage_count: u64,
    pub unknown_usage_count: u64,
    pub failed_attempt_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LlmUsageReport {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub days: Vec<LlmUsageDay>,
    pub known_usage_count: u64,
    pub unknown_usage_count: u64,
    pub failed_attempt_count: u64,
    pub pending_attempt_count: u64,
    pub models: Vec<LlmUsageModel>,
    pub timezone_offset_minutes: i32,
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
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let deleted = tx
            .execute("DELETE FROM llm_models WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        if deleted > 0 {
            super::connectors::remove_connector_entity_on(&tx, "model", id)
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
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
        self.aggregate_llm_usage_in_timezone(model_id, from, to, 0)
    }
    pub fn aggregate_llm_usage_in_timezone(
        &self,
        model_id: Option<&str>,
        from: i64,
        to: i64,
        offset_minutes: i32,
    ) -> Result<LlmUsageReport, String> {
        if !(-840..=840).contains(&offset_minutes) {
            return Err("invalid_timezone_offset".into());
        }
        let conn = self.conn();
        // Basic profiles are not llm_models rows: project their reported token
        // usage into the read model without inserting invalid foreign keys.
        // Image counts and other non-token units remain in the attempt payload.
        let usage_source = "WITH usage_events AS (
            SELECT id,model_id,provider,model,prompt_tokens,completion_tokens,total_tokens,recorded_at FROM llm_usage_events
            UNION ALL
            SELECT a.id,a.model_id,COALESCE(json_extract(account.payload,'$.provider_id'),''),COALESCE(json_extract(p.payload,'$.model'),a.model_id),
                json_extract(a.usage,'$.prompt_tokens'),json_extract(a.usage,'$.completion_tokens'),json_extract(a.usage,'$.total_tokens'),COALESCE(a.ended_at,a.started_at)
            FROM model_pool_attempts a LEFT JOIN basic_service_profiles p ON p.id=a.model_id
            LEFT JOIN provider_accounts account ON account.id=json_extract(p.payload,'$.provider_account_id')
            WHERE json_type(a.usage,'$.prompt_tokens')='integer' AND json_extract(a.usage,'$.prompt_tokens')>=0
              AND json_type(a.usage,'$.completion_tokens')='integer' AND json_extract(a.usage,'$.completion_tokens')>=0
              AND json_type(a.usage,'$.total_tokens')='integer' AND json_extract(a.usage,'$.total_tokens')>=0
              AND NOT EXISTS(SELECT 1 FROM llm_usage_events e WHERE e.id=a.id)
        )";
        let mut report = conn
            .query_row(
                &format!("{usage_source} SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0),
                        COALESCE(SUM(total_tokens), 0)
                 FROM usage_events
                 WHERE recorded_at >= ?1 AND recorded_at < ?2
                   AND (?3 IS NULL OR model_id = ?3)"),
                params![from, to, model_id],
                |row| {
                    Ok(LlmUsageReport {
                        prompt_tokens: row.get::<_, i64>(0)? as u64,
                        completion_tokens: row.get::<_, i64>(1)? as u64,
                        total_tokens: row.get::<_, i64>(2)? as u64,
                        ..LlmUsageReport::default()
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                &format!("{usage_source} SELECT strftime('%Y-%m-%d', (recorded_at + ?4 * 60000) / 1000, 'unixepoch') AS day,
                        SUM(prompt_tokens), SUM(completion_tokens), SUM(total_tokens)
                 FROM usage_events
                 WHERE recorded_at >= ?1 AND recorded_at < ?2
                   AND (?3 IS NULL OR model_id = ?3)
                 GROUP BY day ORDER BY day ASC LIMIT 366"),
            )
            .map_err(|e| e.to_string())?;
        report.days = stmt
            .query_map(params![from, to, model_id, offset_minutes], |row| {
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
        report.timezone_offset_minutes = offset_minutes;
        report.known_usage_count = conn.query_row(&format!("{usage_source} SELECT COUNT(*) FROM usage_events WHERE recorded_at>=?1 AND recorded_at<?2 AND (?3 IS NULL OR model_id=?3)"),params![from,to,model_id],|row|row.get::<_,i64>(0)).map_err(|e|e.to_string())? as u64;
        let mut stmt = conn.prepare(&format!("{usage_source} SELECT e.model_id,e.provider,e.model,SUM(e.prompt_tokens),SUM(e.completion_tokens),SUM(e.total_tokens),COUNT(*) FROM usage_events e WHERE e.recorded_at>=?1 AND e.recorded_at<?2 AND (?3 IS NULL OR e.model_id=?3) GROUP BY e.model_id,e.provider,e.model ORDER BY SUM(e.total_tokens) DESC")).map_err(|e|e.to_string())?;
        report.models = stmt
            .query_map(params![from, to, model_id], |row| {
                Ok(LlmUsageModel {
                    model_id: row.get(0)?,
                    provider: row.get(1)?,
                    model: row.get(2)?,
                    prompt_tokens: row.get::<_, i64>(3)? as u64,
                    completion_tokens: row.get::<_, i64>(4)? as u64,
                    total_tokens: row.get::<_, i64>(5)? as u64,
                    known_usage_count: row.get::<_, i64>(6)? as u64,
                    unknown_usage_count: 0,
                    failed_attempt_count: 0,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut attempts = conn.prepare("SELECT a.model_id,COALESCE(m.provider,json_extract(account.payload,'$.provider_id'),''),COALESCE(m.model,json_extract(p.payload,'$.model'),a.model_id),SUM(CASE WHEN a.ended_at IS NOT NULL AND a.usage IS NULL THEN 1 ELSE 0 END),SUM(CASE WHEN a.failure IS NOT NULL THEN 1 ELSE 0 END),SUM(CASE WHEN a.ended_at IS NULL THEN 1 ELSE 0 END) FROM model_pool_attempts a LEFT JOIN llm_models m ON m.id=a.model_id LEFT JOIN basic_service_profiles p ON p.id=a.model_id LEFT JOIN provider_accounts account ON account.id=json_extract(p.payload,'$.provider_account_id') WHERE a.started_at>=?1 AND a.started_at<?2 AND (?3 IS NULL OR a.model_id=?3) GROUP BY a.model_id").map_err(|e|e.to_string())?;
        let groups = attempts
            .query_map(params![from, to, model_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                    row.get::<_, i64>(5)? as u64,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (id, provider, model, unknown, failed, pending) in groups {
            report.unknown_usage_count += unknown;
            report.failed_attempt_count += failed;
            report.pending_attempt_count += pending;
            if let Some(entry) = report.models.iter_mut().find(|entry| entry.model_id == id) {
                entry.unknown_usage_count = unknown;
                entry.failed_attempt_count = failed;
            } else {
                report.models.push(LlmUsageModel {
                    model_id: id,
                    provider,
                    model,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    known_usage_count: 0,
                    unknown_usage_count: unknown,
                    failed_attempt_count: failed,
                });
            }
        }
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

#[cfg(test)]
mod monitor_usage_tests {
    use super::*;
    #[test]
    fn basic_pool_tokens_join_usage_without_counting_image_units_as_tokens() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-20T16:30:00Z")
            .unwrap()
            .timestamp_millis();
        db.save_provider_account(&crate::db::model_pools::ProviderAccount {
            id: "vision-account".into(),
            provider_id: "vision-provider".into(),
            label: "Vision".into(),
            base_url: "https://example.invalid".into(),
            credential_id: None,
            profile_ids: vec![],
        })
        .unwrap();
        for (id, kind) in [("vision-profile", "vision"), ("image-profile", "image")] {
            db.save_basic_service_profile(
                id,
                crate::db::basic_services::BasicServiceInput {
                    label: id.into(),
                    provider_account_id: "vision-account".into(),
                    service_kind: kind.into(),
                    model: format!("actual-{kind}"),
                    adapter: "openai".into(),
                    parameters: serde_json::json!({}),
                    expected_revision: None,
                },
            )
            .unwrap();
        }
        db.conn().execute("INSERT INTO model_pool_attempts(id,request_id,model_id,tier,started_at,ended_at,usage) VALUES('vision-attempt','r','vision-profile',1,?1,?1,'{\"prompt_tokens\":7,\"completion_tokens\":3,\"total_tokens\":10}'),('image-attempt','r','image-profile',1,?1,?1,'{\"image_count\":1}')",[at]).unwrap();
        let report = db
            .aggregate_llm_usage_in_timezone(None, at - 1, at + 1, 480)
            .unwrap();
        assert_eq!(
            (
                report.prompt_tokens,
                report.completion_tokens,
                report.total_tokens,
                report.known_usage_count
            ),
            (7, 3, 10, 1)
        );
        assert_eq!(report.days[0].date, "2026-10-21");
        let vision = report
            .models
            .iter()
            .find(|m| m.model_id == "vision-profile")
            .unwrap();
        assert_eq!(
            (&*vision.provider, &*vision.model, vision.known_usage_count),
            ("vision-provider", "actual-vision", 1)
        );
        let image = report
            .models
            .iter()
            .find(|m| m.model_id == "image-profile")
            .unwrap();
        assert_eq!((image.total_tokens, image.known_usage_count), (0, 0));
        let filtered = db
            .aggregate_llm_usage_in_timezone(Some("vision-profile"), at - 1, at + 1, 480)
            .unwrap();
        assert_eq!(filtered.total_tokens, 10);
        assert_eq!(filtered.models.len(), 1);
    }
    #[test]
    fn local_usage_days_keep_unknown_failed_attempts_separate_and_filter_models() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.conn().execute("INSERT INTO llm_models(id,provider,label,model,base_url,api_key_ref,created_at,updated_at) VALUES('model-a','local','Model A','actual-a','http://127.0.0.1:18001/v1','fixture-a',1,1),('model-b','local','Model B','actual-b','http://127.0.0.1:18002/v1','fixture-b',1,1)",[]).unwrap();
        let base = chrono::DateTime::parse_from_rfc3339("2026-10-20T15:30:00Z")
            .unwrap()
            .timestamp_millis();
        for (id, recorded_at, prompt, completion) in
            [("known-a", base, 4, 1), ("known-b", base + 3600000, 6, 4)]
        {
            db.record_llm_usage(&LlmUsageInput {
                id: id.into(),
                model_id: "model-a".into(),
                provider: "local".into(),
                model: "actual-a".into(),
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: prompt + completion,
                recorded_at,
                source: "test".into(),
            })
            .unwrap();
        }
        db.conn().execute("INSERT INTO model_pool_attempts(id,request_id,model_id,tier,started_at,ended_at,failure,usage) VALUES('known-a','request-a','model-a',0,?1,?1,'timeout','{}'),('unknown-b','request-b','model-b',1,?1,?1,'rate_limit',NULL),('pending-c','request-c','model-b',1,?1,NULL,NULL,NULL)",[base]).unwrap();
        let report = db
            .aggregate_llm_usage_in_timezone(None, base - 1000, base + 7200000, 480)
            .unwrap();
        assert_eq!(report.total_tokens, 15);
        assert_eq!(report.known_usage_count, 2);
        assert_eq!(report.unknown_usage_count, 1);
        assert_eq!(report.failed_attempt_count, 2);
        assert_eq!(report.pending_attempt_count, 1);
        assert_eq!(report.days[0].date, "2026-10-20");
        assert_eq!(report.days[1].date, "2026-10-21");
        let filtered = db
            .aggregate_llm_usage_in_timezone(Some("model-a"), base - 1000, base + 7200000, 480)
            .unwrap();
        assert_eq!(filtered.models.len(), 1);
        assert_eq!(filtered.unknown_usage_count, 0);
        assert_eq!(filtered.failed_attempt_count, 1);
        assert!(db
            .aggregate_llm_usage_in_timezone(None, base, base + 1000, 900)
            .is_err());
    }
}
