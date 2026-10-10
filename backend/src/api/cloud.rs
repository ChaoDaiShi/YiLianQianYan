use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::modules::cloud::{
    client::{CloudApiClient, CloudClientError},
    domain::CloudAuthRequest,
    lifecycle::{CloudLifecycleError, CloudLifecycleService},
    session::{CloudSessionError, CloudSessionService},
    sync::{CloudSyncError, CloudSyncService},
};

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

#[derive(serde::Deserialize)]
pub struct AuthPayload {
    cloud_base_url: String,
    email: String,
    password: String,
    display_name: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct SyncChoicePayload {
    choice: String,
}
#[derive(serde::Deserialize)]
pub struct ResolveConflictPayload {
    pub resolution: Option<String>,
    pub choice: Option<crate::modules::cloud::contracts::ResolutionChoice>,
    pub expected_conflict_version: Option<i64>,
    pub expected_remote_revision: Option<i64>,
    pub request_id: Option<String>,
}

pub async fn identity_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let identity = server
        .db
        .get_or_create_cloud_identity()
        .map_err(internal_error)?;
    let sync = server.db.cloud_sync_status().map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "identity": identity,
        "sync": sync,
        "sync_existing_choice": server.db.cloud_sync_existing_choice().map_err(internal_error)?,
    })))
}

pub async fn sync_choice_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<SyncChoicePayload>,
) -> ApiResult {
    if !matches!(payload.choice.as_str(), "sync" | "local_only" | "undecided") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_sync_choice",
            "同步选择无效",
        ));
    }
    server
        .db
        .set_sync_existing_choice(&payload.choice)
        .map_err(internal_error)?;
    sync_status_handler(State(server)).await
}

pub async fn sync_conflicts_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let legacy = server.db.list_sync_conflicts().map_err(internal_error)?;
    let records = server.db.list_conflict_records().map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "conflicts": legacy,
        "records": records,
    })))
}

pub async fn resolve_sync_conflict_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(payload): Json<ResolveConflictPayload>,
) -> ApiResult {
    if let Some(choice) = payload.choice {
        let lease = server.scoped_db.lease().clone();
        let expected_conflict_version = payload.expected_conflict_version.unwrap_or(1);
        let expected_remote_revision = payload.expected_remote_revision.unwrap_or(1);
        let request_id = payload
            .request_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let request = crate::modules::cloud::contracts::ResolveConflictRequest {
            conflict_id: id.clone(),
            expected_conflict_version,
            expected_remote_revision,
            choice,
            request_id,
        };
        let identity = server
            .db
            .get_or_create_cloud_identity()
            .map_err(internal_error)?;
        let result = crate::modules::cloud::conflict_service::resolve_conflict(
            &server.scoped_db,
            &lease,
            &request,
            &identity.device_id,
        )
        .map_err(|e| sync_error(CloudSyncError::Sync(e)))?;
        return Ok(Json(serde_json::json!({
            "result": result,
            "conflicts": server.db.list_sync_conflicts().map_err(internal_error)?,
            "records": server.db.list_conflict_records().map_err(internal_error)?,
        })));
    }

    let resolution = payload.resolution.as_deref().unwrap_or("local");
    if !matches!(resolution, "local" | "remote") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_conflict_resolution",
            "冲突处理方式无效",
        ));
    }
    server
        .db
        .resolve_sync_conflict(&id, resolution)
        .map_err(internal_error)?;
    sync_conflicts_handler(State(server)).await
}

pub async fn login_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> ApiResult {
    authenticate(server, payload, false).await
}

pub async fn register_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> ApiResult {
    authenticate(server, payload, true).await
}

pub async fn refresh_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    refresh_session(&server).await?;
    identity_handler(State(server)).await
}

pub async fn logout_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let lease = server.scoped_db.lease().clone();
    let conn_config = server
        .db
        .cloud_connection_config()
        .map_err(internal_error)?;
    let session_id = server
        .db
        .cloud_current_session_id()
        .map_err(internal_error)?;
    let identity = server.db.cloud_identity().map_err(internal_error)?;
    let account_id = identity.as_ref().and_then(|id| id.account_id.as_deref());

    let service = CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    );

    let report = service
        .logout_with_report(
            &lease,
            session_id.as_deref(),
            conn_config.as_ref().map(|c| c.0.as_str()),
            account_id,
        )
        .await;

    let updated_identity = server
        .db
        .get_or_create_cloud_identity()
        .map_err(internal_error)?;
    let sync = server.db.cloud_sync_status().map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "report": report,
        "identity": updated_identity,
        "sync": sync,
        "sync_existing_choice": server.db.cloud_sync_existing_choice().map_err(internal_error)?,
    })))
}

#[derive(serde::Deserialize)]
pub struct ProfileSwitchPayload {
    pub service_origin: String,
    pub account_id: String,
    pub choice: Option<crate::modules::cloud::profile::ActivationChoice>,
}

fn resolve_data_dir() -> std::path::PathBuf {
    std::env::var("YILIAN_DATA_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("yilianqianyan-v1")))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn cloud_base_url(server: &AppServer) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let (url, _) = server
        .db
        .cloud_connection_config()
        .map_err(internal_error)?
        .ok_or_else(|| {
            api_error(
                StatusCode::CONFLICT,
                "cloud_not_configured",
                "请先登录云账号",
            )
        })?;
    Ok(url)
}

pub async fn profile_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let data_dir = resolve_data_dir();

    let registry = crate::modules::cloud::profile::ProfileManager::load_or_initialize(&data_dir)
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;

    Ok(Json(serde_json::json!({
        "active_profile_id": registry.active_profile_id,
        "active_generation": registry.active_generation,
        "active_lease": server.scoped_db.lease(),
        "profiles": registry.profiles.values().collect::<Vec<_>>(),
    })))
}

pub async fn profile_switch_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<ProfileSwitchPayload>,
) -> ApiResult {
    let data_dir = resolve_data_dir();

    let scope_key = crate::modules::cloud::contracts::ScopeKey {
        service_origin: payload.service_origin,
        account_id: payload.account_id,
    };
    let choice = payload
        .choice
        .unwrap_or(crate::modules::cloud::profile::ActivationChoice::OpenAccountOnly);

    let plan = crate::modules::cloud::profile::ProfileManager::activate_profile(
        &data_dir,
        &server,
        &scope_key,
        choice,
        std::time::Duration::from_secs(5),
    )
    .await
    .map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "profile_switch_failed",
            &e.to_string(),
        )
    })?;

    Ok(Json(serde_json::json!({
        "plan": plan,
        "status": "awaiting_restart"
    })))
}

pub async fn sessions_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let base_url = cloud_base_url(&server)?;
    let sessions = CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    )
    .list_remote_sessions(&base_url)
    .await
    .map_err(session_error)?;
    Ok(Json(serde_json::json!({"sessions":sessions})))
}

pub async fn revoke_session_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> ApiResult {
    let base_url = cloud_base_url(&server)?;
    CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    )
    .revoke_remote_session(&base_url, &id)
    .await
    .map_err(session_error)?;
    sessions_handler(State(server)).await
}

pub async fn sync_status_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let identity = server
        .db
        .get_or_create_cloud_identity()
        .map_err(internal_error)?;
    let sync = server.db.cloud_sync_status().map_err(internal_error)?;
    let configuration = server
        .db
        .cloud_connection_config()
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "identity_status": identity.status,
        "cloud_configured": configuration.is_some(),
        "sync_existing_choice": configuration.as_ref().map(|item| item.1.as_str()).unwrap_or("undecided"),
        "sync": sync,
    })))
}

pub async fn sync_now_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    if server
        .db
        .cloud_sync_existing_choice()
        .map_err(internal_error)?
        != "sync"
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "sync_not_enabled",
            "请先明确选择同步已有数据",
        ));
    }
    let base_url = cloud_base_url(&server)?;
    let service = CloudSyncService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
        base_url,
    )
    .map_err(sync_error)?;
    server
        .db
        .capture_sync_candidates()
        .map_err(internal_error)?;
    let report = match service.sync_now().await {
        Ok(report) => report,
        Err(CloudSyncError::AuthenticationRequired) => {
            refresh_session(&server).await?;
            service.sync_now().await.map_err(sync_error)?
        }
        Err(error) => return Err(sync_error(error)),
    };
    Ok(Json(
        serde_json::json!({"report": report, "sync": server.db.cloud_sync_status().map_err(internal_error)?}),
    ))
}

pub async fn privacy_summary_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let service = lifecycle_service(&server)?;
    let summary = service.data_summary().await.map_err(lifecycle_error)?;
    Ok(Json(
        serde_json::json!({"summary":summary,"identity":server.db.get_or_create_cloud_identity().map_err(internal_error)?}),
    ))
}

pub async fn request_export_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let job = lifecycle_service(&server)?
        .request_export()
        .await
        .map_err(lifecycle_error)?;
    Ok(Json(serde_json::json!({"job":job})))
}

pub async fn request_deletion_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let job = lifecycle_service(&server)?
        .request_account_deletion()
        .await
        .map_err(lifecycle_error)?;
    server
        .db
        .mark_cloud_identity_closing()
        .map_err(internal_error)?;
    Ok(Json(
        serde_json::json!({"job":job,"identity":server.db.cloud_identity().map_err(internal_error)?}),
    ))
}

pub async fn lifecycle_job_handler(
    State(server): State<Arc<AppServer>>,
    Path((kind, id)): Path<(String, String)>,
) -> ApiResult {
    let job = lifecycle_service(&server)?
        .job(&kind, &id)
        .await
        .map_err(lifecycle_error)?;
    Ok(Json(serde_json::json!({"job":job})))
}

pub async fn download_export_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let service = lifecycle_service(&server)?;
    let (filename, bytes) = service
        .download_export(&id)
        .await
        .map_err(lifecycle_error)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "download_failed",
                "导出文件读取失败",
            )
        })
}

fn lifecycle_service(
    server: &Arc<AppServer>,
) -> Result<CloudLifecycleService, (StatusCode, Json<serde_json::Value>)> {
    let base_url = cloud_base_url(server)?;
    CloudLifecycleService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
        base_url,
    )
    .map_err(lifecycle_error)
}

async fn refresh_session(
    server: &Arc<AppServer>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let base_url = cloud_base_url(server)?;
    CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    )
    .refresh(&base_url)
    .await
    .map_err(session_error)
}

async fn authenticate(server: Arc<AppServer>, payload: AuthPayload, register: bool) -> ApiResult {
    if payload.email.trim().is_empty() || payload.password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_credentials",
            "请输入邮箱和密码",
        ));
    }
    let identity = server
        .db
        .get_or_create_cloud_identity()
        .map_err(internal_error)?;
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
    CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    )
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
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "cloud_state_unavailable",
        "本地云状态暂时不可用",
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
    use crate::modules::cloud::contracts::SyncError;
    match error {
        CloudSyncError::AuthenticationRequired
        | CloudSyncError::Sync(SyncError::AuthenticationRequired) => api_error(
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
        CloudSyncError::Scope(_) | CloudSyncError::Sync(SyncError::ScopeChanged) => {
            api_error(StatusCode::CONFLICT, "scope_changed", "账号或作用域已改变")
        }
        CloudSyncError::Sync(SyncError::ProtocolUpgradeRequired) => api_error(
            StatusCode::UPGRADE_REQUIRED,
            "sync_upgrade_required",
            "云同步协议版本不匹配，需要升级",
        ),
        CloudSyncError::Sync(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "cloud_sync_failed",
            "云同步执行失败",
        ),
    }
}

fn lifecycle_error(error: CloudLifecycleError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        CloudLifecycleError::AuthenticationRequired => api_error(
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_required",
            "云会话已失效，请重新登录",
        ),
        CloudLifecycleError::InvalidBaseUrl => api_error(
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudLifecycleError::Unavailable | CloudLifecycleError::InvalidResponse => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_lifecycle_unavailable",
            "云端数据生命周期服务暂时不可用",
        ),
        CloudLifecycleError::NotFound => api_error(
            StatusCode::NOT_FOUND,
            "export_not_found",
            "导出任务或文件不存在",
        ),
        CloudLifecycleError::Expired => api_error(
            StatusCode::GONE,
            "export_expired",
            "导出文件已过期，请重新申请导出",
        ),
        CloudLifecycleError::Database(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "lifecycle_state_unavailable",
            "本地生命周期状态暂时不可用",
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
