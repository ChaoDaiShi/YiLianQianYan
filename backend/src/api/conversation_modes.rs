use crate::{app::state::AppServer, db::conversation_modes::WorkspaceMode};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMode {
    pub mode: WorkspaceMode,
}
pub async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMode>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if !server
        .db
        .set_conversation_mode(&id, body.mode)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        return Err((StatusCode::NOT_FOUND, "会话不存在".into()));
    }
    Ok(Json(json!({"mode":body.mode})))
}
pub fn event(
    id: &str,
    mode: WorkspaceMode,
    automatic: bool,
    reason: &str,
) -> crate::agent::engine::AgentEvent {
    let mut event = crate::agent::engine::AgentEvent::error_event(id, String::new());
    event.event_type = "workspace_mode".into();
    event.error = None;
    event.args = Some(json!({"mode":mode,"automatic":automatic,"reason":reason}));
    event
}

pub fn router() -> axum::Router<Arc<AppServer>> {
    axum::Router::new().route("/api/conversations/:id/mode", axum::routing::put(update))
}
