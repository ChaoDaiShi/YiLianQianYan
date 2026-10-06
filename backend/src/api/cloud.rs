use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::modules::cloud::{
    client::{CloudApiClient, CloudClientError},
    domain::CloudAuthRequest,
    session::{CloudSessionError, CloudSessionService},
    sync::{CloudSyncError, CloudSyncService},
};

#[derive(serde::Deserialize)]
pub struct AuthPayload {
    cloud_base_url: String,
    email: String,
    password: String,
    display_name: Option<String>,
}

pub async fn identity_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let identity = server.db.get_or_create_cloud_identity().map_err(internal_error)?;
    let sync = server.db.cloud_sync_status().map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "identity": identity,
        "sync": sync,
    })))
}

pub async fn login_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    authenticate(server, payload, false).await
}

pub async fn register_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    authenticate(server, payload, true).await
}

pub async fn logout_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    CloudSessionService::new(server.db.clone_connection(), Arc::clone(&server.secret_store))
        .logout()
        .await
        .map_err(session_error)?;
    identity_handler(State(server)).await
}

pub async fn sync_status_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let identity = server.db.get_or_create_cloud_identity().map_err(internal_error)?;
    let sync = server.db.cloud_sync_status().map_err(internal_error)?;
    let configuration = server.db.cloud_connection_config().map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "identity_status": identity.status,
        "cloud_configured": configuration.is_some(),
        "sync_existing_choice": configuration.as_ref().map(|item| item.1.as_str()).unwrap_or("undecided"),
        "sync": sync,
    })))
}

pub async fn sync_now_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some((base_url, _)) = server.db.cloud_connection_config().map_err(internal_error)? else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "cloud_not_configured",
            "请先登录并配置云同步",
        ));
    };
    let service = CloudSyncService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
        base_url,
    )
    .map_err(sync_error)?;
    let report = service.sync_now().await.map_err(sync_error)?;
    Ok(Json(serde_json::json!({"report": report, "sync": server.db.cloud_sync_status().map_err(internal_error)?})))
}

async fn authenticate(
    server: Arc<AppServer>,
    payload: AuthPayload,
    register: bool,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if payload.email.trim().is_empty() || payload.password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_credentials",
            "请输入邮箱和密码",
        ));
    }
    let identity = server.db.get_or_create_cloud_identity().map_err(internal_error)?;
    let client = CloudApiClient::new(&payload.cloud_base_url).map_err(cloud_client_error)?;
    let request = CloudAuthRequest {
        email: payload.email.trim().to_string(),
        password: payload.password,
        display_name: payload.display_name,
        device_id: identity.device_id,
        device_label: "Windows device".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let session = if register {
        client
            .register(request, &uuid::Uuid::new_v4().to_string())
            .await
    } else {
        client.login(request).await
    }
    .map_err(cloud_client_error)?;
    CloudSessionService::new(server.db.clone_connection(), Arc::clone(&server.secret_store))
        .persist_authenticated_session(session)
        .await
        .map_err(session_error)?;
    server
        .db
        .save_cloud_configuration(payload.cloud_base_url.trim_end_matches('/'), "undecided")
        .map_err(internal_error)?;
    identity_handler(State(server)).await
}

fn internal_error(_: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": "cloud_state_unavailable",
            "message": "本地云状态暂时不可用"
        })),
    )
}

fn cloud_client_error(error: CloudClientError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        CloudClientError::InvalidBaseUrl => api_error(
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudClientError::Authentication => api_error(
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_failed",
            "邮箱或密码不正确",
        ),
        CloudClientError::Unavailable => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_unavailable",
            "云服务暂时不可用，本地功能仍可继续使用",
        ),
        CloudClientError::Rejected | CloudClientError::InvalidResponse => api_error(
            StatusCode::BAD_GATEWAY,
            "cloud_request_failed",
            "云服务未能完成请求",
        ),
    }
}

fn session_error(_: CloudSessionError) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "cloud_session_not_saved",
        "登录凭据未能安全保存，登录状态未启用",
    )
}

fn sync_error(error: CloudSyncError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        CloudSyncError::AuthenticationRequired => api_error(
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_required",
            "云会话已失效，请重新登录；本地数据仍会保留",
        ),
        CloudSyncError::InvalidBaseUrl => api_error(
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudSyncError::Transport | CloudSyncError::InvalidResponse => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_sync_unavailable",
            "云同步暂时不可用，本地变更已保留",
        ),
        CloudSyncError::Database(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "cloud_sync_state_unavailable",
            "本地同步状态暂时不可用",
        ),
    }
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
