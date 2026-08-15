// ============================================================
// Workspaces API — CRUD + archive.
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppServer;
use crate::workspace::{Workspace, WorkspaceFieldError, WorkspaceId};

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub root_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub root_path: Option<String>,
}

fn view(workspace: &Workspace, active_tasks: usize) -> serde_json::Value {
    serde_json::json!({
        "id": workspace.id.as_str(),
        "name": workspace.name,
        "description": workspace.description,
        "root_path": workspace.root_path,
        "status": workspace.status.to_string(),
        "created_at": workspace.created_at,
        "updated_at": workspace.updated_at,
        "active_tasks": active_tasks,
    })
}

fn map_field_error(error: &WorkspaceFieldError) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, error.to_string())
}

pub async fn list_workspaces(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let workspaces = server.db.list_workspaces().unwrap_or_default();
    let views = workspaces
        .iter()
        .map(|w| view(w, server.db.count_active_tasks(&w.id).unwrap_or(0)))
        .collect::<Vec<_>>();
    Json(serde_json::json!({ "workspaces": views }))
}

pub async fn create_workspace(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let now = chrono::Utc::now().timestamp_millis();
    let workspace = Workspace::new(
        WorkspaceId::generate(),
        body.name,
        body.description,
        body.root_path,
        now,
    )
    .map_err(|error| map_field_error(&error))?;
    server
        .db
        .create_workspace(&workspace)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(view(&workspace, 0)))
}

pub async fn get_workspace(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = WorkspaceId::new(id).map_err(|error| map_field_error(&error))?;
    let workspace = server
        .db
        .get_workspace(&id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作空间不存在".to_string()))?;
    let active_tasks = server.db.count_active_tasks(&workspace.id).unwrap_or(0);
    Ok(Json(view(&workspace, active_tasks)))
}

pub async fn update_workspace(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = WorkspaceId::new(id).map_err(|error| map_field_error(&error))?;
    let existing = server
        .db
        .get_workspace(&id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作空间不存在".to_string()))?;

    let name = body.name.unwrap_or(existing.name);
    let description = body.description.unwrap_or(existing.description);
    let name = crate::workspace::validate_workspace_fields(&name, &description)
        .map_err(|error| map_field_error(&error))?;

    let now = chrono::Utc::now().timestamp_millis();
    let updated = Workspace {
        id: existing.id,
        name,
        description: description.trim().to_string(),
        root_path: body.root_path.or(existing.root_path),
        status: existing.status,
        created_at: existing.created_at,
        updated_at: now,
    };
    server
        .db
        .update_workspace(&updated)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let active_tasks = server.db.count_active_tasks(&updated.id).unwrap_or(0);
    Ok(Json(view(&updated, active_tasks)))
}

/// Archive a workspace (retains all history). DELETE keeps this same semantic.
pub async fn delete_workspace(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = WorkspaceId::new(id).map_err(|error| map_field_error(&error))?;
    let now = chrono::Utc::now().timestamp_millis();
    server
        .db
        .archive_workspace(&id, now)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(serde_json::json!({ "status": "archived" })))
}
