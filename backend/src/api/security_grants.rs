// ============================================================
// Security grants API — protected CRUD + isolation status.
//
// Configuration only: grants authorize resources; they never execute anything.
// No generic execution endpoint exists here.
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::safety::grant::{validate_grant, GrantEffect, GrantResource, SecurityGrant};
use crate::safety::PermissionId;
use crate::server::AppServer;

const LOCAL_SUBJECT: &str = "local-user";

#[derive(Deserialize)]
pub struct CreateGrantRequest {
    pub permission_id: String,
    pub effect: String,
    pub resource: GrantResource,
}

fn parse_permission(s: &str) -> Result<PermissionId, String> {
    serde_json::from_value(serde_json::json!(s)).map_err(|e| e.to_string())
}

fn parse_effect(s: &str) -> Result<GrantEffect, String> {
    serde_json::from_value(serde_json::json!(s)).map_err(|e| e.to_string())
}

pub async fn list_grants(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let grants = server.db.list_grants(LOCAL_SUBJECT).unwrap_or_default();
    Json(serde_json::json!({ "grants": grants }))
}

pub async fn create_grant(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateGrantRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let permission =
        parse_permission(&body.permission_id).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let effect = parse_effect(&body.effect).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    validate_grant(permission, &body.resource).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let grant = SecurityGrant {
        id: uuid::Uuid::new_v4().to_string(),
        subject_id: LOCAL_SUBJECT.to_string(),
        effect,
        permission,
        resource: body.resource,
        source: crate::safety::grant::GrantSource::User,
        created_at: chrono::Utc::now().timestamp_millis(),
        expires_at: None,
    };
    server
        .db
        .create_grant(&grant)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!(grant)))
}

pub async fn delete_grant(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    server
        .db
        .delete_grant(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn isolation_status(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let status = crate::isolation::isolation_status();
    Json(serde_json::json!({
        "backend": status.backend,
        "process_containment": status.process_containment,
        "restricted_token": status.restricted_token,
        "kill_tree": status.kill_tree,
        "filesystem_os_enforced": status.filesystem_os_enforced,
        "network_os_enforced": status.network_os_enforced,
        "experimental_appcontainer_available": false,
    }))
}
