use super::{
    client::{LlmClient, LlmError},
    types::{ChatCompletionResponse, ChatMessage, StreamAccumulator},
};
use crate::db::model_pools::ModelPool;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    Network,
    Timeout,
    RateLimit,
    Quota,
    Authentication,
    ModelUnavailable,
    ServiceError,
}
impl FailureReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::RateLimit => "rate_limit",
            Self::Quota => "quota",
            Self::Authentication => "authentication",
            Self::ModelUnavailable => "model_unavailable",
            Self::ServiceError => "service_error",
        }
    }
}
struct AttemptLifecycle<'a> {
    client: &'a LlmClient,
    finished: bool,
}
impl AttemptLifecycle<'_> {
    fn finish(&mut self, failure: Option<&str>) {
        self.client.finish_attempt(failure);
        self.finished = true;
    }
}
impl Drop for AttemptLifecycle<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.client.finish_attempt(Some("cancelled"));
        }
    }
}
pub fn classify(error: &LlmError) -> Option<FailureReason> {
    match error {
        LlmError::Http(e) if e.is_timeout() => Some(FailureReason::Timeout),
        LlmError::Http(e) if e.is_connect() || e.is_body() => Some(FailureReason::Network),
        LlmError::Timeout => Some(FailureReason::Timeout),
        LlmError::Stream(_) => Some(FailureReason::Network),
        LlmError::NoApiKey | LlmError::SecretUnavailable => Some(FailureReason::Authentication),
        LlmError::Provider { status, code } => match (*status, code.as_deref()) {
            (401, _) => Some(FailureReason::Authentication),
            (429, Some("insufficient_quota" | "quota_exceeded" | "billing_hard_limit_reached")) => {
                Some(FailureReason::Quota)
            }
            (429, _) => Some(FailureReason::RateLimit),
            (_, Some("model_not_found" | "model_not_available" | "model_decommissioned")) => {
                Some(FailureReason::ModelUnavailable)
            }
            (_, Some("insufficient_quota" | "quota_exceeded")) => Some(FailureReason::Quota),
            (_, Some("invalid_api_key" | "authentication_error" | "invalid_authentication")) => {
                Some(FailureReason::Authentication)
            }
            (_, Some("Arrearage" | "AllocationQuota.FreeTierOnly")) => Some(FailureReason::Quota),
            (_, Some("InvalidApiKey")) => Some(FailureReason::Authentication),
            (_, Some("rate_limit_exceeded" | "Throttling")) => Some(FailureReason::RateLimit),
            (500..=599, _) => Some(FailureReason::ServiceError),
            _ => None,
        },
        _ => None,
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelRouteEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub request_id: String,
    pub conversation_id: String,
    pub pool_id: String,
    pub pool_revision: u64,
    pub attempt_id: String,
    pub phase: String,
    pub from_profile_id: Option<String>,
    pub to_profile_id: Option<String>,
    pub from_tier: Option<usize>,
    pub to_tier: Option<usize>,
    pub reason: Option<FailureReason>,
    pub interrupted_content: Option<String>,
    pub replace_content: bool,
}
pub struct PoolCandidate {
    pub profile_id: String,
    pub tier: usize,
    pub client: LlmClient,
}
pub struct ModelPoolRuntime {
    db: Option<crate::db::Database>,
    pub pool: ModelPool,
    pub candidates: Vec<PoolCandidate>,
    pub conversation_id: String,
    pub on_route: Arc<dyn Fn(ModelRouteEvent) + Send + Sync>,
    last_tier: AtomicUsize,
    serial: tokio::sync::Mutex<()>,
}
impl ModelPoolRuntime {
    pub fn set_thinking_mode(&mut self, mode: super::thinking::ThinkingMode) {
        for candidate in &mut self.candidates {
            candidate.client.set_thinking_mode(mode);
        }
    }
    pub fn with_thinking_mode(mut self, mode: super::thinking::ThinkingMode) -> Self {
        self.set_thinking_mode(mode);
        self
    }

    pub fn new(
        pool: ModelPool,
        candidates: Vec<PoolCandidate>,
        conversation_id: String,
        on_route: Arc<dyn Fn(ModelRouteEvent) + Send + Sync>,
    ) -> Self {
        Self {
            db: None,
            pool,
            candidates,
            conversation_id,
            on_route,
            last_tier: AtomicUsize::new(0),
            serial: tokio::sync::Mutex::new(()),
        }
    }
    fn event(
        &self,
        request: &str,
        attempt: &str,
        index: usize,
        phase: &str,
        reason: Option<FailureReason>,
        interrupted: Option<String>,
    ) {
        let candidate = self.candidates.get(index);
        let previous = index.checked_sub(1).and_then(|i| self.candidates.get(i));
        (self.on_route)(ModelRouteEvent {
            event_type: "model_route",
            request_id: request.into(),
            conversation_id: self.conversation_id.clone(),
            pool_id: self.pool.id.clone(),
            pool_revision: self.pool.revision,
            attempt_id: attempt.into(),
            phase: phase.into(),
            from_profile_id: previous.map(|c| c.profile_id.clone()),
            to_profile_id: candidate.map(|c| c.profile_id.clone()),
            from_tier: previous.map(|c| c.tier),
            to_tier: candidate.map(|c| c.tier),
            reason,
            replace_content: interrupted.is_some(),
            interrupted_content: interrupted,
        });
    }
    pub async fn invoke(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let request = uuid::Uuid::new_v4().to_string();
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(self.pool.total_timeout_ms);
        let _serial = tokio::time::timeout_at(deadline, self.serial.lock())
            .await
            .map_err(|_| LlmError::Timeout)?;
        let mut reason = None;
        let mut failures = Vec::new();
        for (i, c) in self.candidates.iter().enumerate() {
            let attempt = uuid::Uuid::new_v4().to_string();
            c.client.begin_attempt(&request, &attempt, c.tier);
            let mut lifecycle = AttemptLifecycle {
                client: &c.client,
                finished: false,
            };
            self.event(
                &request,
                &attempt,
                i,
                if i == 0 { "selected" } else { "switched" },
                reason,
                None,
            );
            let until = deadline.min(
                tokio::time::Instant::now() + Duration::from_millis(self.pool.attempt_timeout_ms),
            );
            match tokio::time::timeout_at(
                until,
                c.client.invoke_single_with_max_tokens(
                    messages,
                    tools,
                    max_tokens.min(c.client.max_output_tokens()),
                ),
            )
            .await
            .unwrap_or(Err(LlmError::Timeout))
            {
                Ok(result) => {
                    lifecycle.finish(None);
                    if i == 0 && self.last_tier.swap(c.tier, Ordering::Relaxed) > c.tier {
                        self.event(&request, &attempt, i, "recovered", None, None);
                    } else {
                        self.last_tier.store(c.tier, Ordering::Relaxed);
                    }
                    if let Some(db) = &self.db {
                        if let Err(error) = db.set_model_pool_last_tier(&self.pool.id, c.tier) {
                            tracing::warn!("Model route persistence failed: {}", error);
                        }
                    }
                    return Ok(result);
                }
                Err(e) => {
                    reason = classify(&e);
                    lifecycle.finish(Some(reason.map_or("request_error", FailureReason::as_str)));
                    if reason.is_none() {
                        return Err(e);
                    }
                    failures.push(format!("{}: {:?}", c.profile_id, reason.unwrap()));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        self.event(
            &request,
            &uuid::Uuid::new_v4().to_string(),
            self.candidates.len(),
            "exhausted",
            reason,
            None,
        );
        Err(LlmError::Api(format!(
            "本轮所有候选均不可用：{}",
            failures.join("；")
        )))
    }
    pub async fn stream<F>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        on_token: F,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_with_reasoning(messages, tools, max_tokens, on_token, |_| {})
            .await
    }
    pub async fn stream_with_reasoning<F, R>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        mut on_token: F,
        mut on_reasoning: R,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
        R: FnMut(&str) + Send,
    {
        let request = uuid::Uuid::new_v4().to_string();
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(self.pool.total_timeout_ms);
        let _serial = tokio::time::timeout_at(deadline, self.serial.lock())
            .await
            .map_err(|_| LlmError::Timeout)?;
        let mut reason = None;
        let mut interrupted = None;
        let mut failures = Vec::new();
        for (i, c) in self.candidates.iter().enumerate() {
            let attempt = uuid::Uuid::new_v4().to_string();
            c.client.begin_attempt(&request, &attempt, c.tier);
            let mut lifecycle = AttemptLifecycle {
                client: &c.client,
                finished: false,
            };
            self.event(
                &request,
                &attempt,
                i,
                if i == 0 { "selected" } else { "switched" },
                reason,
                interrupted.take(),
            );
            let mut content = String::new();
            let until = deadline.min(
                tokio::time::Instant::now() + Duration::from_millis(self.pool.attempt_timeout_ms),
            );
            let result = tokio::time::timeout_at(
                until,
                c.client.stream_single_with_reasoning_max_tokens(
                    messages,
                    tools,
                    max_tokens.min(c.client.max_output_tokens()),
                    |token| {
                        content.push_str(token);
                        on_token(token)
                    },
                    |reasoning| on_reasoning(reasoning),
                ),
            )
            .await
            .unwrap_or(Err(LlmError::Timeout));
            match result {
                Ok(result) => {
                    lifecycle.finish(None);
                    if i == 0 && self.last_tier.swap(c.tier, Ordering::Relaxed) > c.tier {
                        self.event(&request, &attempt, i, "recovered", None, None);
                    } else {
                        self.last_tier.store(c.tier, Ordering::Relaxed);
                    }
                    if let Some(db) = &self.db {
                        if let Err(error) = db.set_model_pool_last_tier(&self.pool.id, c.tier) {
                            tracing::warn!("Model route persistence failed: {}", error);
                        }
                    }
                    return Ok(result);
                }
                Err(e) => {
                    reason = classify(&e);
                    lifecycle.finish(Some(reason.map_or("request_error", FailureReason::as_str)));
                    if reason.is_none() {
                        return Err(e);
                    }
                    failures.push(format!("{}: {:?}", c.profile_id, reason.unwrap()));
                    interrupted = (!content.is_empty()).then_some(content);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        self.event(
            &request,
            &uuid::Uuid::new_v4().to_string(),
            self.candidates.len(),
            "exhausted",
            reason,
            interrupted,
        );
        Err(LlmError::Api(format!(
            "本轮所有候选均不可用：{}",
            failures.join("；")
        )))
    }
}
/// Resolve one immutable scheduling snapshot; no lock is retained over provider I/O.
pub fn load_model_pool_runtime(
    db: &crate::db::Database,
    resolver: Arc<crate::integrations::secret::SecretResolver>,
    pool_id: &str,
    conversation_id: &str,
    on_route: Arc<dyn Fn(ModelRouteEvent) + Send + Sync>,
) -> Result<ModelPoolRuntime, String> {
    let pool = db.get_model_pool(pool_id)?.ok_or("模型池不存在")?;
    if !pool.enabled {
        return Err("模型池未启用".into());
    }
    let mut candidates = Vec::new();
    for (tier, layer) in pool.tiers.iter().enumerate() {
        for profile_id in &layer.profile_ids {
            let row = db
                .get_llm_model(profile_id)?
                .ok_or("模型池候选档案已删除")?;
            let recorder = Arc::new(super::usage::DatabaseUsageRecorder::new(
                db.clone_connection(),
                row.id.clone(),
                row.provider.clone(),
                row.model.clone(),
                "model_pool",
            ));
            let client = LlmClient::new_with_usage_recorder(
                &row.to_model_config(),
                resolver.clone(),
                Some(recorder),
            );
            candidates.push(PoolCandidate {
                profile_id: profile_id.clone(),
                tier: tier + 1,
                client,
            });
        }
    }
    let last_tier = db.model_pool_last_tier(pool_id)?;
    let mut runtime = ModelPoolRuntime::new(pool, candidates, conversation_id.into(), on_route);
    runtime.db = Some(db.clone_connection());
    runtime.last_tier.store(last_tier, Ordering::Relaxed);
    Ok(runtime)
}

#[cfg(test)]
#[path = "model_pool_tests.rs"]
mod model_pool_tests;
