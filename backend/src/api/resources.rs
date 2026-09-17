use crate::server::AppServer;
use crate::shared::resource::ResourceError;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
static RESOURCE_PARSERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(Deserialize)]
pub struct IngestQuery {
    name: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    50
}

pub async fn ingest_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<IngestQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let mime_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    if let Err(message) =
        crate::resource_input::validate_upload(&query.name, &mime_type, body.len(), 1)
    {
        return input_error(StatusCode::BAD_REQUEST, message);
    }
    let Ok(permit) = RESOURCE_PARSERS.try_acquire() else {
        return input_error(StatusCode::TOO_MANY_REQUESTS, "资源解析繁忙，请重试".into());
    };
    let service = server.resource_service.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::resource_input::ingest_resource(&service, &query.name, &mime_type, &body)
    })
    .await;
    drop(permit);
    match result {
        Ok(Ok(resource)) => (StatusCode::CREATED, Json(resource)).into_response(),
        Ok(Err(error)) => input_error(StatusCode::BAD_REQUEST, error),
        Err(_) => input_error(StatusCode::UNPROCESSABLE_ENTITY, "资源解析失败".into()),
    }
}

fn input_error(status: StatusCode, message: String) -> Response {
    (
        status,
        Json(json!({"error":"resource_input_error","message":message})),
    )
        .into_response()
}

pub async fn preview_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Response {
    let Ok(permit) = RESOURCE_PARSERS.try_acquire() else {
        return input_error(StatusCode::TOO_MANY_REQUESTS, "资源解析繁忙，请重试".into());
    };
    let service = server.resource_service.clone();
    let result =
        tokio::task::spawn_blocking(move || crate::resource_input::resource_preview(&service, &id))
            .await;
    drop(permit);
    match result {
        Ok(Ok(preview)) => Json(preview).into_response(),
        Ok(Err(error)) => input_error(StatusCode::NOT_FOUND, error),
        Err(_) => input_error(StatusCode::UNPROCESSABLE_ENTITY, "资源解析失败".into()),
    }
}

pub async fn content_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Response {
    match server.resource_service.read_bytes(&id) {
        Ok(bytes) => {
            let mime = server
                .resource_service
                .get(&id)
                .ok()
                .flatten()
                .map(|resource| resource.mime_type)
                .unwrap_or_else(|| "application/octet-stream".into());
            (
                [
                    (axum::http::header::CONTENT_TYPE, mime),
                    (axum::http::header::CONTENT_DISPOSITION, "attachment".into()),
                    (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
                ],
                bytes,
            )
                .into_response()
        }
        Err(_) => input_error(StatusCode::NOT_FOUND, "资源文件缺失或完整性检查失败".into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindRequest {
    pub target: crate::db::ResourceTarget,
    pub resource_ids: Vec<String>,
}
#[derive(Deserialize)]
pub struct BindingQuery {
    pub target: String,
}

pub async fn bind_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<BindRequest>,
) -> Response {
    match server
        .db
        .bind_resources(&request.target, &request.resource_ids)
    {
        Ok(bindings) => Json(json!({"bindings":bindings})).into_response(),
        Err(error) => input_error(StatusCode::BAD_REQUEST, error),
    }
}
pub async fn list_bindings_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<BindingQuery>,
) -> Response {
    let target: crate::db::ResourceTarget = match serde_json::from_str(&query.target) {
        Ok(target) => target,
        Err(_) => return input_error(StatusCode::BAD_REQUEST, "绑定目标无效".into()),
    };
    match server.db.resource_bindings(&target) {
        Ok(bindings) => Json(json!({"bindings":bindings})).into_response(),
        Err(error) => input_error(StatusCode::BAD_REQUEST, error),
    }
}
pub async fn unbind_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Response {
    match server.db.unbind_resource(&id) {
        Ok(removed) => Json(json!({"removed":removed})).into_response(),
        Err(error) => input_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListQuery>,
) -> Response {
    match server.resource_service.list(query.limit, query.offset) {
        Ok(resources) => Json(json!({ "resources": resources })).into_response(),
        Err(error) => resource_error_response(error),
    }
}

pub async fn get_handler(State(server): State<Arc<AppServer>>, Path(id): Path<String>) -> Response {
    match server.resource_service.get(&id) {
        Ok(Some(resource)) => Json(resource).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "resource_not_found", "message": "resource does not exist"})),
        )
            .into_response(),
        Err(error) => resource_error_response(error),
    }
}

fn resource_error_response(error: ResourceError) -> Response {
    let (status, code) = match error {
        ResourceError::Empty
        | ResourceError::TooLarge { .. }
        | ResourceError::InvalidName(_)
        | ResourceError::InvalidMetadata => (StatusCode::BAD_REQUEST, "invalid_resource"),
        ResourceError::Storage(_) | ResourceError::Persistence(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "resource_ingest_failed")
        }
    };
    (
        status,
        Json(json!({
            "error": code,
            "message": error.to_string(),
        })),
    )
        .into_response()
}
