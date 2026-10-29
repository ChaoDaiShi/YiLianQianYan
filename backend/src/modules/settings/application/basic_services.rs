use crate::{
    db::{basic_services::BasicServiceProfile, Database},
    integrations::secret::SecretResolver,
};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Clone, Deserialize)]
pub struct BasicServiceRequest {
    pub prompt: String,
    pub image_url: Option<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum BasicServiceError {
    #[error("{0}")]
    Configuration(String),
    #[error("{0}")]
    Availability(crate::integrations::llm::client::LlmError),
}
impl BasicServiceError {
    pub fn diagnostic_code(&self) -> Option<&'static str> {
        use crate::integrations::llm::client::LlmError;
        match self {
            Self::Availability(LlmError::NoApiKey) => Some("BASIC_SERVICE_NO_API_KEY"),
            Self::Availability(LlmError::SecretUnavailable) => {
                Some("BASIC_SERVICE_SECRET_UNAVAILABLE")
            }
            _ => None,
        }
    }
    pub fn user_message(&self, kind: &str) -> String {
        let service = if kind == "vision" {
            "图像理解"
        } else {
            "图像生成"
        };
        match self.diagnostic_code() {
            Some("BASIC_SERVICE_NO_API_KEY") => format!("{service}服务所选厂商账户没有可用 API Key。请前往设置 → 模型与服务 → {service}，确认所选账户，并为该账户配置 API Key。"),
            Some("BASIC_SERVICE_SECRET_UNAVAILABLE") => format!("{service}服务无法读取所选厂商账户的凭据：系统凭据库不可用。请检查系统凭据库，并前往设置 → 模型与服务 → {service}确认所选账户。"),
            _ => self.to_string(),
        }
    }
}
impl From<String> for BasicServiceError {
    fn from(value: String) -> Self {
        Self::Configuration(value)
    }
}
impl From<&str> for BasicServiceError {
    fn from(value: &str) -> Self {
        Self::Configuration(value.into())
    }
}
pub async fn invoke_profile(
    db: &Database,
    resolver: &SecretResolver,
    p: &BasicServiceProfile,
    request: BasicServiceRequest,
) -> Result<Value, BasicServiceError> {
    invoke_profile_inner(db, resolver, p, request, None).await
}
async fn invoke_profile_inner(
    db: &Database,
    resolver: &SecretResolver,
    p: &BasicServiceProfile,
    request: BasicServiceRequest,
    on_dispatched: Option<&(dyn Fn() -> Result<(), String> + Send + Sync)>,
) -> Result<Value, BasicServiceError> {
    if request.prompt.trim().is_empty() {
        return Err("请输入请求内容".into());
    }
    if p.service_kind == "vision"
        && request
            .image_url
            .as_deref()
            .is_none_or(|url| url.trim().is_empty())
    {
        return Err("识图需要显式提供当前对话已绑定图片的 resource_id 或图片 image_url".into());
    }
    let account = db
        .list_provider_accounts()?
        .into_iter()
        .find(|a| a.id == p.provider_account_id)
        .ok_or("厂商账户不存在")?;
    let key = if let Some(reference) = &account.credential_id {
        resolver
            .resolve_ref(&crate::integrations::secret::SecretRef::new(
                reference.clone(),
            ))
            .await
            .map_err(|_| {
                BasicServiceError::Availability(
                    crate::integrations::llm::client::LlmError::SecretUnavailable,
                )
            })?
    } else {
        None
    };
    if key.is_none() && !matches!(account.provider_id.as_str(), "ollama" | "lmstudio") {
        return Err(BasicServiceError::Availability(
            crate::integrations::llm::client::LlmError::NoApiKey,
        ));
    }
    let base = account.base_url.trim_end_matches('/');
    let (url, body) = service_request(p, base, request)?;
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "请求客户端不可用")?
        .post(url)
        .json(&body);
    if let Some(key) = key {
        builder = builder.bearer_auth(key.expose_secret());
    }
    if let Some(on_dispatched) = on_dispatched {
        on_dispatched()?;
    }
    let response = builder.send().await.map_err(|e| {
        BasicServiceError::Availability(crate::integrations::llm::client::LlmError::Http(e))
    })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let value = response.json::<Value>().await.ok();
        let error = value
            .as_ref()
            .and_then(|v| {
                crate::integrations::llm::client::response_error(&account.provider_id, status, v)
            })
            .unwrap_or(crate::integrations::llm::client::LlmError::Provider { status, code: None });
        return Err(BasicServiceError::Availability(error));
    }
    let value: Value = response.json().await.map_err(|_| "服务响应格式无效")?;
    if let Some(error) =
        crate::integrations::llm::client::response_error(&account.provider_id, 200, &value)
    {
        return Err(BasicServiceError::Availability(error));
    }
    parse_service_response(p, &value).map_err(BasicServiceError::from)
}
pub fn service_request(
    p: &BasicServiceProfile,
    base: &str,
    request: BasicServiceRequest,
) -> Result<(String, Value), String> {
    if p.service_kind == "vision" {
        let image = request.image_url.ok_or("识图需要上传图像或提供图像 URL")?;
        return Ok((
            format!("{base}/chat/completions"),
            json!({"model":p.model,"messages":[{"role":"user","content":[{"type":"text","text":request.prompt},{"type":"image_url","image_url":{"url":image}}]}],"max_tokens":1024}),
        ));
    }
    let mut body = match p.adapter.as_str() {
        "openai" => json!({"model":p.model,"prompt":request.prompt,"n":1}),
        "minimax" => {
            json!({"model":p.model,"prompt":request.prompt,"n":1,"response_format":"url","aspect_ratio":"1:1"})
        }
        "dashscope" => {
            json!({"model":p.model,"input":{"messages":[{"role":"user","content":[{"text":request.prompt}]}]},"parameters":{"size":"1024*1024","n":1}})
        }
        _ => return Err("未知服务协议".into()),
    };
    if let Some(parameters) = p.parameters.as_object() {
        for (k, v) in parameters {
            if matches!(
                k.as_str(),
                "size" | "quality" | "aspect_ratio" | "response_format" | "watermark"
            ) {
                if p.adapter == "dashscope" {
                    body["parameters"][k] = v.clone();
                } else {
                    body[k] = v.clone();
                }
            }
        }
    }
    let url = match p.adapter.as_str() {
        "minimax" => format!("{base}/image_generation"),
        "dashscope" => {
            let parsed = url::Url::parse(base).map_err(|_| "账户地址无效")?;
            format!(
                "{}/api/v1/services/aigc/multimodal-generation/generation",
                parsed.origin().ascii_serialization()
            )
        }
        _ => format!("{base}/images/generations"),
    };
    Ok((url, body))
}
pub fn parse_service_response(p: &BasicServiceProfile, v: &Value) -> Result<Value, String> {
    if p.service_kind == "vision" {
        return v.pointer("/choices/0/message/content").and_then(Value::as_str).filter(|text|!text.is_empty()).map(|text|json!({"profile_id":p.id,"service_kind":"vision","text":text,"usage":v.get("usage")})).ok_or("识图未返回文本".into());
    }
    let image = match p.adapter.as_str() {
        "minimax" => v
            .pointer("/data/image_urls/0")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "dashscope" => v
            .pointer("/output/choices/0/message/content/0/image")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => v
            .pointer("/data/0/url")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                v.pointer("/data/0/b64_json")
                    .and_then(Value::as_str)
                    .map(|b| format!("data:image/png;base64,{b}"))
            }),
    }
    .ok_or("生图未返回图像".to_string())?;
    Ok(json!({"profile_id":p.id,"service_kind":"image","image_url":image,"usage":v.get("usage")}))
}
struct ServiceAttemptLifecycle {
    db: Database,
    id: String,
    finished: bool,
}
impl ServiceAttemptLifecycle {
    fn finish(&mut self, failure: Option<&str>) -> Result<(), String> {
        self.db.finish_pool_attempt(&self.id, failure)?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for ServiceAttemptLifecycle {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.db.finish_pool_attempt(&self.id, Some("cancelled"));
        }
    }
}
pub async fn invoke_service_pool(
    db: &Database,
    resolver: &SecretResolver,
    pool: &crate::db::model_pools::ModelPool,
    profiles: &[BasicServiceProfile],
    request: BasicServiceRequest,
    conversation_id: &str,
    on_route: Option<
        &(dyn Fn(crate::integrations::llm::model_pool::ModelRouteEvent) + Send + Sync),
    >,
) -> Result<Value, BasicServiceError> {
    use crate::integrations::llm::{
        client::LlmError,
        model_pool::{classify, ModelRouteEvent},
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(pool.total_timeout_ms);
    let mut events = Vec::new();
    let mut previous: Option<(String, usize)> = None;
    let mut reason = None;
    let mut failures = Vec::new();
    for (tier, layer) in pool.tiers.iter().enumerate() {
        for id in &layer.profile_ids {
            let p = profiles
                .iter()
                .find(|p| &p.id == id)
                .ok_or("模型池候选档案不存在")?;
            let attempt = uuid::Uuid::new_v4().to_string();
            let event = ModelRouteEvent {
                event_type: "model_route",
                request_id: request_id.clone(),
                conversation_id: conversation_id.into(),
                pool_id: pool.id.clone(),
                pool_revision: pool.revision,
                attempt_id: attempt.clone(),
                phase: if previous.is_some() {
                    "switched"
                } else {
                    "selected"
                }
                .into(),
                from_profile_id: previous.as_ref().map(|p| p.0.clone()),
                to_profile_id: Some(id.clone()),
                from_tier: previous.as_ref().map(|p| p.1),
                to_tier: Some(tier + 1),
                reason,
                interrupted_content: None,
                replace_content: false,
            };
            if let Some(callback) = on_route {
                callback(event.clone());
            }
            events.push(serde_json::to_value(event).map_err(|_| "路由事件序列化失败")?);
            let mut lifecycle = ServiceAttemptLifecycle {
                db: db.clone_connection(),
                id: attempt.clone(),
                finished: false,
            };
            let on_dispatched = || db.begin_pool_attempt(&attempt, &request_id, id, tier + 1);
            let until = deadline.min(
                tokio::time::Instant::now()
                    + std::time::Duration::from_millis(pool.attempt_timeout_ms),
            );
            let result = tokio::time::timeout_at(
                until,
                invoke_profile_inner(db, resolver, p, request.clone(), Some(&on_dispatched)),
            )
            .await
            .unwrap_or(Err(BasicServiceError::Availability(LlmError::Timeout)));
            match result {
                Ok(mut result) => {
                    lifecycle.finish(None)?;
                    if let Some(usage) = result.get("usage").filter(|v| !v.is_null()) {
                        db.record_basic_pool_usage(&attempt, usage)?;
                    }
                    if tier == 0 && db.model_pool_last_tier(&pool.id)? > 1 {
                        let event = ModelRouteEvent {
                            event_type: "model_route",
                            request_id: request_id.clone(),
                            conversation_id: conversation_id.into(),
                            pool_id: pool.id.clone(),
                            pool_revision: pool.revision,
                            attempt_id: attempt,
                            phase: "recovered".into(),
                            from_profile_id: None,
                            to_profile_id: Some(id.clone()),
                            from_tier: None,
                            to_tier: Some(1),
                            reason: None,
                            interrupted_content: None,
                            replace_content: false,
                        };
                        if let Some(callback) = on_route {
                            callback(event.clone());
                        }
                        events.push(serde_json::to_value(event).map_err(|_| "路由事件序列化失败")?);
                    }
                    db.set_model_pool_last_tier(&pool.id, tier + 1)?;
                    result["model_routes"] = json!(events);
                    result["request_id"] = json!(request_id);
                    return Ok(result);
                }
                Err(error) => {
                    reason = match &error {
                        BasicServiceError::Availability(error) => classify(error),
                        _ => None,
                    };
                    lifecycle.finish(Some(reason.map_or(
                        "request_error",
                        crate::integrations::llm::model_pool::FailureReason::as_str,
                    )))?;
                    if reason.is_none() {
                        return Err(error);
                    }
                    if error.diagnostic_code().is_some() {
                        failures.push(format!(
                            "{}: {:?}；{}",
                            id,
                            reason,
                            error.user_message(&p.service_kind)
                        ));
                    } else {
                        failures.push(format!("{}: {:?}", id, reason));
                    }
                    previous = Some((id.clone(), tier + 1));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    let event = ModelRouteEvent {
        event_type: "model_route",
        request_id,
        conversation_id: conversation_id.into(),
        pool_id: pool.id.clone(),
        pool_revision: pool.revision,
        attempt_id: uuid::Uuid::new_v4().to_string(),
        phase: "exhausted".into(),
        from_profile_id: previous.as_ref().map(|p| p.0.clone()),
        to_profile_id: None,
        from_tier: previous.map(|p| p.1),
        to_tier: None,
        reason,
        interrupted_content: None,
        replace_content: false,
    };
    if let Some(callback) = on_route {
        callback(event);
    }
    Err(BasicServiceError::Configuration(format!(
        "本轮所有候选均不可用：{}",
        failures.join("；")
    )))
}

#[cfg(test)]
#[path = "basic_services_tests.rs"]
mod basic_services_tests;
