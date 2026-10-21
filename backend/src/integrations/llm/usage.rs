use crate::db::{Database, LlmUsageInput};

use super::client::UsageRecorder;
use super::types::Usage;

pub struct DatabaseUsageRecorder {
    attempt: std::sync::Mutex<Option<String>>,
    pending_attempt: std::sync::Mutex<Option<(String, String, usize)>>,
    db: Database,
    model_id: String,
    provider: String,
    model: String,
    source: String,
}

impl DatabaseUsageRecorder {
    pub fn new(
        db: Database,
        model_id: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            attempt: std::sync::Mutex::new(None),
            pending_attempt: std::sync::Mutex::new(None),
            db,
            model_id: model_id.into(),
            provider: provider.into(),
            model: model.into(),
            source: source.into(),
        }
    }
}

impl UsageRecorder for DatabaseUsageRecorder {
    fn record_stream_usage(&self, usage: &Usage) {
        let pool_attempt = self.attempt.lock().unwrap().is_some();
        if pool_attempt {
            self.record(usage)
        }
    }
    fn record(&self, usage: &Usage) {
        let input = LlmUsageInput {
            id: uuid::Uuid::new_v4().to_string(),
            model_id: self.model_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            recorded_at: chrono::Utc::now().timestamp_millis(),
            source: self.source.clone(),
        };
        let attempt = self.attempt.lock().unwrap().clone();
        let result = if let Some(attempt) = attempt {
            self.db.record_pool_attempt_usage(&attempt, &input)
        } else {
            self.db.record_llm_usage(&input)
        };
        if let Err(error) = result {
            tracing::warn!("LLM usage persistence failed: {}", error);
        }
    }
    fn begin_attempt(&self, request: &str, attempt: &str, tier: usize) {
        *self.attempt.lock().unwrap() = Some(attempt.into());
        *self.pending_attempt.lock().unwrap() = Some((request.into(), attempt.into(), tier));
    }
    fn request_started(&self) {
        let pending = self.pending_attempt.lock().unwrap().take();
        if let Some((request, attempt, tier)) = pending {
            if let Err(error) = self
                .db
                .begin_pool_attempt(&attempt, &request, &self.model_id, tier)
            {
                tracing::warn!("LLM attempt persistence failed: {}", error);
            }
        }
    }
    fn finish_attempt(&self, failure: Option<&str>) {
        if let Some(attempt) = self.attempt.lock().unwrap().as_deref() {
            if let Err(error) = self.db.finish_pool_attempt(attempt, failure) {
                tracing::warn!("LLM attempt completion persistence failed: {}", error);
            }
        }
    }
}
