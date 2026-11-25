//! Read-only metadata about the last accepted chat turn's context assembly.
use crate::{
    app::state::AppServer,
    execution::context_manifest::{load_conversation_manifest, ConversationContextSnapshot},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

pub async fn get_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<ConversationContextSnapshot>, (StatusCode, String)> {
    // Reject orphan metadata after conversation deletion. No prompt bodies are returned.
    let exists: bool = server
        .db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [&id],
            |row| row.get(0),
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "conversation_read_failed".into(),
            )
        })?;
    if !exists {
        return Err((StatusCode::NOT_FOUND, "conversation_not_found".into()));
    }
    load_conversation_manifest(&server.db, &id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or((
            StatusCode::NOT_FOUND,
            "context_manifest_not_available".into(),
        ))
}

pub async fn get_workflow_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let run_id = crate::modules::workflow::WorkflowRunId::new(&id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid_workflow_run_id".into()))?;
    let stored = server
        .db
        .get_workflow_run(&run_id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "workflow_run_read_failed".into(),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "workflow_run_not_found".into()))?;
    let owner = crate::execution::ExecutionOwner::Workflow {
        workflow_graph_id: stored.workflow_graph_id,
        run_id: id,
    };
    let manifest = crate::execution::context_manifest::load_manifest(&server.db, &owner)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or((
            StatusCode::NOT_FOUND,
            "context_manifest_not_available".into(),
        ))?;
    Ok(Json(serde_json::json!({"owner":owner,"manifest":manifest})))
}
