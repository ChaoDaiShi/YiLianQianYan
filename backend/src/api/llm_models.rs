use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::types::ModelConfig;
use crate::db::{LlmModelInput, LlmModelRow, LlmUsageInput};
use crate::llm::client::{LlmClient, LlmError};
use crate::llm::types::ChatMessage;
use crate::llm::usage::DatabaseUsageRecorder;
use crate::safety::AuditEventType;
use crate::secret::{llm_model_key_ref, record_secret_event, SecretKind, SecretRef, SecretSource};
use crate::server::AppServer;

const MAX_MODELS: usize = 32;

#[derive(Debug, Clone, Deserialize)]
pub struct LlmModelPayload {
    pub provider: String,
    pub label: String,
    pub model: String,
    pub base_url: String,
    #[serde(default = "default_api_format")]
    pub api_format: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout")]
    pub invoke_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmModelView {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub model: String,
    pub base_url: String,
    pub api_format: String,
    pub api_key_configured: bool,
    pub api_key_source: SecretSource,
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

#[derive(Debug, Clone, Serialize)]
pub struct LlmModelResponse {
    pub model: LlmModelView,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageQuery {
    pub model_id: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

fn default_api_format() -> String {
    "openai".to_string()
}

fn default_max_tokens() -> u32 {
    16_384
}

fn default_timeout() -> u64 {
    120_000
}

fn validate_payload(payload: &LlmModelPayload) -> Result<(), String> {
    if payload.provider.trim().is_empty()
        || payload.label.trim().is_empty()
        || payload.model.trim().is_empty()
    {
        return Err("服务商、显示名称和模型名称不能为空".to_string());
    }
    let parsed = url::Url::parse(payload.base_url.trim())
        .map_err(|_| "API 地址必须是有效的 http(s) URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("API 地址必须使用 http 或 https".to_string());
    }
    if payload.api_format.trim() != "openai" {
        return Err("当前仅支持 OpenAI 兼容格式".to_string());
    }
    if !payload.temperature.is_finite() || !(0.0..=2.0).contains(&payload.temperature) {
        return Err("温度必须在 0 到 2 之间".to_string());
    }
    if payload.max_tokens == 0 || payload.max_tokens > 1_000_000 {
        return Err("最大 Token 必须在 1 到 1000000 之间".to_string());
    }
    if payload.invoke_timeout_ms < 1_000 || payload.invoke_timeout_ms > 600_000 {
        return Err("超时必须在 1000 到 600000 毫秒之间".to_string());
    }
    Ok(())
}

fn to_model_config(row: &LlmModelRow) -> ModelConfig {
    ModelConfig {
        provider: row.provider.clone(),
        name: row.model.clone(),
        base_url: row.base_url.clone(),
        api_key: String::new(),
        api_key_env: row.api_key_env.clone(),
        api_key_ref: Some(SecretRef::new(row.api_key_ref.clone())),
        clear_api_key: false,
        temperature: row.temperature,
        max_tokens: row.max_tokens,
        invoke_timeout_ms: row.invoke_timeout_ms,
        ..ModelConfig::default()
    }
}

async fn to_view(server: &AppServer, row: LlmModelRow) -> LlmModelView {
    let secret_ref = SecretRef::new(row.api_key_ref.clone());
    let in_store = matches!(
        server.secret_resolver.resolve_ref(&secret_ref).await,
        Ok(Some(_))
    );
    let from_env = !row.api_key_env.is_empty() && std::env::var(&row.api_key_env).is_ok();
    let source = if in_store {
        SecretSource::SecretStore
    } else if from_env {
        SecretSource::Environment
    } else {
        SecretSource::None
    };
    LlmModelView {
        id: row.id,
        provider: row.provider,
        label: row.label,
        model: row.model,
        base_url: row.base_url,
        api_format: row.api_format,
        api_key_configured: in_store || from_env,
        api_key_source: source,
        api_key_env: row.api_key_env,
        temperature: row.temperature,
        max_tokens: row.max_tokens,
        invoke_timeout_ms: row.invoke_timeout_ms,
        active: row.active,
        verified_at: row.verified_at,
        last_error: row.last_error,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn input_for(id: String, payload: &LlmModelPayload, api_key_ref: String) -> LlmModelInput {
    LlmModelInput {
        id,
        provider: payload.provider.trim().to_string(),
        label: payload.label.trim().to_string(),
        model: payload.model.trim().to_string(),
        base_url: payload.base_url.trim().trim_end_matches('/').to_string(),
        api_format: payload.api_format.trim().to_lowercase(),
        api_key_ref,
        api_key_env: payload.api_key_env.trim().to_string(),
        temperature: payload.temperature,
        max_tokens: payload.max_tokens,
        invoke_timeout_ms: payload.invoke_timeout_ms,
    }
}

async fn save_secret(
    server: &AppServer,
    secret_ref: &SecretRef,
    value: &str,
) -> Result<(), String> {
    server
        .secret_store
        .put(secret_ref, SecretString::from(value.to_string()))
        .await
        .map_err(|_| "系统凭据库不可用，API Key 未保存".to_string())
}

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<Vec<LlmModelView>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = server.db.list_llm_models().map_err(internal_error)?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(to_view(&server, row).await);
    }
    Ok(Json(views))
}

pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<LlmModelPayload>,
) -> Result<(StatusCode, Json<LlmModelView>), (StatusCode, Json<serde_json::Value>)> {
    validate_payload(&payload).map_err(bad_request)?;
    if server.db.list_llm_models().map_err(internal_error)?.len() >= MAX_MODELS {
        return Err(bad_request("模型数量已达到 32 个上限".to_string()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let secret_ref = llm_model_key_ref(&id);
    if !payload.api_key.trim().is_empty() {
        save_secret(&server, &secret_ref, payload.api_key.trim())
            .await
            .map_err(bad_request)?;
    }
    let row = server
        .db
        .create_llm_model(&input_for(id, &payload, secret_ref.key.clone()))
        .map_err(internal_error)?;
    if !payload.api_key.trim().is_empty() {
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::LlmModelApiKey,
            &secret_ref,
            "create",
            true,
        );
    }
    Ok((StatusCode::CREATED, Json(to_view(&server, row).await)))
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(payload): Json<LlmModelPayload>,
) -> Result<Json<LlmModelView>, (StatusCode, Json<serde_json::Value>)> {
    validate_payload(&payload).map_err(bad_request)?;
    let existing = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    let secret_ref = SecretRef::new(existing.api_key_ref.clone());
    if payload.clear_api_key {
        server
            .secret_store
            .delete(&secret_ref)
            .await
            .map_err(|_| bad_request("API Key 清除失败".to_string()))?;
    } else if !payload.api_key.trim().is_empty() {
        save_secret(&server, &secret_ref, payload.api_key.trim())
            .await
            .map_err(bad_request)?;
    }
    let row = server
        .db
        .update_llm_model(&input_for(id, &payload, existing.api_key_ref))
        .map_err(internal_error)?;
    Ok(Json(to_view(&server, row).await))
}

pub async fn delete_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let row = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    let secret_ref = SecretRef::new(row.api_key_ref);
    let _ = server.secret_store.delete(&secret_ref).await;
    server.db.delete_llm_model(&id).map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn activate_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<LlmModelView>, (StatusCode, Json<serde_json::Value>)> {
    let row = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    if row.verified_at.is_none() {
        return Err(bad_request("请先验证模型连接".to_string()));
    }
    server.db.activate_llm_model(&id).map_err(internal_error)?;
    let row = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    Ok(Json(to_view(&server, row).await))
}

pub async fn verify_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<LlmModelResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    let recorder = Arc::new(DatabaseUsageRecorder::new(
        server.db.clone_connection(),
        row.id.clone(),
        row.provider.clone(),
        row.model.clone(),
        "verify",
    ));
    let client = LlmClient::new_with_usage_recorder(
        &to_model_config(&row),
        Arc::clone(&server.secret_resolver),
        Some(recorder),
    );
    let result = client
        .invoke(
            &[ChatMessage {
                role: "user".to_string(),
                content: Some("ping".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            &[],
        )
        .await;
    match result {
        Ok(response) => {
            let verified_at = chrono::Utc::now().timestamp_millis();
            server
                .db
                .set_llm_model_verification(&id, Some(verified_at), None)
                .map_err(internal_error)?;
            if let Some(usage) = response.usage {
                let _ = server.db.record_llm_usage(&LlmUsageInput {
                    id: uuid::Uuid::new_v4().to_string(),
                    model_id: id.clone(),
                    provider: row.provider.clone(),
                    model: row.model.clone(),
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                    recorded_at: verified_at,
                    source: "verify".to_string(),
                });
            }
        }
        Err(error) => {
            let safe = safe_error(&error);
            let _ = server.db.set_llm_model_verification(&id, None, Some(&safe));
            return Err(bad_request(safe));
        }
    }
    let row = server
        .db
        .get_llm_model(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("模型不存在".to_string()))?;
    Ok(Json(LlmModelResponse {
        model: to_view(&server, row).await,
    }))
}

pub async fn usage_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<crate::db::LlmUsageReport>, (StatusCode, Json<serde_json::Value>)> {
    let now = chrono::Utc::now().timestamp_millis();
    let from = query.from.unwrap_or(now - 30 * 86_400_000);
    let to = query.to.unwrap_or(now + 1);
    if to <= from || to - from > 366 * 86_400_000 {
        return Err(bad_request(
            "用量查询时间范围必须在 1 到 366 天内".to_string(),
        ));
    }
    Ok(Json(
        server
            .db
            .aggregate_llm_usage(query.model_id.as_deref(), from, to)
            .map_err(internal_error)?,
    ))
}

fn safe_error(error: &LlmError) -> String {
    match error {
        LlmError::NoApiKey | LlmError::EmbeddingNoApiKey => "未配置 API Key".to_string(),
        LlmError::Timeout => "连接超时".to_string(),
        LlmError::Http(_) | LlmError::Api(_) => "服务商返回错误".to_string(),
        LlmError::Parse(_) | LlmError::Stream(_) => "响应格式不兼容".to_string(),
        _ => "模型连接失败".to_string(),
    }
}

fn bad_request(message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": message })),
    )
}

fn internal_error(message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "模型数据暂时不可用", "detail": message })),
    )
}
