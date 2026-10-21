use axum::{extract::State, http::StatusCode, Json};
use secrecy::SecretString;
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::integrations::secret::{anysearch_key_ref, SecretStoreError};
use crate::modules::cloud::search::{
    SearchError, SearchFailureKind, SearchRequest, SearchResponse,
};

#[derive(serde::Deserialize)]
pub struct SearchConfigUpdate {
    provider: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
}

#[derive(serde::Deserialize)]
pub struct SearchPayload {
    query: String,
    max_results: Option<u8>,
    tag: Option<String>,
    zone: Option<String>,
    language: Option<String>,
}

pub async fn config_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (provider, base_url, secret_ref) =
        server.db.cloud_search_config().map_err(database_error)?;
    let configured = if let Some(secret_ref) = secret_ref {
        matches!(server.secret_store.get(&secret_ref).await, Ok(Some(_)))
    } else {
        false
    };
    Ok(Json(serde_json::json!({
        "provider": provider,
        "base_url": base_url,
        "configured": configured,
        "credential_state": if configured { "local_only" } else { "anonymous" }
    })))
}

pub async fn update_config_handler(
    State(server): State<Arc<AppServer>>,
    Json(update): Json<SearchConfigUpdate>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (current_provider, current_base_url, current_ref) =
        server.db.cloud_search_config().map_err(database_error)?;
    let base_url = update
        .base_url
        .as_deref()
        .unwrap_or(&current_base_url)
        .trim_end_matches('/');
    validate_base_url(base_url)?;
    let provider = update.provider.as_deref().unwrap_or(&current_provider);
    if !matches!(provider, "anysearch" | "tavily" | "brave" | "bocha") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_provider",
            "搜索厂商无效",
        ));
    }
    let target = if provider == "anysearch" {
        anysearch_key_ref()
    } else {
        crate::integrations::secret::SecretRef::new(format!("search.{provider}.api_key"))
    };
    let mut secret_ref = if provider == current_provider {
        current_ref
    } else {
        Some(target.clone())
    };
    if update.clear_api_key {
        server
            .secret_store
            .delete(&target)
            .await
            .map_err(secret_error)?;
        secret_ref = None;
    } else if let Some(api_key) = update.api_key.filter(|value| !value.trim().is_empty()) {
        server
            .secret_store
            .put(&target, SecretString::from(api_key))
            .await
            .map_err(secret_error)?;
        secret_ref = Some(target);
    }
    server
        .db
        .save_search_provider(provider, base_url, secret_ref.as_ref())
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "search_config_failed",
                "搜索配置保存失败",
            )
        })?;
    config_handler(State(server)).await
}

pub async fn search_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<SearchPayload>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut request = SearchRequest::new(payload.query, payload.max_results.unwrap_or(10))
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, "invalid_search", message))?;
    request.tag = bounded(payload.tag, 80, "tag")?;
    request.language = bounded(payload.language, 16, "language")?;
    request.zone = match payload.zone.as_deref() {
        None => None,
        Some("cn" | "intl") => payload.zone,
        Some(_) => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "invalid_zone",
                "搜索区域必须是 cn 或 intl",
            ))
        }
    };
    let (provider, base_url, secret_ref) =
        server.db.cloud_search_config().map_err(database_error)?;
    crate::modules::cloud::search::search_configured(
        &provider,
        base_url,
        Arc::clone(&server.secret_store),
        secret_ref,
        request,
    )
    .await
    .map(Json)
    .map_err(search_error)
}

fn validate_base_url(value: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let parsed = url::Url::parse(value).map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_search_url",
            "搜索服务地址无效",
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_search_url",
            "搜索服务地址无效",
        ));
    }
    Ok(())
}

fn bounded(
    value: Option<String>,
    limit: usize,
    field: &str,
) -> Result<Option<String>, (StatusCode, Json<serde_json::Value>)> {
    if value
        .as_ref()
        .is_some_and(|item| item.chars().count() > limit)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_search",
            &format!("{field} 过长"),
        ));
    }
    Ok(value)
}

fn search_error(error: SearchError) -> (StatusCode, Json<serde_json::Value>) {
    match error.kind {
        SearchFailureKind::InvalidRequest => api_error(
            StatusCode::BAD_REQUEST,
            "search_invalid_request",
            "搜索请求无效",
        ),
        SearchFailureKind::Authentication => api_error(
            StatusCode::BAD_GATEWAY,
            "search_authentication_failed",
            "搜索服务凭据不可用",
        ),
        SearchFailureKind::Quota => api_error(
            StatusCode::PAYMENT_REQUIRED,
            "search_quota_exhausted",
            "匿名搜索额度已用完，请配置本机 API Key",
        ),
        SearchFailureKind::RateLimited => api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "search_rate_limited",
            "搜索请求过于频繁，请稍后重试",
        ),
        SearchFailureKind::Unavailable | SearchFailureKind::Provider => api_error(
            StatusCode::BAD_GATEWAY,
            "search_unavailable",
            "搜索服务暂时不可用",
        ),
    }
}

fn database_error(_: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "search_config_unavailable",
        "搜索配置暂时不可用",
    )
}

fn secret_error(_: SecretStoreError) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "search_secret_not_saved",
        "搜索凭据未能安全保存",
    )
}

fn api_error(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({"error": error, "message": message})),
    )
}
