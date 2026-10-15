//! MCP Tasks extension wire helpers pinned to the 2026-07-28 extension schema.
//!
//! Transport selection and task polling belong to `McpRuntimeManager`; this
//! module only builds request parameters and preserves polymorphic results.

use serde_json::{json, Map, Value};

pub const TASKS_EXTENSION_ID: &str = "io.modelcontextprotocol/tasks";
pub const TASKS_SPEC_VERSION: &str = "2026-07-28";
pub const TASKS_SPEC_URL: &str =
    "https://modelcontextprotocol.github.io/ext-tasks/specification/2026-07-28/tasks.html";

#[derive(Debug, Clone, PartialEq)]
pub enum McpTaskAugmentedResult {
    /// A normal tool result, including old servers that omit `resultType`.
    Complete(Value),
    /// Existing MCP multi-round-trip input request shape.
    InputRequired(Value),
    /// A durable task handle returned by a task-augmented request.
    Task(McpTaskHandle),
    /// A future result type that this version does not interpret.
    Unknown { result_type: String, raw: Value },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTaskStatus {
    Working,
    InputRequired,
    Completed,
    Cancelled,
    Failed,
    Unknown(String),
}

impl McpTaskStatus {
    fn parse(value: &str) -> Self {
        match value {
            "working" => Self::Working,
            "input_required" => Self::InputRequired,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            value => Self::Unknown(value.to_string()),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTaskHandle {
    pub task_id: String,
    pub status: McpTaskStatus,
    pub poll_interval_ms: Option<u64>,
}

impl McpTaskHandle {
    fn from_create_result(value: &Value) -> Option<Self> {
        Some(Self {
            task_id: value.get("taskId")?.as_str()?.to_string(),
            status: McpTaskStatus::parse(value.get("status")?.as_str()?),
            poll_interval_ms: value.get("pollIntervalMs").and_then(Value::as_u64),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpTaskSnapshot {
    pub task_id: String,
    pub status: McpTaskStatus,
    pub status_message: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub created_at: Option<String>,
    pub last_updated_at: Option<String>,
    pub ttl_ms: Option<u64>,
    pub input_requests: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpTaskExtensionError {
    #[error("MCP task result is missing a taskId")]
    MissingTaskId,
    #[error("MCP task result is missing a status")]
    MissingStatus,
    #[error("MCP task id must not be empty")]
    EmptyTaskId,
    #[error("MCP task status is empty")]
    EmptyStatus,
}

/// Parse a tools/call result without changing the legacy ordinary-result path.
pub fn parse_tool_call_result(value: Value) -> McpTaskAugmentedResult {
    match value.get("resultType").and_then(Value::as_str) {
        None | Some("complete") => McpTaskAugmentedResult::Complete(value),
        Some("input_required") => McpTaskAugmentedResult::InputRequired(value),
        Some("task") => match McpTaskHandle::from_create_result(&value) {
            Some(task) if !task.task_id.trim().is_empty() => McpTaskAugmentedResult::Task(task),
            _ => McpTaskAugmentedResult::Unknown {
                result_type: "task".to_string(),
                raw: value,
            },
        },
        Some(result_type) => McpTaskAugmentedResult::Unknown {
            result_type: result_type.to_string(),
            raw: value,
        },
    }
}

/// Parse a `tasks/get` result. The returned value keeps status-specific fields
/// separate so a working handle is never mistaken for a final artifact.
pub fn parse_task_snapshot(value: &Value) -> Result<McpTaskSnapshot, McpTaskExtensionError> {
    let task_id = value
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or(McpTaskExtensionError::MissingTaskId)?;
    if task_id.trim().is_empty() {
        return Err(McpTaskExtensionError::EmptyTaskId);
    }
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or(McpTaskExtensionError::MissingStatus)?;
    if status.trim().is_empty() {
        return Err(McpTaskExtensionError::EmptyStatus);
    }
    Ok(McpTaskSnapshot {
        task_id: task_id.to_string(),
        status: McpTaskStatus::parse(status),
        status_message: value
            .get("statusMessage")
            .and_then(Value::as_str)
            .map(str::to_string),
        poll_interval_ms: value.get("pollIntervalMs").and_then(Value::as_u64),
        created_at: value
            .get("createdAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        last_updated_at: value
            .get("lastUpdatedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        ttl_ms: value.get("ttlMs").and_then(Value::as_u64),
        input_requests: value.get("inputRequests").cloned(),
        result: value.get("result").cloned(),
        error: value.get("error").cloned(),
    })
}

/// Per-request capability shape required by the pinned Tasks extension.
pub fn tasks_client_capability() -> Value {
    let mut extensions = Map::new();
    extensions.insert(TASKS_EXTENSION_ID.to_string(), json!({}));
    json!({"extensions": extensions})
}

/// Merge the Tasks extension capability into `params._meta` while retaining
/// unrelated metadata already attached by a transport.
pub fn attach_tasks_capability(params: &mut Value) {
    if !params.is_object() {
        *params = Value::Object(Map::new());
    }
    let object = params.as_object_mut().expect("params was set to an object");
    if !object.get("_meta").is_some_and(Value::is_object) {
        object.insert("_meta".to_string(), Value::Object(Map::new()));
    }
    let meta = object
        .get_mut("_meta")
        .and_then(Value::as_object_mut)
        .expect("_meta was set to an object");
    if !meta
        .get("io.modelcontextprotocol/clientCapabilities")
        .is_some_and(Value::is_object)
    {
        meta.insert(
            "io.modelcontextprotocol/clientCapabilities".to_string(),
            Value::Object(Map::new()),
        );
    }
    let capabilities = meta
        .get_mut("io.modelcontextprotocol/clientCapabilities")
        .and_then(Value::as_object_mut)
        .expect("clientCapabilities was set to an object");
    if !capabilities.get("extensions").is_some_and(Value::is_object) {
        capabilities.insert("extensions".to_string(), Value::Object(Map::new()));
    }
    capabilities
        .get_mut("extensions")
        .and_then(Value::as_object_mut)
        .expect("extensions was set to an object")
        .insert(TASKS_EXTENSION_ID.to_string(), json!({}));
}

pub fn tasks_get_params(task_id: &str) -> Value {
    task_params(task_id, Map::new())
}

pub fn tasks_update_params(task_id: &str, input_responses: Value) -> Value {
    let mut extra = Map::new();
    extra.insert("inputResponses".to_string(), input_responses);
    task_params(task_id, extra)
}

pub fn tasks_cancel_params(task_id: &str) -> Value {
    task_params(task_id, Map::new())
}

fn task_params(task_id: &str, extra: Map<String, Value>) -> Value {
    let mut params = Map::new();
    params.insert("taskId".to_string(), Value::String(task_id.to_string()));
    params.extend(extra);
    let mut params = Value::Object(params);
    attach_tasks_capability(&mut params);
    params
}

#[cfg(test)]
#[path = "task_extension_tests.rs"]
mod tests;
