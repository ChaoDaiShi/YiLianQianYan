//! Read-only Task World projection for the frozen shared context contract.
//!
//! The adapter exposes a bounded summary of one [`TaskSupervisor`].  It never
//! exposes the graph or node-state collections to a consumer, and the
//! projection metadata explicitly says that this is structured state only;
//! no workflow or tool execution is implied.

use crate::shared::context::{ContextRequest, TaskProjection, TaskProjectionProvider};
use crate::shared::contracts::{SimulationMetadata, SHARED_SCHEMA_VERSION};
use serde::Serialize;
use serde_json::Value;

use super::{
    ExecutorRef, NodeExecution, NodeExecutionStatus, TaskCheckpointSummary, TaskCommandExecution,
    TaskCommandExecutionStatus, TaskEdge, TaskNode, TaskNodeKind, TaskNodeState, TaskNodeStatus,
    TaskRevisionSummary, TaskSupervisor, ValidationStatus,
};

const PROJECTION_PROVIDER: &str = "task-supervisor-state";
const PROJECTION_REASON: &str =
    "structured task state projection only; no workflow or tool execution";

const MAX_DETAIL_TEXT_CHARS: usize = 500;
const MAX_DETAIL_LIST_ITEMS: usize = 20;

/// Safe, public summary of a TaskGraph. It intentionally contains no raw
/// TaskSupervisor, database row, lock, or arbitrary node input payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskGraphSummary {
    pub id: String,
    pub schema_version: u32,
    pub revision: u64,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskNodeStateSummary {
    pub status: TaskNodeStatus,
    pub attempts: u32,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
    pub command_execution: Option<TaskCommandExecutionSummary>,
}

/// Bounded command correlation exposed to the Task World UI. Provider native
/// identifiers and arbitrary command results remain outside this projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskCommandExecutionSummary {
    pub request_id: String,
    pub command: String,
    pub app_id: String,
    pub attempt: u32,
    pub graph_revision: u64,
    pub status: TaskCommandExecutionStatus,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskResourceReference {
    pub id: String,
    pub name: Option<String>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskValidationSummary {
    pub status: String,
    pub issues: Vec<String>,
}

/// Bounded public evidence for one Task Harness attempt. Raw context and
/// provider output stay inside the execution boundary; only a short result
/// summary and independent validation facts cross into the UI projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskNodeExecutionSummary {
    pub execution_id: String,
    pub attempt: u32,
    pub status: NodeExecutionStatus,
    pub executor_ref: Option<String>,
    pub validation: Option<TaskExecutionValidationSummary>,
    pub approval_ref: Option<String>,
    pub failure_code: Option<String>,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskExecutionValidationSummary {
    pub status: ValidationStatus,
    pub issues: Vec<String>,
    pub checked_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskNodeDetail {
    pub id: String,
    pub kind: TaskNodeKind,
    pub title: String,
    pub status: TaskNodeStatus,
    pub state: TaskNodeStateSummary,
    pub executor_ref: Option<String>,
    pub command_binding: Option<super::CommandBinding>,
    pub instruction_summary: String,
    pub acceptance_criteria: Vec<String>,
    pub resources: Vec<TaskResourceReference>,
    pub validation: TaskValidationSummary,
    pub result_summary: Option<String>,
    pub latest_execution: Option<TaskNodeExecutionSummary>,
    pub execution_history: Vec<TaskNodeExecutionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskGraphDetail {
    pub graph: TaskGraphSummary,
    pub graph_id: String,
    pub revision: u64,
    pub nodes: Vec<TaskNodeDetail>,
    pub edges: Vec<TaskEdge>,
    pub revisions: Vec<TaskRevisionSummary>,
    pub checkpoints: Vec<TaskCheckpointSummary>,
}

impl TaskGraphDetail {
    pub fn from_supervisor(
        supervisor: &TaskSupervisor,
        revisions: Vec<TaskRevisionSummary>,
        checkpoints: Vec<TaskCheckpointSummary>,
    ) -> Self {
        Self::from_supervisor_with_executions(
            supervisor,
            revisions,
            checkpoints,
            &std::collections::HashMap::new(),
        )
    }

    pub fn from_supervisor_with_executions(
        supervisor: &TaskSupervisor,
        revisions: Vec<TaskRevisionSummary>,
        checkpoints: Vec<TaskCheckpointSummary>,
        executions: &std::collections::HashMap<super::TaskNodeId, Vec<NodeExecution>>,
    ) -> Self {
        let graph = supervisor.graph();
        let state_by_id = supervisor
            .node_states()
            .iter()
            .map(|state| (state.node_id.as_str(), state))
            .collect::<std::collections::HashMap<_, _>>();
        let nodes = graph
            .nodes
            .iter()
            .map(|node| {
                node_detail(
                    node,
                    state_by_id.get(node.id.as_str()).copied(),
                    executions.get(&node.id),
                )
            })
            .collect();
        let graph_summary = TaskGraphSummary {
            id: graph.id.to_string(),
            schema_version: graph.schema_version,
            revision: graph.revision.value(),
            node_count: graph.nodes.len(),
            edge_count: graph.edges.len(),
        };
        Self {
            graph_id: graph_summary.id.clone(),
            revision: graph_summary.revision,
            graph: graph_summary,
            nodes,
            edges: graph.edges.clone(),
            revisions,
            checkpoints,
        }
    }
}

fn node_detail(
    node: &TaskNode,
    state: Option<&TaskNodeState>,
    executions: Option<&Vec<NodeExecution>>,
) -> TaskNodeDetail {
    let fallback_state;
    let state = if let Some(state) = state {
        state
    } else {
        fallback_state = TaskNodeState {
            node_id: node.id.clone(),
            status: TaskNodeStatus::Pending,
            attempts: 0,
            output: None,
            error: None,
            started_at: None,
            finished_at: None,
            updated_at: 0,
            command_execution: None,
        };
        &fallback_state
    };
    let mut result_summary = state.output.as_ref().and_then(result_summary);
    let mut state_summary = TaskNodeStateSummary {
        status: state.status,
        attempts: state.attempts,
        result_summary: result_summary.clone(),
        error: state.error.clone(),
        started_at: state.started_at,
        finished_at: state.finished_at,
        updated_at: state.updated_at,
        command_execution: state
            .command_execution
            .as_ref()
            .map(command_execution_summary),
    };
    let input = &node.input;
    let executor_ref = input
        .get("executor_ref")
        .and_then(Value::as_str)
        .and_then(|raw| ExecutorRef::new(raw).ok())
        .map(|reference| reference.to_string());
    let command_binding = node.command_binding().ok().flatten();
    let instruction_summary = input
        .get("instruction")
        .and_then(Value::as_str)
        .or_else(|| input.get("description").and_then(Value::as_str))
        .map_or_else(
            || node.title.clone(),
            |value| truncate(value, MAX_DETAIL_TEXT_CHARS),
        );

    let acceptance_criteria = list_text_values(input.get("acceptance_criteria"));
    let resources = resource_references(input.get("resources"));
    let validation = validation_summary(input.get("validation"));
    let execution_history = executions
        .map(|attempts| {
            attempts
                .iter()
                .rev()
                .take(MAX_DETAIL_LIST_ITEMS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(execution_summary)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let latest_execution = execution_history.last().cloned();
    let projected_status = latest_execution
        .as_ref()
        .map(|execution| match execution.status {
            super::NodeExecutionStatus::Pending | super::NodeExecutionStatus::Ready => {
                TaskNodeStatus::Runnable
            }
            super::NodeExecutionStatus::Dispatching
            | super::NodeExecutionStatus::WaitingApproval
            | super::NodeExecutionStatus::Running
            | super::NodeExecutionStatus::Validating => TaskNodeStatus::Running,
            super::NodeExecutionStatus::Succeeded => TaskNodeStatus::Succeeded,
            super::NodeExecutionStatus::Failed => TaskNodeStatus::Failed,
            super::NodeExecutionStatus::Blocked => TaskNodeStatus::Blocked,
            super::NodeExecutionStatus::Cancelled => TaskNodeStatus::Cancelled,
            super::NodeExecutionStatus::Stale => TaskNodeStatus::Invalidated,
        })
        .unwrap_or(state.status);
    if let Some(execution) = latest_execution.as_ref() {
        state_summary.status = projected_status;
        state_summary.attempts = execution.attempt;
        state_summary.error = execution.error.clone().or_else(|| state.error.clone());
        state_summary.started_at = execution.started_at.or(state.started_at);
        state_summary.finished_at = execution.finished_at.or(state.finished_at);
        state_summary.updated_at = state.updated_at.max(execution.updated_at);
        result_summary = execution.result_summary.clone().or(result_summary);
        state_summary.result_summary = result_summary.clone();
    }
    TaskNodeDetail {
        id: node.id.to_string(),
        kind: node.kind,
        title: node.title.clone(),
        status: projected_status,
        state: state_summary,
        executor_ref,
        command_binding,
        instruction_summary,
        acceptance_criteria,
        resources,
        validation,
        result_summary,
        latest_execution,
        execution_history,
    }
}

pub fn execution_summary(execution: &NodeExecution) -> TaskNodeExecutionSummary {
    TaskNodeExecutionSummary {
        execution_id: execution.id.to_string(),
        attempt: execution.attempt,
        status: execution.status,
        executor_ref: execution
            .executor_ref
            .as_ref()
            .map(|value| truncate(&value.to_string(), 256)),
        validation: execution.validation.as_ref().map(|validation| {
            TaskExecutionValidationSummary {
                status: validation.status,
                issues: validation
                    .issues
                    .iter()
                    .map(|issue| truncate(issue, MAX_DETAIL_TEXT_CHARS))
                    .take(MAX_DETAIL_LIST_ITEMS)
                    .collect(),
                checked_at: validation.checked_at,
            }
        }),
        approval_ref: execution
            .approval_ref
            .as_deref()
            .map(|value| truncate(value, 256)),
        failure_code: execution
            .failure_code
            .as_deref()
            .map(|value| truncate(value, 128)),
        error: execution
            .error
            .as_deref()
            .map(|value| truncate(value, MAX_DETAIL_TEXT_CHARS)),
        result_summary: execution.output.as_ref().and_then(result_summary),
        created_at: execution.created_at,
        updated_at: execution.updated_at,
        started_at: execution.started_at,
        finished_at: execution.finished_at,
    }
}

fn command_execution_summary(execution: &TaskCommandExecution) -> TaskCommandExecutionSummary {
    TaskCommandExecutionSummary {
        request_id: execution.request_id.clone(),
        command: execution.command.clone(),
        app_id: execution.app_id.clone(),
        attempt: execution.attempt,
        graph_revision: execution.graph_revision.value(),
        status: execution.status,
        approval_id: execution.approval_id.clone(),
    }
}

fn result_summary(value: &Value) -> Option<String> {
    let candidate = match value {
        Value::Object(object) => object.get("summary").or_else(|| object.get("result")),
        _ => Some(value),
    }?;
    let text = match candidate {
        Value::String(value) => value.clone(),
        Value::Bool(_) | Value::Number(_) => candidate.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => return None,
    };
    (!text.trim().is_empty()).then(|| truncate(text.trim(), MAX_DETAIL_TEXT_CHARS))
}

fn list_text_values(value: Option<&Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let values = match value {
        Value::String(value) => value
            .lines()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate(value, MAX_DETAIL_TEXT_CHARS))
            .collect(),
        Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                value
                    .as_str()
                    .or_else(|| value.get("text").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| truncate(value, MAX_DETAIL_TEXT_CHARS))
            })
            .collect(),
        _ => Vec::new(),
    };
    values.into_iter().take(MAX_DETAIL_LIST_ITEMS).collect()
}

fn resource_references(value: Option<&Value>) -> Vec<TaskResourceReference> {
    let Some(Value::Array(values)) = value else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|value| match value {
            Value::String(id) if !id.trim().is_empty() => Some(TaskResourceReference {
                id: truncate(id.trim(), 128),
                name: None,
                uri: None,
            }),
            Value::Object(object) => {
                let id = object
                    .get("id")
                    .or_else(|| object.get("resource_id"))
                    .or_else(|| object.get("ref"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())?;
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|value| truncate(value.trim(), 200));
                let uri = object
                    .get("uri")
                    .or_else(|| object.get("url"))
                    .and_then(Value::as_str)
                    .map(|value| truncate(value.trim(), 500));
                Some(TaskResourceReference {
                    id: truncate(id, 128),
                    name,
                    uri,
                })
            }
            _ => None,
        })
        .take(MAX_DETAIL_LIST_ITEMS)
        .collect()
}

fn validation_summary(value: Option<&Value>) -> TaskValidationSummary {
    let Some(Value::Object(object)) = value else {
        return TaskValidationSummary {
            status: "not_checked".to_string(),
            issues: Vec::new(),
        };
    };
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .map(|value| truncate(value.trim(), 64))
        .or_else(|| {
            object.get("valid").and_then(Value::as_bool).map(|valid| {
                if valid {
                    "valid".to_string()
                } else {
                    "invalid".to_string()
                }
            })
        })
        .unwrap_or_else(|| "not_checked".to_string());
    let issues = list_text_values(object.get("issues"));
    TaskValidationSummary { status, issues }
}

fn truncate(value: &str, maximum: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(maximum).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// Read-only consumer of one in-memory Task World supervisor.
///
/// The lifetime-bound reference keeps the projection current without giving
/// this adapter any mutation path.  Only [`TaskProjection`] values cross the
/// shared contract boundary; the graph remains a v1-owned implementation
/// detail.
pub struct TaskProjectionProviderAdapter<'a> {
    supervisor: &'a TaskSupervisor,
}

impl<'a> TaskProjectionProviderAdapter<'a> {
    pub fn new(supervisor: &'a TaskSupervisor) -> Self {
        Self { supervisor }
    }

    fn projection(&self) -> TaskProjection {
        let graph = self.supervisor.graph();
        let states = self.supervisor.node_states();
        let completed = states
            .iter()
            .filter(|state| state.status == TaskNodeStatus::Succeeded)
            .count();
        let progress = (!states.is_empty()).then(|| completed as f32 / states.len() as f32);

        TaskProjection {
            id: graph.id.to_string(),
            title: format!("Task graph {}", graph.id),
            status: projection_status(states).to_string(),
            progress,
            current_activity: current_activity(self.supervisor),
            attention_required: attention_required(self.supervisor),
            updated_at: states
                .iter()
                .map(|state| state.updated_at)
                .max()
                .unwrap_or(0),
            simulation: SimulationMetadata {
                simulated: false,
                provider: PROJECTION_PROVIDER.to_string(),
                reason: PROJECTION_REASON.to_string(),
            },
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }

    pub(crate) fn project(&self) -> TaskProjection {
        self.projection()
    }
}

impl TaskProjectionProvider for TaskProjectionProviderAdapter<'_> {
    fn list(&self, request: &ContextRequest) -> Vec<TaskProjection> {
        if request.max_items == 0 {
            return Vec::new();
        }

        let projection = self.projection();
        if request.query.as_deref().is_some_and(|query| {
            let query = query.to_lowercase();
            !projection.id.to_lowercase().contains(&query)
                && !projection.title.to_lowercase().contains(&query)
        }) {
            return Vec::new();
        }

        // This adapter owns one graph; max_items still bounds the shared
        // provider response and keeps the contract deterministic.
        vec![projection]
            .into_iter()
            .take(request.max_items)
            .collect()
    }
}

fn projection_status(states: &[TaskNodeState]) -> &'static str {
    if !states.is_empty() && states.iter().all(|state| state.status.is_terminal()) {
        if states
            .iter()
            .all(|state| state.status == TaskNodeStatus::Succeeded)
        {
            return "completed";
        }
    }

    if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Failed)
    {
        "failed"
    } else if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Blocked)
    {
        "blocked"
    } else if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Cancelled)
    {
        "cancelled"
    } else if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Running)
    {
        "working"
    } else if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Runnable)
    {
        "runnable"
    } else if states
        .iter()
        .any(|state| state.status == TaskNodeStatus::Invalidated)
    {
        "invalidated"
    } else {
        "pending"
    }
}

fn attention_required(supervisor: &TaskSupervisor) -> bool {
    supervisor.node_states().iter().any(|state| {
        if state.command_execution.as_ref().is_some_and(|execution| {
            matches!(
                execution.status,
                TaskCommandExecutionStatus::Dispatching
                    | TaskCommandExecutionStatus::WaitingApproval
                    | TaskCommandExecutionStatus::Running
            )
        }) {
            return true;
        }
        if matches!(
            state.status,
            TaskNodeStatus::Failed | TaskNodeStatus::Blocked | TaskNodeStatus::Cancelled
        ) {
            return true;
        }

        matches!(
            state.status,
            TaskNodeStatus::Runnable | TaskNodeStatus::Running
        ) && supervisor.graph().node(&state.node_id).is_some_and(|node| {
            matches!(
                node.kind,
                TaskNodeKind::Approval | TaskNodeKind::UserCheckpoint
            )
        })
    })
}

fn current_activity(supervisor: &TaskSupervisor) -> Option<String> {
    let graph = supervisor.graph();
    let states = supervisor.node_states();

    // Prefer an active state, then an explicit user attention point, then the
    // first ready/blocked/stale state in graph definition order.
    first_activity(states, graph, |state, _| {
        state.status == TaskNodeStatus::Running
    })
    .or_else(|| {
        first_activity(states, graph, |state, node| {
            state.status == TaskNodeStatus::Runnable
                && matches!(
                    node.kind,
                    TaskNodeKind::Approval | TaskNodeKind::UserCheckpoint
                )
        })
    })
    .or_else(|| {
        first_activity(states, graph, |state, _| {
            state.status == TaskNodeStatus::Runnable
        })
    })
    .or_else(|| {
        first_activity(states, graph, |state, _| {
            matches!(
                state.status,
                TaskNodeStatus::Failed | TaskNodeStatus::Blocked | TaskNodeStatus::Cancelled
            )
        })
    })
    .or_else(|| {
        first_activity(states, graph, |state, _| {
            matches!(
                state.status,
                TaskNodeStatus::Invalidated | TaskNodeStatus::Pending
            )
        })
    })
}

fn first_activity<F>(
    states: &[TaskNodeState],
    graph: &super::TaskGraph,
    predicate: F,
) -> Option<String>
where
    F: Fn(&TaskNodeState, &super::TaskNode) -> bool,
{
    states.iter().find_map(|state| {
        let node = graph.node(&state.node_id)?;
        predicate(state, node).then(|| activity_label(state, node))
    })
}

fn activity_label(state: &TaskNodeState, node: &super::TaskNode) -> String {
    if let Some(execution) = state.command_execution.as_ref() {
        match execution.status {
            TaskCommandExecutionStatus::Dispatching => {
                return format!("Requesting command: {}", node.title)
            }
            TaskCommandExecutionStatus::WaitingApproval => {
                return format!("Awaiting approval: {}", node.title)
            }
            TaskCommandExecutionStatus::Running => {
                return format!("Executing command: {}", node.title)
            }
            TaskCommandExecutionStatus::Verified
            | TaskCommandExecutionStatus::Failed
            | TaskCommandExecutionStatus::Cancelled => {}
        }
    }
    match state.status {
        TaskNodeStatus::Running => format!("Task state: {}", node.title),
        TaskNodeStatus::Runnable => match node.kind {
            TaskNodeKind::Approval => format!("Awaiting approval: {}", node.title),
            TaskNodeKind::UserCheckpoint => format!("Awaiting user checkpoint: {}", node.title),
            TaskNodeKind::Work => format!("Ready: {}", node.title),
        },
        TaskNodeStatus::Failed => format!("Failed state: {}", node.title),
        TaskNodeStatus::Blocked => format!("Blocked state: {}", node.title),
        TaskNodeStatus::Cancelled => format!("Cancelled state: {}", node.title),
        TaskNodeStatus::Invalidated => format!("Needs refresh: {}", node.title),
        TaskNodeStatus::Pending => format!("Waiting for dependencies: {}", node.title),
        TaskNodeStatus::Succeeded => node.title.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::context::{ContextRequest, TaskProjectionProvider};
    use crate::shared::contracts::SHARED_SCHEMA_VERSION;
    use crate::task::{
        GraphRevision, TaskEdge, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
        TaskSupervisor,
    };
    use serde_json::json;

    fn graph_id(raw: &str) -> TaskGraphId {
        TaskGraphId::new(raw).expect("valid graph id")
    }

    fn node_id(raw: &str) -> TaskNodeId {
        TaskNodeId::new(raw).expect("valid node id")
    }

    fn node(id: &str, kind: TaskNodeKind, title: &str) -> TaskNode {
        TaskNode::new(node_id(id), kind, title, json!({ "node": id })).expect("valid task node")
    }

    fn graph() -> TaskGraph {
        TaskGraph::new(
            graph_id("graph-1"),
            GraphRevision::initial(),
            vec![
                node("running", TaskNodeKind::Work, "Running node"),
                node("approval", TaskNodeKind::Approval, "Approve plan"),
                node("pending", TaskNodeKind::Work, "Follow-up node"),
            ],
            vec![TaskEdge::new(node_id("running"), node_id("pending"))],
        )
        .expect("valid task graph")
    }

    #[test]
    fn projection_maps_state_metadata_and_query_bounds() {
        let mut supervisor = TaskSupervisor::new(graph(), 10).expect("valid supervisor graph");
        supervisor
            .start_node(&node_id("running"), 11)
            .expect("root can enter running state");
        let provider = TaskProjectionProviderAdapter::new(&supervisor);

        let mut request = ContextRequest::new("workspace:test");
        request.max_items = 1;
        let projection = provider.list(&request);

        assert_eq!(projection.len(), 1);
        let projection = &projection[0];
        assert_eq!(projection.id, "graph-1");
        assert_eq!(projection.title, "Task graph graph-1");
        assert_eq!(projection.status, "working");
        assert_eq!(projection.progress, Some(0.0));
        assert_eq!(
            projection.current_activity.as_deref(),
            Some("Task state: Running node")
        );
        assert!(projection.attention_required);
        assert_eq!(projection.updated_at, 11);
        assert_eq!(projection.schema_version, SHARED_SCHEMA_VERSION);
        assert!(!projection.simulation.simulated);
        assert_eq!(projection.simulation.provider, "task-supervisor-state");
        assert_eq!(
            projection.simulation.reason,
            "structured task state projection only; no workflow or tool execution"
        );

        request.query = Some("APPROVE".to_string());
        assert!(provider.list(&request).is_empty());
        request.query = Some("GRAPH-1".to_string());
        assert_eq!(provider.list(&request).len(), 1);
        request.query = Some("not present".to_string());
        assert!(provider.list(&request).is_empty());
        request.query = None;
        request.max_items = 0;
        assert!(provider.list(&request).is_empty());
    }

    #[test]
    fn projection_reflects_completion_without_exposing_graph_structure() {
        let mut supervisor = TaskSupervisor::new(
            TaskGraph::new(
                graph_id("complete-graph"),
                GraphRevision::initial(),
                vec![
                    node("source", TaskNodeKind::Work, "Source"),
                    node("result", TaskNodeKind::Work, "Result"),
                ],
                vec![TaskEdge::new(node_id("source"), node_id("result"))],
            )
            .expect("valid task graph"),
            20,
        )
        .expect("valid supervisor graph");
        supervisor.start_node(&node_id("source"), 21).unwrap();
        supervisor
            .succeed_node(&node_id("source"), json!({ "ok": true }), 22)
            .unwrap();
        supervisor.start_node(&node_id("result"), 23).unwrap();
        supervisor
            .succeed_node(&node_id("result"), json!("done"), 24)
            .unwrap();

        let provider = TaskProjectionProviderAdapter::new(&supervisor);
        let projection = provider.list(&ContextRequest::new("workspace:test"));
        assert_eq!(projection.len(), 1);
        let projection = &projection[0];
        assert_eq!(projection.id, "complete-graph");
        assert_eq!(projection.status, "completed");
        assert_eq!(projection.progress, Some(1.0));
        assert!(projection.current_activity.is_none());
        assert!(!projection.attention_required);
        assert_eq!(projection.updated_at, 24);

        let value = serde_json::to_value(projection).expect("projection serializes");
        assert!(value.get("nodes").is_none());
        assert!(value.get("edges").is_none());
    }

    #[test]
    fn graph_detail_does_not_fallback_to_raw_node_output_or_input() {
        let node = TaskNode::new(
            node_id("safe-node"),
            TaskNodeKind::Work,
            "Safe node",
            json!({
                "executor_ref": "workflow://review",
                "instruction": "Review this task",
                "private_payload": "do not expose"
            }),
        )
        .unwrap();
        let graph = TaskGraph::new(
            graph_id("detail-graph"),
            GraphRevision::initial(),
            vec![node],
            Vec::new(),
        )
        .unwrap();
        let mut supervisor = TaskSupervisor::new(graph, 1).unwrap();
        supervisor.start_node(&node_id("safe-node"), 2).unwrap();
        supervisor
            .succeed_node(
                &node_id("safe-node"),
                json!({"private_payload": "do not expose"}),
                3,
            )
            .unwrap();

        let detail = TaskGraphDetail::from_supervisor(&supervisor, Vec::new(), Vec::new());
        assert_eq!(detail.nodes[0].result_summary, None);
        assert_eq!(
            detail.nodes[0].executor_ref.as_deref(),
            Some("workflow://review")
        );
        let value = serde_json::to_value(detail).unwrap();
        assert!(value["nodes"][0].get("input").is_none());
        assert!(value["nodes"][0].get("private_payload").is_none());
    }

    #[test]
    fn detail_projects_command_correlation_and_attention() {
        let node = TaskNode::new(
            node_id("focus"),
            TaskNodeKind::Work,
            "Focus app",
            json!({
                "executor_ref": "command://desktop.app.focus",
                "command_binding": {
                    "command": "desktop.app.focus",
                    "args": {"app_id": "app:code.exe"}
                }
            }),
        )
        .unwrap();
        let graph = TaskGraph::new(
            graph_id("focus-detail"),
            GraphRevision::initial(),
            vec![node],
            Vec::new(),
        )
        .unwrap();
        let mut supervisor = TaskSupervisor::new(graph, 1).unwrap();
        supervisor.start_node(&node_id("focus"), 2).unwrap();
        supervisor
            .attach_command_execution(
                &node_id("focus"),
                crate::task::TaskCommandExecution {
                    request_id: "request-1".into(),
                    command: "desktop.app.focus".into(),
                    app_id: "app:code.exe".into(),
                    attempt: 1,
                    graph_revision: GraphRevision::initial(),
                    status: crate::task::TaskCommandExecutionStatus::Dispatching,
                    approval_id: None,
                },
            )
            .unwrap();
        supervisor
            .update_command_execution(
                &node_id("focus"),
                "request-1",
                "app:code.exe",
                crate::task::TaskCommandExecutionStatus::WaitingApproval,
                Some("approval-1".into()),
                3,
            )
            .unwrap();

        let detail = TaskGraphDetail::from_supervisor(&supervisor, Vec::new(), Vec::new());
        assert_eq!(
            detail.nodes[0]
                .command_binding
                .as_ref()
                .map(|binding| binding.args.app_id.as_str()),
            Some("app:code.exe")
        );
        let state = &detail.nodes[0].state;
        assert_eq!(
            state.command_execution.as_ref().unwrap().request_id,
            "request-1"
        );
        assert_eq!(
            state.command_execution.as_ref().unwrap().app_id,
            "app:code.exe"
        );
        assert_eq!(
            TaskProjectionProviderAdapter::new(&supervisor)
                .list(&ContextRequest::new("task-world"))[0]
                .attention_required,
            true
        );
    }
}
