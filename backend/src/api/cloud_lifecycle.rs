use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::modules::cloud::lifecycle::{CloudLifecycleError, CloudLifecycleService};

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

pub async fn privacy_summary_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let service = lifecycle_service(&server)?;
    let summary = service.data_summary().await.map_err(lifecycle_error)?;
    Ok(Json(
        serde_json::json!({"summary": summary, "identity": server.db.get_or_create_cloud_identity().map_err(internal_error)?}),
    ))
}

pub async fn request_export_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let job = lifecycle_service(&server)?
        .request_export()
        .await
        .map_err(lifecycle_error)?;
    Ok(Json(serde_json::json!({"job": job})))
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
        serde_json::json!({"job": job, "identity": server.db.cloud_identity().map_err(internal_error)?}),
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
    Ok(Json(serde_json::json!({"job": job})))
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
    let base_url = crate::api::cloud::cloud_base_url(server)?;
    CloudLifecycleService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
        base_url,
    )
    .map_err(lifecycle_error)
}

fn lifecycle_error(error: CloudLifecycleError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code, msg) = match error {
        CloudLifecycleError::AuthenticationRequired => (
            StatusCode::UNAUTHORIZED,
            "cloud_authentication_required",
            "云会话已失效，请重新登录",
        ),
        CloudLifecycleError::InvalidBaseUrl => (
            StatusCode::BAD_REQUEST,
            "invalid_cloud_url",
            "云服务地址无效",
        ),
        CloudLifecycleError::HttpsRequired => (
            StatusCode::BAD_REQUEST,
            "cloud_https_required",
            "远程云服务地址必须使用 HTTPS",
        ),
        CloudLifecycleError::Unavailable | CloudLifecycleError::InvalidResponse => (
            StatusCode::SERVICE_UNAVAILABLE,
            "cloud_lifecycle_unavailable",
            "云端数据生命周期服务暂时不可用",
        ),
        CloudLifecycleError::NotFound => (
            StatusCode::NOT_FOUND,
            "export_not_found",
            "导出任务或文件不存在",
        ),
        CloudLifecycleError::Expired => (
            StatusCode::GONE,
            "export_expired",
            "导出文件已过期，请重新申请导出",
        ),
        CloudLifecycleError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "lifecycle_state_unavailable",
            "本地生命周期状态暂时不可用",
        ),
    };
    api_error(status, code, msg)
}

fn internal_error(_: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "cloud_state_unavailable",
        "本地云状态暂时不可用",
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
