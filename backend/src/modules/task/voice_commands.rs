//! Stable Task command/query adapters used by Voice and Text interactions.
//!
//! The service delegates to [`TaskWorldRuntime`] and never reaches into a
//! supervisor or executor.  It owns only the bounded command payload mapping
//! and the user-facing status projection.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::shared::command::{CommandError, CommandRouter};

use super::{TaskGraphId, TaskNodeId, TaskNodeStatus, TaskWorldRuntime, TaskWorldRuntimeError};

const MAX_CONTROL_GENERATION: u64 = i64::MAX as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionControlState {
    Running,
    Paused,
    Cancelled,
}

impl Default for TaskExecutionControlState {
    fn default() -> Self {
        Self::Running
    }
}

impl TaskExecutionControlState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Persisted graph-level execution control.  It is orthogonal to the Task
/// Graph and node execution rows, so pausing never duplicates node history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionControl {
    pub graph_id: TaskGraphId,
    pub state: TaskExecutionControlState,
    pub generation: u64,
    #[serde(default)]
    pub paused_nodes: Vec<TaskNodeId>,
    pub updated_at: i64,
}

impl TaskExecutionControl {
    pub(crate) fn initial(graph_id: TaskGraphId, updated_at: i64) -> Self {
        Self {
            graph_id,
            state: TaskExecutionControlState::Running,
            generation: 0,
            paused_nodes: Vec::new(),
            updated_at,
        }
    }

    pub(crate) fn next_generation(&self) -> Result<u64, TaskWorldRuntimeError> {
        self.generation
            .checked_add(1)
            .filter(|generation| *generation <= MAX_CONTROL_GENERATION)
            .ok_or(TaskWorldRuntimeError::ExecutionControlGenerationOverflow)
    }
}

/// Stable, bounded status information for narration and Voice/Text clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusProjection {
    pub graph_id: String,
    pub title: String,
    pub overall_status: String,
    pub current_activity: Option<String>,
    pub completed_count: usize,
    pub running_count: usize,
    pub waiting_count: usize,
    pub failed_count: usize,
    pub attention_required: bool,
    pub current_node_summary: Option<String>,
    pub control_state: TaskExecutionControlState,
    pub generation: u64,
    pub updated_at: i64,
}

/// Minimal projection for `task.current`. Unlike `task.status`, this
/// response identifies the current Task and node without duplicating the full
/// count-oriented status payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCurrentProjection {
    pub task_id: String,
    pub task_title: String,
    pub current_node_id: Option<String>,
    pub current_node_summary: Option<String>,
    pub overall_status: String,
    pub control_state: TaskExecutionControlState,
    pub generation: u64,
    pub updated_at: i64,
}

#[derive(Debug, Error)]
pub enum TaskCommandError {
    #[error("task command target is invalid: {0}")]
    InvalidTarget(String),
    #[error("task command failed: {0}")]
    Runtime(#[from] TaskWorldRuntimeError),
}

impl TaskCommandError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidTarget(_) => "invalid_target",
            Self::Runtime(TaskWorldRuntimeError::GraphNotFound(_)) => "task_not_found",
            Self::Runtime(TaskWorldRuntimeError::ExecutionPaused(_)) => "task_paused",
            Self::Runtime(TaskWorldRuntimeError::ExecutionCancelled(_)) => "task_cancelled",
            Self::Runtime(_) => "task_command_error",
        }
    }
}

/// Adapter from stable Task commands to the authoritative Task World runtime.
#[derive(Clone)]
pub struct TaskCommandService {
    runtime: TaskWorldRuntime,
}

/// v1 name used by Voice/Text integrations; the implementation remains the
/// single TaskCommandService adapter and does not create another runtime.
pub type TaskVoiceAdapter = TaskCommandService;

impl From<&TaskWorldRuntime> for TaskWorldRuntime {
    fn from(runtime: &TaskWorldRuntime) -> Self {
        runtime.clone()
    }
}

impl TaskCommandService {
    pub fn new(runtime: impl Into<TaskWorldRuntime>) -> Self {
        Self {
            runtime: runtime.into(),
        }
    }

    pub fn runtime(&self) -> TaskWorldRuntime {
        self.runtime.clone()
    }

    pub fn pause(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        self.runtime.pause_task(graph_id, now)?;
        self.status(graph_id, now)
    }

    pub fn resume(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        self.runtime.resume_task(graph_id, now)?;
        self.status(graph_id, now)
    }

    pub fn cancel(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        self.runtime.cancel_task(graph_id, now)?;
        self.status(graph_id, now)
    }

    pub fn retry(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        self.runtime.retry_node(graph_id, node_id, now)?;
        self.status(graph_id, now)
    }

    pub fn rerun(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        let revision = self
            .runtime
            .get_graph(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?
            .revision
            .value();
        self.runtime
            .rerun_from_node(graph_id, node_id, revision, now)?;
        self.status(graph_id, now)
    }

    pub fn status(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<TaskStatusProjection, TaskCommandError> {
        self.runtime
            .status_projection(graph_id, now)
            .map_err(Into::into)
    }

    pub fn current(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<TaskCurrentProjection, TaskCommandError> {
        let detail = self
            .runtime
            .get_graph_detail(graph_id)
            .map_err(TaskCommandError::from)?;
        let control = self
            .runtime
            .execution_control(graph_id)
            .map_err(TaskCommandError::from)?;
        let status = status_from_detail(&detail, &control, now);
        let current = current_node(&detail, &control);
        Ok(TaskCurrentProjection {
            task_id: status.graph_id,
            task_title: status.title,
            current_node_id: current.map(|node| node.id.clone()),
            current_node_summary: current.map(|node| node.title.clone()),
            overall_status: status.overall_status,
            control_state: status.control_state,
            generation: status.generation,
            updated_at: status.updated_at,
        })
    }

    /// Register the stable command surface on a shared CommandRouter.
    pub fn register(&self, router: &CommandRouter) -> Result<(), String> {
        register_task_commands(router, self.clone())
    }
}

/// Register `task.pause/resume/cancel/retry/rerun/status/current` handlers.
pub fn register_task_commands(
    router: &CommandRouter,
    service: TaskCommandService,
) -> Result<(), String> {
    for command in [
        "task.pause",
        "task.resume",
        "task.cancel",
        "task.retry",
        "task.rerun",
        "task.status",
        "task.current",
    ] {
        let service = service.clone();
        router.register(command, move |request| {
            execute_registered_command(&service, command, &request.payload)
        })?;
    }
    Ok(())
}

fn execute_registered_command(
    service: &TaskCommandService,
    command: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let graph_id = payload
        .get("graph_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CommandError::new("invalid_payload", "graph_id is required"))
        .and_then(|raw| {
            TaskGraphId::new(raw.to_string())
                .map_err(|error| CommandError::new("invalid_payload", error.to_string()))
        })?;
    let now = chrono::Utc::now().timestamp_millis();
    let result = match command {
        "task.pause" => service.pause(&graph_id, now),
        "task.resume" => service.resume(&graph_id, now),
        "task.cancel" => service.cancel(&graph_id, now),
        "task.status" => service.status(&graph_id, now),
        "task.current" => {
            return service
                .current(&graph_id, now)
                .map(|projection| {
                    serde_json::to_value(projection).expect("current projection serializes")
                })
                .map_err(|error| CommandError::new(error.code(), error.to_string()));
        }
        "task.retry" => {
            let node_id = node_id_from_payload(payload)?;
            service.retry(&graph_id, &node_id, now)
        }
        "task.rerun" => {
            let node_id = node_id_from_payload(payload)?;
            service.rerun(&graph_id, &node_id, now)
        }
        _ => {
            return Err(CommandError::new(
                "command_not_found",
                "unknown task command",
            ))
        }
    };
    result
        .map(|projection| serde_json::to_value(projection).expect("status projection serializes"))
        .map_err(|error| CommandError::new(error.code(), error.to_string()))
}

fn node_id_from_payload(payload: &serde_json::Value) -> Result<TaskNodeId, CommandError> {
    let raw = payload
        .get("node_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CommandError::new("invalid_payload", "node_id is required"))?;
    TaskNodeId::new(raw.to_string())
        .map_err(|error| CommandError::new("invalid_payload", error.to_string()))
}

pub(crate) fn status_from_detail(
    detail: &super::TaskGraphDetail,
    control: &TaskExecutionControl,
    now: i64,
) -> TaskStatusProjection {
    let completed_count = detail
        .nodes
        .iter()
        .filter(|node| node.status == TaskNodeStatus::Succeeded)
        .count();
    let running_count = detail
        .nodes
        .iter()
        .filter(|node| node.status == TaskNodeStatus::Running)
        .count();
    let waiting_count = detail
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.status,
                TaskNodeStatus::Pending | TaskNodeStatus::Runnable
            ) || node
                .state
                .command_execution
                .as_ref()
                .is_some_and(|execution| {
                    matches!(
                        execution.status,
                        super::TaskCommandExecutionStatus::WaitingApproval
                    )
                })
        })
        .count();
    let failed_count = detail
        .nodes
        .iter()
        .filter(|node| node.status == TaskNodeStatus::Failed)
        .count();

    let current = current_node(detail, control);
    let current_activity = current.map(|node| match node.status {
        TaskNodeStatus::Running => format!("正在处理：{}", node.title),
        TaskNodeStatus::Runnable => format!("待处理：{}", node.title),
        TaskNodeStatus::Pending => format!("等待前置步骤：{}", node.title),
        _ => node.title.clone(),
    });
    let current_node_summary = current.map(|node| node.title.clone());

    let overall_status = match control.state {
        TaskExecutionControlState::Paused => "paused",
        TaskExecutionControlState::Cancelled => "cancelled",
        TaskExecutionControlState::Running
            if !detail.nodes.is_empty() && completed_count == detail.nodes.len() =>
        {
            "completed"
        }
        TaskExecutionControlState::Running if failed_count > 0 => "failed",
        TaskExecutionControlState::Running if running_count > 0 => "working",
        TaskExecutionControlState::Running if waiting_count > 0 => "runnable",
        TaskExecutionControlState::Running => "pending",
    }
    .to_string();
    let attention_required = control.state != TaskExecutionControlState::Running
        || failed_count > 0
        || waiting_count > 0;
    let updated_at = detail
        .nodes
        .iter()
        .map(|node| node.state.updated_at)
        .max()
        .unwrap_or(now)
        .max(control.updated_at);

    TaskStatusProjection {
        graph_id: detail.graph_id.clone(),
        title: format!("Task graph {}", detail.graph_id),
        overall_status,
        current_activity,
        completed_count,
        running_count,
        waiting_count,
        failed_count,
        attention_required,
        current_node_summary,
        control_state: control.state,
        generation: control.generation,
        updated_at,
    }
}

fn current_node<'a>(
    detail: &'a super::TaskGraphDetail,
    control: &TaskExecutionControl,
) -> Option<&'a super::TaskNodeDetail> {
    detail
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.status,
                TaskNodeStatus::Running | TaskNodeStatus::Runnable | TaskNodeStatus::Pending
            ) || control
                .paused_nodes
                .iter()
                .any(|paused| paused.as_str() == node.id)
        })
        .min_by_key(|node| match node.status {
            TaskNodeStatus::Running => 0,
            TaskNodeStatus::Runnable | TaskNodeStatus::Pending => 1,
            _ => 2,
        })
}
