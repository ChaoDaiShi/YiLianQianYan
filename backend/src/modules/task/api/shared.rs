use super::*;

pub(crate) fn parse_graph_id(raw: String) -> Result<TaskGraphId, TaskWorldRuntimeError> {
    TaskGraphId::new(raw).map_err(TaskWorldRuntimeError::Graph)
}

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

pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
