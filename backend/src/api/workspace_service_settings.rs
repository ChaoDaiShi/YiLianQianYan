use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::state::AppServer,
    db::workspace_service_settings::{
        ServiceReference, ServiceSettingsError, WorkspaceServiceKind,
    },
    modules::settings::application::workspace_services::{
        settings_view, WorkspaceServiceSettingsView,
    },
    workspace::WorkspaceId,
};

type Error = (StatusCode, Json<Value>);

pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/workspaces/:id/service-settings/:kind",
            get(get_setting).put(save_setting),
        )
        .route("/api/settings/service-overrides", get(list_overrides))
}

fn error(error: ServiceSettingsError) -> Error {
    let (status, code) = match error {
        ServiceSettingsError::WorkspaceMissing => (StatusCode::NOT_FOUND, "workspace_not_found"),
        ServiceSettingsError::RevisionConflict => (StatusCode::CONFLICT, "revision_conflict"),
        ServiceSettingsError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_service_settings"),
        ServiceSettingsError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "service_settings_unavailable",
        ),
    };
    let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
        "无法访问工作空间服务设置".to_owned()
    } else {
        error.to_string()
    };
    (status, Json(json!({"error": message, "code": code})))
}

fn setting_path(id: String, kind: &str) -> Result<(WorkspaceId, WorkspaceServiceKind), Error> {
    let workspace = WorkspaceId::new(id)
        .map_err(|invalid| error(ServiceSettingsError::Invalid(invalid.to_string())))?;
    let kind = WorkspaceServiceKind::parse(kind).map_err(error)?;
    Ok((workspace, kind))
}

async fn get_setting(
    State(server): State<Arc<AppServer>>,
    Path((id, kind)): Path<(String, String)>,
) -> Result<Json<WorkspaceServiceSettingsView>, Error> {
    let (id, kind) = setting_path(id, &kind)?;
    let setting = server
        .db
        .get_workspace_service_settings(&id, kind)
        .map_err(error)?
        .ok_or_else(|| error(ServiceSettingsError::WorkspaceMissing))?;
    settings_view(&server, &id, kind, setting)
        .map(Json)
        .map_err(error)
}

#[derive(Deserialize)]
struct SaveSetting {
    expected_revision: u64,
    #[serde(rename = "override", deserialize_with = "Deserialize::deserialize")]
    override_value: Option<ServiceReference>,
}

async fn save_setting(
    State(server): State<Arc<AppServer>>,
    Path((id, kind)): Path<(String, String)>,
    Json(body): Json<SaveSetting>,
) -> Result<Json<WorkspaceServiceSettingsView>, Error> {
    let (id, kind) = setting_path(id, &kind)?;
    let setting = server
        .db
        .set_workspace_service_override(&id, kind, body.expected_revision, body.override_value)
        .map_err(error)?;
    settings_view(&server, &id, kind, setting)
        .map(Json)
        .map_err(error)
}

async fn list_overrides(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    let workspaces = server
        .db
        .list_workspace_service_overrides()
        .map_err(error)?;
    Ok(Json(json!({"workspaces": workspaces})))
}
