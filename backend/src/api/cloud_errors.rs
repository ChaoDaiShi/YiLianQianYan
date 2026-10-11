use axum::{http::StatusCode, Json};

use crate::modules::cloud::{
    client::CloudClientError,
    contracts::{ScopeError, SyncError},
    revocation::LogoutReport,
    session::CloudSessionError,
    sync::CloudSyncError,
};

pub(super) fn profile_error(error: ScopeError) -> (StatusCode, Json<serde_json::Value>) {
    tracing::warn!(%error, "cloud profile activation failed");
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "profile_activation_failed",
        "账号数据空间切换失败，原有数据空间保持不变",
    )
}

pub(super) fn local_logout_incomplete(
    report: LogoutReport,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "code": "local_logout_incomplete",
            "error": "本地退出未完成，请重试",
            "message": "本地退出未完成，请重试",
            "report": report
        })),
    )
}

pub(super) fn internal_error(_: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "cloud_state_unavailable",
        "本地云状态暂时不可用",
    )
}

pub(super) fn cloud_client_error(error: CloudClientError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code, message) = match error {
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
    api_error(status, code, message)
}

pub(super) fn session_error(error: CloudSessionError) -> (StatusCode, Json<serde_json::Value>) {
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

pub(super) fn sync_error(error: CloudSyncError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code, message) = match error {
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
    api_error(status, code, message)
}

pub(super) fn api_error(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "code": code,
            "error": message,
            "message": message,
        })),
    )
}
