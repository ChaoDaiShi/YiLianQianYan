use crate::{app::state::AppServer, db::access_rules::AccessRule};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRules {
    rules: Vec<AccessRule>,
}
type Error = (StatusCode, Json<Value>);
fn error(message: String) -> Error {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":message,"code":"invalid_access_rules"})),
    )
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/security/access-rules", get(list).put(save))
        .route(
            "/api/security/approval-mode",
            get(get_approval_mode).put(save_approval_mode),
        )
}
pub async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"rules":server.db.list_access_rules().map_err(error)?}),
    ))
}
pub async fn save(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SaveRules>,
) -> Result<Json<Value>, Error> {
    server
        .db
        .replace_access_rules(&request.rules)
        .map_err(error)?;
    Ok(Json(
        json!({"rules":server.db.list_access_rules().map_err(error)?}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalModeRequest {
    mode: crate::config::types::PermissionMode,
}
pub async fn get_approval_mode(State(server): State<Arc<AppServer>>) -> Json<Value> {
    Json(json!({"mode":server.config.read().permissions.mode}))
}
pub async fn save_approval_mode(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ApprovalModeRequest>,
) -> Result<Json<Value>, Error> {
    let mut config = server.config.write();
    let mut next = config.clone();
    next.permissions.mode = request.mode;
    server.db.save_settings(&next).map_err(error)?;
    *config = next;
    Ok(Json(json!({"mode":config.permissions.mode})))
}
