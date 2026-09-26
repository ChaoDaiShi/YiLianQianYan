use super::*;

use crate::modules::workflow::application::graph_service::GraphServiceError;
use crate::modules::workflow::application::run_service::RunServiceError;

pub(crate) fn graph_service_error(error: GraphServiceError) -> (StatusCode, String) {
    match error {
        GraphServiceError::Invalid(message) => (StatusCode::BAD_REQUEST, message),
        GraphServiceError::NotFound => (StatusCode::NOT_FOUND, "工作流图不存在".to_string()),
        GraphServiceError::Read(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        GraphServiceError::Persist(message) => (StatusCode::BAD_REQUEST, message),
    }
}

pub(crate) fn run_service_error(error: RunServiceError) -> (StatusCode, String) {
    match error {
        RunServiceError::InvalidId(message) => (StatusCode::BAD_REQUEST, message),
        RunServiceError::GraphNotFound => (StatusCode::NOT_FOUND, "工作流图不存在".to_string()),
        RunServiceError::RunNotFound => (StatusCode::NOT_FOUND, "工作流运行不存在".to_string()),
        RunServiceError::InvalidRun(message) => (StatusCode::BAD_REQUEST, message),
        RunServiceError::Read(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        RunServiceError::Persist(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
    }
}

pub(crate) fn run_view(graph_id: &str, run: &WorkflowRun) -> serde_json::Value {
    serde_json::json!({
        "run_id": run.run_id.as_str(),
        "workflow_graph_id": graph_id,
        "status": run.status.to_string(),
        "created_at": run.created_at,
        "updated_at": run.updated_at,
        "execution_id": run.execution_context.execution_id.as_str(),
        "subject_id": run.execution_context.subject_id,
        "nodes": run.node_states.iter().map(|s| serde_json::json!({
            "node_id": s.node_id.as_str(),
            "status": s.status.to_string(),
            "started_at": s.started_at,
            "finished_at": s.finished_at,
            "error": s.error,
            "result": s.result.as_ref().map(|r| serde_json::json!({ "summary": r.summary })),
        })).collect::<Vec<_>>(),
    })
}

pub(crate) fn graph_view(record: &WorkflowGraphRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "name": record.name,
        "description": record.description,
        "definition": record.definition,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}
