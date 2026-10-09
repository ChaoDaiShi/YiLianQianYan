//! Transport mapping — typed application errors to HTTP responses.
//!
//! The status codes and error codes here are the wire contract and must not
//! change. `runtime_error` is the pre-existing runtime mapping and is reused
//! rather than duplicated; `planning_error` moved here from `graph_routes`
//! because it is a mapping concern shared by graph and review routes.

use super::*;

use crate::modules::task::application::graph_service::CreateGraphError;
use crate::modules::task::application::review_service::ReviewGraphError;

/// A domain error carried by the task runtime.
pub(crate) fn runtime_error(error: TaskWorldRuntimeError) -> Response {
    let (status, code) = match &error {
        TaskWorldRuntimeError::GraphNotFound(_) | TaskWorldRuntimeError::CheckpointNotFound(_) => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        TaskWorldRuntimeError::GraphAlreadyExists(_) => (StatusCode::CONFLICT, "already_exists"),
        TaskWorldRuntimeError::StaleRevision { .. } => (StatusCode::CONFLICT, "stale_revision"),
        TaskWorldRuntimeError::StaleCanvasViewRevision { .. } => {
            (StatusCode::CONFLICT, "stale_view_revision")
        }
        TaskWorldRuntimeError::CommandExecutionActive { .. }
        | TaskWorldRuntimeError::CommandExecutionNotActive { .. } => {
            (StatusCode::CONFLICT, "command_execution_conflict")
        }
        TaskWorldRuntimeError::CommandRequiresExecution(_) => {
            (StatusCode::BAD_REQUEST, "command_execution_required")
        }
        TaskWorldRuntimeError::InvalidCommandResult(_) => {
            (StatusCode::BAD_REQUEST, "invalid_command_result")
        }
        TaskWorldRuntimeError::ExecutionPaused(_) => (StatusCode::CONFLICT, "task_paused"),
        TaskWorldRuntimeError::ExecutionCancelled(_) => (StatusCode::CONFLICT, "task_cancelled"),
        TaskWorldRuntimeError::ExecutionControlGenerationOverflow => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::Persistence(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::ExecutionPersistence(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_execution_error")
        }
        TaskWorldRuntimeError::Harness(_) => (StatusCode::BAD_REQUEST, "task_execution_error"),
        TaskWorldRuntimeError::CanvasViewRevisionOverflow => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::Graph(_)
        | TaskWorldRuntimeError::Supervisor(_)
        | TaskWorldRuntimeError::Canvas(_) => (StatusCode::BAD_REQUEST, "task_world_error"),
    };
    (
        status,
        Json(json!({
            "error": code,
            "message": error.to_string(),
        })),
    )
        .into_response()
}

/// A planning-shaped error: an explicit status, an error code, and a message.
pub(crate) fn planning_error(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({"error":code,"message":message}))).into_response()
}

/// Map a rejected graph creation onto the wire contract it had before the
/// orchestration moved into `application::graph_service`.
pub(crate) fn create_graph_error(error: CreateGraphError) -> Response {
    match error {
        CreateGraphError::PlanRequestConflict => planning_error(
            StatusCode::BAD_REQUEST,
            "invalid_plan_request",
            error.to_string(),
        ),
        CreateGraphError::AlreadyExists(graph_id) => {
            runtime_error(TaskWorldRuntimeError::GraphAlreadyExists(graph_id))
        }
        CreateGraphError::PlanningContextUnavailable => planning_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "planning_context_failed",
            error.to_string(),
        ),
        CreateGraphError::PlannerUnavailable(_) => planning_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "planner_unavailable",
            error.to_string(),
        ),
        CreateGraphError::InvalidPlan(_) => planning_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_plan",
            error.to_string(),
        ),
        CreateGraphError::Runtime(error) => runtime_error(error),
    }
}

/// Map a rejected graph review onto the wire contract it had before the
/// orchestration moved into `application::review_service`.
pub(crate) fn review_graph_error(error: ReviewGraphError) -> Response {
    match error {
        ReviewGraphError::NotFound(graph_id) => {
            runtime_error(TaskWorldRuntimeError::GraphNotFound(graph_id))
        }
        ReviewGraphError::StaleRevision { expected, actual } => {
            runtime_error(TaskWorldRuntimeError::StaleRevision { expected, actual })
        }
        ReviewGraphError::PlannerUnavailable(message) => planning_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "review_unavailable",
            message,
        ),
        ReviewGraphError::InvalidReview(message) => {
            planning_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_review", message)
        }
    }
}
