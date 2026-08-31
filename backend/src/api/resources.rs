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
        .unwrap_or("application/octet-stream");
    match server.resource_service.ingest(
        &query.name,
        mime_type,
        &body,
        json!({"source": "file-picker"}),
    ) {
        Ok(resource) => (StatusCode::CREATED, Json(resource)).into_response(),
        Err(error) => resource_error_response(error),
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
