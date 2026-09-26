use super::*;

pub(crate) fn parse_graph_id(raw: String) -> Result<TaskGraphId, TaskWorldRuntimeError> {
    TaskGraphId::new(raw).map_err(TaskWorldRuntimeError::Graph)
}

pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
