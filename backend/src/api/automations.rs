use crate::{
    app::state::AppServer,
    modules::automation::{self, model::AutomationInput},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

type ApiError = (StatusCode, String);
fn bad(error: String) -> ApiError {
    (StatusCode::BAD_REQUEST, error)
}
fn internal(error: String) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}

pub async fn list(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "automations": server.db.list_automations().map_err(internal)? }),
    ))
}
pub async fn create(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<AutomationInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let value = automation::save(&server, None, body).map_err(bad)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "automation": value })),
    ))
}
pub async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<AutomationInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "automation": automation::save(&server, Some(&id), body).map_err(bad)? }),
    ))
}
pub async fn delete(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "deleted": server.db.delete_automation(&id).map_err(internal)? }),
    ))
}
#[derive(Deserialize)]
pub struct EnabledRequest {
    pub enabled: bool,
    pub expected_revision: u64,
}
pub async fn enabled(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<EnabledRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "automation": automation::set_enabled(&server, &id, body.enabled, body.expected_revision).map_err(bad)? }),
    ))
}
pub async fn run(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "occurrence": automation::run_now(&server, &id).await.map_err(bad)? }),
    ))
}
pub async fn history(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "occurrences": automation::history(&server, &id).map_err(internal)? }),
    ))
}
