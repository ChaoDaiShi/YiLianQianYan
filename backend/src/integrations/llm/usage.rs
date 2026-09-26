use crate::db::{Database, LlmUsageInput};

use super::client::UsageRecorder;
use super::types::Usage;

pub struct DatabaseUsageRecorder {
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
            db,
            model_id: model_id.into(),
            provider: provider.into(),
            model: model.into(),
            source: source.into(),
        }
    }
}

impl UsageRecorder for DatabaseUsageRecorder {
    fn record(&self, usage: &Usage) {
        let _ = self.db.record_llm_usage(&LlmUsageInput {
            id: uuid::Uuid::new_v4().to_string(),
            model_id: self.model_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            recorded_at: chrono::Utc::now().timestamp_millis(),
            source: self.source.clone(),
        });
    }
}
