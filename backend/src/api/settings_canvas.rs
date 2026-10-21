//! Canvas connections describe the settings workspace, never runtime feature enablement.
use crate::{
    app::state::AppServer,
    db::settings_canvas::{settings_canvas_presets, SettingsCanvasPreset, SettingsCanvasState},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[cfg(test)]
mod tests;

type Error = (StatusCode, Json<Value>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    canvas_id: Option<String>,
}

#[derive(Serialize)]
struct SettingsCanvasResponse {
    state: SettingsCanvasState,
    presets: Vec<SettingsCanvasPreset>,
}

fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (
        status,
        Json(json!({"code":"settings_canvas_error","error":message.into()})),
    )
}

fn response(
    server: &AppServer,
    state: SettingsCanvasState,
) -> Result<Json<SettingsCanvasResponse>, Error> {
    let mut presets =
        settings_canvas_presets().map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    presets.extend(
        super::plugin_settings::presets(&server.db)
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?,
    );
    Ok(Json(SettingsCanvasResponse { state, presets }))
}

pub fn router() -> Router<Arc<AppServer>> {
    Router::new().route("/api/settings/canvas", get(load).put(save))
}

async fn load(
    State(server): State<Arc<AppServer>>,
    Query(scope): Query<Scope>,
) -> Result<Json<SettingsCanvasResponse>, Error> {
    let state = server
        .db
        .settings_canvas_state(scope.canvas_id.as_deref())
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "画布不存在"))?;
    response(&server, state)
}

async fn save(
    State(server): State<Arc<AppServer>>,
    Query(scope): Query<Scope>,
    Json(state): Json<SettingsCanvasState>,
) -> Result<Json<SettingsCanvasResponse>, Error> {
    state
        .validate()
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    let previous = server
        .db
        .settings_canvas_state(scope.canvas_id.as_deref())
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "画布不存在"))?;
    let plugin_ids: Vec<String> = super::plugin_settings::presets(&server.db)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .into_iter()
        .map(|p| p.id)
        .collect();
    if state
        .connected_node_ids
        .iter()
        .chain(state.nodes.iter().map(|node| &node.id))
        .any(|id| {
            id.starts_with("plugin:")
                && !plugin_ids.contains(id)
                && !previous.connected_node_ids.contains(id)
                && !previous.nodes.iter().any(|node| &node.id == id)
        })
    {
        return Err(error(StatusCode::BAD_REQUEST, "插件设置不存在或插件未启用"));
    }
    let exists = server
        .db
        .save_settings_canvas_state(scope.canvas_id.as_deref(), &state)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "画布不存在"));
    }
    response(&server, state)
}
