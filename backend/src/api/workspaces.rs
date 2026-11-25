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

use crate::app::state::AppServer;
use crate::workspace::{Workspace, WorkspaceFieldError, WorkspaceId, WorkspaceProject};

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub root_path: Option<String>,
    pub project: Option<WorkspaceProject>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub root_path: Option<String>,
    pub project: Option<WorkspaceProject>,
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultWorkspaceRequest {
    pub workspace_id: Option<String>,
}

fn view(workspace: &Workspace, active_tasks: usize) -> serde_json::Value {
    serde_json::json!({
        "id": workspace.id.as_str(),
        "name": workspace.name,
        "description": workspace.description,
        "root_path": workspace.root_path,
        "status": workspace.status.to_string(),
        "project": workspace.project,
        "created_at": workspace.created_at,
        "updated_at": workspace.updated_at,
        "active_tasks": active_tasks,
    })
}

pub(crate) fn router() -> axum::Router<Arc<AppServer>> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .merge(crate::api::workspace_pages::router())
        .route("/api/workspaces", get(list_workspaces))
        .route("/api/workspaces", post(create_workspace))
        .route("/api/workspaces/default", put(set_default_workspace))
        .route("/api/workspaces/:id", get(get_workspace))
        .route("/api/workspaces/:id", put(update_workspace))
        .route("/api/workspaces/:id", delete(delete_workspace))
        .route("/api/workspaces/:id/restore", post(restore_workspace))
        .route(
            "/api/workspaces/:id/registration",
            delete(remove_workspace_registration),
        )
}

fn map_field_error(error: &WorkspaceFieldError) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, error.to_string())
}

pub async fn list_workspaces(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let workspaces = server.db.list_workspaces().map_err(storage_error)?;
    let default = server.db.default_workspace_id().map_err(storage_error)?;
    let views = workspaces
        .iter()
        .map(|w| {
            let mut result = view(w, server.db.count_active_tasks(&w.id).unwrap_or(0));
            result["is_default"] = serde_json::json!(default.as_deref() == Some(w.id.as_str()));
            result
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "workspaces": views })))
}

fn storage_error(error: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}

pub async fn set_default_workspace(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<SetDefaultWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = body
        .workspace_id
        .map(WorkspaceId::new)
        .transpose()
        .map_err(|error| map_field_error(&error))?;
    let id = if let Some(id) = id {
        if !server
            .db
            .set_default_workspace(&id)
            .map_err(storage_error)?
        {
            return Err((StatusCode::BAD_REQUEST, "请选择尚未归档的工作空间".into()));
        }
        id.to_string()
    } else {
        server
            .db
            .reset_default_workspace(&server.workspace_root)
            .map_err(storage_error)?
    };
    Ok(Json(serde_json::json!({ "workspace_id": id })))
}

pub async fn create_workspace(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut workspace = Workspace::new(
        WorkspaceId::generate(),
        body.name,
        body.description,
        body.root_path,
        now,
    )
    .map_err(|error| map_field_error(&error))?;
    workspace.project = body
        .project
        .unwrap_or_default()
        .normalize()
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
    let update_project = body.project.is_some();
    let project = body
        .project
        .map(WorkspaceProject::normalize)
        .transpose()
        .map_err(|error| map_field_error(&error))?
        .unwrap_or(existing.project);
    let updated = Workspace {
        id: existing.id,
        name,
        description: description.trim().to_string(),
        root_path: body.root_path.or(existing.root_path),
        status: existing.status,
        project,
        created_at: existing.created_at,
        updated_at: now,
    };
    server
        .db
        .update_workspace_with_project(&updated, update_project)
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
    // Cancel software actions before the project becomes inactive. A blocked
    // UIA provider must not dispatch after the user revokes the project.
    server
        .software_runtime
        .revoke_workspace(id.as_str())
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let now = chrono::Utc::now().timestamp_millis();
    server
        .db
        .archive_workspace(&id, now, Some(&server.workspace_root))
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(serde_json::json!({ "status": "archived" })))
}

pub async fn restore_workspace(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = WorkspaceId::new(id).map_err(|error| map_field_error(&error))?;
    let now = chrono::Utc::now().timestamp_millis();
    if !server
        .db
        .restore_workspace(&id, now)
        .map_err(storage_error)?
    {
        return Err(archive_required(
            &server,
            &id,
            "只有已归档的工作空间可以恢复",
        ));
    }
    Ok(Json(serde_json::json!({ "status": "active" })))
}

pub async fn remove_workspace_registration(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = WorkspaceId::new(id).map_err(|error| map_field_error(&error))?;
    let now = chrono::Utc::now().timestamp_millis();
    if !server
        .db
        .remove_archived_workspace(&id, now)
        .map_err(storage_error)?
    {
        return Err(archive_required(&server, &id, "请先归档工作空间，再删除"));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

fn archive_required(server: &AppServer, id: &WorkspaceId, message: &str) -> (StatusCode, String) {
    match server.db.get_workspace(id) {
        Ok(None) => (StatusCode::NOT_FOUND, "工作空间不存在".into()),
        Ok(Some(_)) => (StatusCode::CONFLICT, message.into()),
        Err(error) => storage_error(error),
    }
}
