use axum::{
    extract::{Path, State},
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

pub use crate::api::cloud_lifecycle::{
    download_export_handler, lifecycle_job_handler, privacy_summary_handler,
    request_deletion_handler, request_export_handler,
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

pub async fn sync_policy_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let policy = server.db.cloud_sync_policy().map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "policy": policy })))
}

pub async fn update_sync_policy_handler(
    State(server): State<Arc<AppServer>>,
    Json(policy): Json<crate::db::cloud::capture::SyncPolicy>,
) -> ApiResult {
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;

    if policy.sync_conversations != policy.sync_messages
        || policy.sync_task_graphs != policy.sync_canvas_views
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "sync_policy_group_mismatch",
            "同步类别分组必须保持一致",
        ));
    }

    server
        .db
        .save_cloud_sync_policy(&policy)
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "policy": policy })))
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
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;

    let data_dir = server.app_data_dir.clone();
    let lease = server.scoped_db.lease();

    // Persist reauthentication_required = true for current active account
    let registry = crate::modules::cloud::profile::ProfileManager::load_or_initialize(&data_dir)
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;
    let active_profile_id = registry.active_profile_id.clone();
    let is_account = registry
        .profiles
        .get(&active_profile_id)
        .map(|e| e.kind == crate::modules::cloud::profile::ProfileKind::Account)
        .unwrap_or(false);

    if is_account {
        crate::modules::cloud::profile::ProfileManager::set_reauthentication_required(
            &data_dir,
            &active_profile_id,
            true,
        )
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;
    }

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

    let mut report = service
        .logout_with_report(
            &lease,
            session_id.as_deref(),
            conn_config.as_ref().map(|c| c.0.as_str()),
            account_id,
        )
        .await;

    let activation_res =
        crate::modules::cloud::profile::ProfileManager::prepare_guest_activation(&data_dir);
    let activation = match activation_res {
        Ok(plan) => {
            let act = crate::modules::cloud::profile::ProfileManager::activate_prepared_profile(
                &data_dir,
                &server,
                &plan,
                std::time::Duration::from_secs(5),
            )
            .await;
            match act {
                Ok(()) => Some(plan),
                Err(_) => {
                    report.error_codes.push("GUEST_ACTIVATION_FAILED".into());
                    report.local_cleanup = crate::modules::cloud::revocation::LocalCleanup::Failed;
                    None
                }
            }
        }
        Err(_) => {
            report.error_codes.push("GUEST_ACTIVATION_FAILED".into());
            report.local_cleanup = crate::modules::cloud::revocation::LocalCleanup::Failed;
            None
        }
    };

    report.error_codes.sort();
    report.error_codes.dedup();
    report.local_signed_out = report.local_cleanup
        == crate::modules::cloud::revocation::LocalCleanup::Cleared
        && activation.is_some();

    if !report.local_signed_out {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "local_logout_incomplete",
                "error": "本地退出未完成，请重试",
                "message": "本地退出未完成，请重试",
                "report": report
            })),
        ));
    }

    Ok(Json(serde_json::json!({
        "report": report,
        "activation": activation.unwrap(),
        "status": "awaiting_restart"
    })))
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct ProfileSwitchPayload {
    pub service_origin: String,
    pub account_id: String,
    pub choice: Option<crate::modules::cloud::profile::ActivationChoice>,
}

pub(crate) fn cloud_base_url(
    server: &AppServer,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
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
    let data_dir = server.app_data_dir.clone();

    let registry = crate::modules::cloud::profile::ProfileManager::load_or_initialize(&data_dir)
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;

    let pending_revocations: Vec<serde_json::Value> = server
        .db
        .conn()
        .prepare("SELECT session_id, last_error_code FROM cloud_pending_revocations")
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                let session_id: String = row.get(0)?;
                let last_error_code: Option<String> = row.get(1)?;
                let remote_revocation = match last_error_code.as_deref() {
                    Some("RETRY_EXHAUSTED") | Some("REVOKE_CREDENTIAL_MISSING") => "unknown",
                    _ => "pending",
                };
                Ok(serde_json::json!({
                    "session_id": session_id,
                    "remote_revocation": remote_revocation,
                    "last_error_code": last_error_code,
                }))
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "active_profile_id": registry.active_profile_id,
        "active_generation": registry.active_generation,
        "active_lease": server.scoped_db.lease(),
        "profiles": registry.profiles.values().collect::<Vec<_>>(),
        "pending_revocations": pending_revocations,
    })))
}

pub async fn profile_switch_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_payload): Json<ProfileSwitchPayload>,
) -> ApiResult {
    Err(api_error(
        StatusCode::UNAUTHORIZED,
        "profile_authentication_required",
        "请登录目标账号后切换资料库",
    ))
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
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;

    server
        .scoped_db
        .validate_stored_scope()
        .map_err(|error| sync_error(CloudSyncError::Sync(error)))?;
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
    let lease = server.scoped_db.lease();
    let report = match service.sync_now_scoped(&server.scoped_db, &lease).await {
        Ok(report) => report,
        Err(CloudSyncError::AuthenticationRequired) => {
            refresh_session(&server).await?;
            service
                .sync_now_scoped(&server.scoped_db, &lease)
                .await
                .map_err(sync_error)?
        }
        Err(error) => return Err(sync_error(error)),
    };
    Ok(Json(
        serde_json::json!({"report": report, "sync": server.db.cloud_sync_status().map_err(internal_error)?}),
    ))
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
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;
    if payload.email.trim().is_empty() || payload.password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_credentials",
            "请输入邮箱和密码",
        ));
    }
    let device_id = uuid::Uuid::new_v4().to_string();
    let client = CloudApiClient::new(&payload.cloud_base_url).map_err(cloud_client_error)?;
    let request = CloudAuthRequest {
        email: payload.email.trim().to_string(),
        password: payload.password,
        display_name: payload.display_name,
        device_id: device_id.clone(),
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
    let data_dir = server.app_data_dir.clone();
    let scope_key = crate::modules::cloud::contracts::ScopeKey {
        service_origin: payload.cloud_base_url.trim_end_matches('/').to_string(),
        account_id: session.account_id.clone(),
    };
    let plan = crate::modules::cloud::profile::ProfileManager::prepare_profile(
        &data_dir,
        &scope_key,
        crate::modules::cloud::profile::ActivationChoice::OpenAccountOnly,
    )
    .map_err(profile_error)?;
    let target_db = crate::db::Database::new(
        &crate::modules::cloud::profile::ProfileManager::profile_db_path(&data_dir, &plan),
    )
    .map_err(internal_error)?;
    target_db
        .set_cloud_device_id(&device_id)
        .map_err(internal_error)?;
    CloudSessionService::new(target_db.clone(), Arc::clone(&server.secret_store))
        .persist_authenticated_session_scoped(session, &plan.lease.scope_id)
        .await
        .map_err(session_error)?;
    target_db
        .save_cloud_configuration(scope_key.service_origin.as_str(), "undecided")
        .map_err(internal_error)?;
    let _ = crate::modules::cloud::profile::ProfileManager::set_reauthentication_required(
        &data_dir,
        &plan.profile_id,
        false,
    );
    crate::modules::cloud::profile::ProfileManager::activate_prepared_profile(
        &data_dir,
        &server,
        &plan,
        std::time::Duration::from_secs(5),
    )
    .await
    .map_err(profile_error)?;
    Ok(Json(serde_json::json!({
        "activation": plan,
        "status": "awaiting_restart"
    })))
}

fn profile_error(
    error: crate::modules::cloud::contracts::ScopeError,
) -> (StatusCode, Json<serde_json::Value>) {
    tracing::warn!(%error, "cloud profile activation failed");
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "profile_activation_failed",
        "账号数据空间切换失败，原有数据空间保持不变",
    )
}

fn internal_error(_: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "cloud_state_unavailable",
        "本地云状态暂时不可用",
    )
}

fn cloud_client_error(error: CloudClientError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code, msg) = match error {
        CloudClientError::InvalidBaseUrl => (
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudClientError::HttpsRequired => (
            StatusCode::BAD_REQUEST,
            "cloud_https_required",
            "远程云服务地址必须使用 HTTPS",
        ),
        CloudClientError::Authentication => (
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_failed",
            "邮箱或密码不正确",
        ),
        CloudClientError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_unavailable",
            "云服务暂时不可用，本地功能仍可继续使用",
        ),
        CloudClientError::Rejected | CloudClientError::InvalidResponse => (
            StatusCode::BAD_GATEWAY,
            "cloud_request_failed",
            "云服务未能完成请求",
        ),
    };
    api_error(status, code, msg)
}

fn session_error(error: CloudSessionError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        CloudSessionError::HttpsRequired => api_error(
            StatusCode::BAD_REQUEST,
            "cloud_https_required",
            "远程云服务地址必须使用 HTTPS",
        ),
        CloudSessionError::InvalidBaseUrl => api_error(
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "cloud_session_not_saved",
            "登录凭据未能安全保存，登录状态未启用",
        ),
    }
}

fn sync_error(error: CloudSyncError) -> (StatusCode, Json<serde_json::Value>) {
    use crate::modules::cloud::contracts::SyncError;
    let (status, code, msg) = match error {
        CloudSyncError::AuthenticationRequired
        | CloudSyncError::Sync(SyncError::AuthenticationRequired) => (
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_required",
            "云会话已失效，请重新登录；本地数据仍会保留",
        ),
        CloudSyncError::InvalidBaseUrl => (
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudSyncError::HttpsRequired => (
            StatusCode::BAD_REQUEST,
            "cloud_https_required",
            "远程云服务地址必须使用 HTTPS",
        ),
        CloudSyncError::Transport | CloudSyncError::InvalidResponse => (
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_sync_unavailable",
            "云同步暂时不可用，本地变更已保留",
        ),
        CloudSyncError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "cloud_sync_state_unavailable",
            "本地同步状态暂时不可用",
        ),
        CloudSyncError::Scope(_) | CloudSyncError::Sync(SyncError::ScopeChanged) => {
            (StatusCode::CONFLICT, "scope_changed", "账号或作用域已改变")
        }
        CloudSyncError::Sync(SyncError::ProtocolUpgradeRequired) => (
            StatusCode::UPGRADE_REQUIRED,
            "sync_upgrade_required",
            "云同步协议版本不匹配，需要升级",
        ),
        CloudSyncError::Sync(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "cloud_sync_failed",
            "云同步执行失败",
        ),
    };
    api_error(status, code, msg)
}

fn api_error(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "code": error,
            "error": message,
            "message": message,
        })),
    )
}
