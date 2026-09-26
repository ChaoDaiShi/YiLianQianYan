//! Minimal v1 Task World supervisor.
//!
//! `TaskSupervisor` owns the runtime state for a validated [`TaskGraph`].  It
//! resolves only product-level dependencies and lifecycle facts; it never
//! invokes a tool, a workflow, a command router, or a security gateway.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::executor_ref::DESKTOP_APP_FOCUS_COMMAND;
use super::task_graph::{
    GraphRevision, TaskEdge, TaskGraph, TaskGraphId, TaskGraphValidationError, TaskNode, TaskNodeId,
};

pub const MAX_TASK_NODE_ERROR_CHARS: usize = 2_000;

/// Runtime status of one product task node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskNodeStatus {
    Pending,
    Runnable,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    /// The node's previous result no longer describes the current input or
    /// upstream graph.  It can be started again once its dependencies are
    /// succeeded.
    Invalidated,
}

impl TaskNodeStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled
        )
    }
}

impl std::fmt::Display for TaskNodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Pending => "pending",
            Self::Runnable => "runnable",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::Invalidated => "invalidated",
        };
        f.write_str(value)
    }
}

/// Runtime phase for a command requested by a TaskNode.  This is deliberately
/// an independent projection; the product TaskNodeStatus remains responsible
/// for dependency scheduling while the command router owns execution details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCommandExecutionStatus {
    Dispatching,
    WaitingApproval,
    Running,
    Verified,
    Failed,
    Cancelled,
}

impl TaskCommandExecutionStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Dispatching | Self::WaitingApproval | Self::Running
        )
    }
}

/// Safe correlation state for one command attempt.  It stores the stable
/// request and target identity needed to reject stale results, without
/// persisting provider-owned native identifiers or arbitrary command output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCommandExecution {
    pub request_id: String,
    pub command: String,
    pub app_id: String,
    pub attempt: u32,
    pub graph_revision: GraphRevision,
    pub status: TaskCommandExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
}

/// Runtime state kept for a node.  Output is structured and owned by the
/// caller; this module never interprets it as a command or executes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNodeState {
    pub node_id: TaskNodeId,
    pub status: TaskNodeStatus,
    pub attempts: u32,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
    /// Independent command correlation/phase projection. Older snapshots do
    /// not have this field and deserialize with no active command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution: Option<TaskCommandExecution>,
}

impl TaskNodeState {
    fn new(node_id: TaskNodeId, now: i64) -> Self {
        Self {
            node_id,
            status: TaskNodeStatus::Pending,
            attempts: 0,
            output: None,
            error: None,
            started_at: None,
            finished_at: None,
            updated_at: now,
            command_execution: None,
        }
    }
}

/// An opaque identifier for a pure in-memory checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskCheckpointId(String);

impl TaskCheckpointId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskCheckpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Clone-based snapshot of a graph revision and all runtime node state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub id: TaskCheckpointId,
    pub graph_id: TaskGraphId,
    pub graph_revision: GraphRevision,
    pub graph: TaskGraph,
    pub node_states: Vec<TaskNodeState>,
    pub created_at: i64,
}

/// Safe error surface for supervisor operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskSupervisorError {
    #[error("unknown task node: {0}")]
    UnknownNode(String),
    #[error("unknown task edge: {from} -> {to}")]
    UnknownEdge { from: TaskNodeId, to: TaskNodeId },
    #[error("invalid task node transition for {node_id}: {from} -> {to}")]
    InvalidTransition {
        node_id: TaskNodeId,
        from: TaskNodeStatus,
        to: TaskNodeStatus,
    },
    #[error("task node is not ready because dependencies have not succeeded: {0}")]
    DependencyNotReady(TaskNodeId),
    #[error(
        "active command execution for task node {node_id} must be cancelled first: {request_id}"
    )]
    ActiveCommandExecution {
        node_id: TaskNodeId,
        request_id: String,
    },
    #[error("invalid task command execution: {0}")]
    InvalidCommandExecution(String),
    #[error("task command execution requires an independently verified result: {0}")]
    CommandVerificationRequired(TaskNodeId),
    #[error("checkpoint belongs to another graph: expected {expected}, got {actual}")]
    CheckpointGraphMismatch {
        expected: TaskGraphId,
        actual: TaskGraphId,
    },
    #[error("invalid task checkpoint: {0}")]
    InvalidCheckpoint(String),
    #[error("task graph error: {0}")]
    Graph(#[from] TaskGraphValidationError),
}

/// Deterministic dependency supervisor for a single TaskGraph revision.
#[derive(Debug, Clone)]
pub struct TaskSupervisor {
    graph: TaskGraph,
    node_states: Vec<TaskNodeState>,
}

impl TaskSupervisor {
    /// Construct a supervisor only from a validated graph.  Validation is
    /// repeated here because the graph fields are public and serde can bypass
    /// `TaskGraph::new`.  Roots are made runnable immediately.
    pub fn new(graph: TaskGraph, now: i64) -> Result<Self, TaskSupervisorError> {
        graph.validate()?;
        let node_states = graph
            .nodes
            .iter()
            .map(|node| TaskNodeState::new(node.id.clone(), now))
            .collect();
        let mut supervisor = Self { graph, node_states };
        supervisor.refresh_runnable(now);
        Ok(supervisor)
    }

    pub fn graph(&self) -> &TaskGraph {
        &self.graph
    }

    pub fn graph_revision(&self) -> GraphRevision {
        self.graph.revision
    }

    pub fn node_states(&self) -> &[TaskNodeState] {
        &self.node_states
    }

    pub fn node_state(&self, node_id: &TaskNodeId) -> Option<&TaskNodeState> {
        self.node_states
            .iter()
            .find(|state| &state.node_id == node_id)
    }

    /// Return the first command that is still awaiting router/provider
    /// progress.  Callers use this as a graph-edit guard and never as
    /// execution authority.
    pub fn active_command_execution(&self) -> Option<&TaskCommandExecution> {
        self.node_states
            .iter()
            .filter_map(|state| state.command_execution.as_ref())
            .find(|execution| execution.status.is_active())
    }

    /// Attach a server-generated request to a running node attempt.  The
    /// graph revision and attempt are retained so a later result cannot be
    /// applied to an edited or retried node.
    pub fn attach_command_execution(
        &mut self,
        node_id: &TaskNodeId,
        execution: TaskCommandExecution,
    ) -> Result<(), TaskSupervisorError> {
        let current_graph_revision = self.graph.revision;
        let binding = self
            .graph
            .node(node_id)
            .ok_or_else(|| TaskSupervisorError::UnknownNode(node_id.to_string()))?
            .command_binding()?;
        let Some(binding) = binding else {
            return Err(TaskSupervisorError::InvalidCommandExecution(format!(
                "node {node_id} has no supported command binding"
            )));
        };
        if binding.command != execution.command || binding.args.app_id != execution.app_id {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command execution does not match the node binding".to_string(),
            ));
        }
        let state = self.require_state_mut(node_id)?;
        if state.status != TaskNodeStatus::Running {
            return Err(TaskSupervisorError::InvalidCommandExecution(format!(
                "node {node_id} must be running before a command request is attached"
            )));
        }
        if state.command_execution.is_some() {
            let request_id = state
                .command_execution
                .as_ref()
                .map(|current| current.request_id.clone())
                .unwrap_or_default();
            return Err(TaskSupervisorError::ActiveCommandExecution {
                node_id: node_id.clone(),
                request_id,
            });
        }
        if execution.request_id.trim().is_empty() || execution.request_id.len() > 128 {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "request_id must be 1-128 characters".to_string(),
            ));
        }
        if execution.command != DESKTOP_APP_FOCUS_COMMAND {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "only desktop.app.focus can be attached by the v1 Task World".to_string(),
            ));
        }
        if execution.app_id.is_empty()
            || execution.app_id.chars().count() > 256
            || execution
                .app_id
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "app_id must be a non-empty string of at most 256 characters".to_string(),
            ));
        }
        if execution.attempt != state.attempts {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command attempt does not match node attempt".to_string(),
            ));
        }
        if execution.graph_revision != current_graph_revision {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command graph revision does not match current graph".to_string(),
            ));
        }
        if execution.status != TaskCommandExecutionStatus::Dispatching {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "new command execution must begin in dispatching status".to_string(),
            ));
        }
        state.command_execution = Some(execution);
        Ok(())
    }

    /// Update a non-terminal command phase after validating the original
    /// request and target correlation.
    pub fn update_command_execution(
        &mut self,
        node_id: &TaskNodeId,
        request_id: &str,
        app_id: &str,
        status: TaskCommandExecutionStatus,
        approval_id: Option<String>,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        if matches!(
            status,
            TaskCommandExecutionStatus::Dispatching | TaskCommandExecutionStatus::Verified
        ) {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command phase cannot be set through the non-terminal updater".to_string(),
            ));
        }
        let execution = self.require_command_execution_mut(node_id, request_id, app_id)?;
        if !execution.status.is_active() {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command execution is already terminal".to_string(),
            ));
        }
        if !command_phase_transition_allowed(execution.status, status) {
            return Err(TaskSupervisorError::InvalidCommandExecution(format!(
                "command phase cannot move from {:?} to {:?}",
                execution.status, status
            )));
        }
        execution.status = status;
        execution.approval_id = approval_id;
        // The caller transitions the product node to failed/cancelled
        // immediately after this projection update, under one candidate
        // snapshot. Keeping the projection makes the terminal reason and
        // request correlation visible to the UI.
        self.node_state_mut(node_id)
            .expect("command state exists after correlation check")
            .updated_at = now;
        Ok(())
    }

    /// Mark a correlated command verified after the runtime has independently
    /// checked the provider verification object.  Product completion still
    /// goes through [`Self::succeed_node`], which refuses unverified commands.
    pub fn mark_command_verified(
        &mut self,
        node_id: &TaskNodeId,
        request_id: &str,
        app_id: &str,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        let execution = self.require_command_execution_mut(node_id, request_id, app_id)?;
        if !execution.status.is_active() {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command execution is already terminal".to_string(),
            ));
        }
        execution.status = TaskCommandExecutionStatus::Verified;
        self.node_state_mut(node_id)
            .expect("command state exists after correlation check")
            .updated_at = now;
        Ok(())
    }

    /// Convert any persisted in-flight command to a truthful failure during
    /// service startup. The v1 process cannot prove that a v2 request still
    /// exists after restart, so it must never leave a node permanently active.
    pub fn fail_active_command_executions_on_restart(&mut self, now: i64) -> Vec<String> {
        let mut request_ids = Vec::new();
        for state in &mut self.node_states {
            let Some(execution) = state.command_execution.as_mut() else {
                continue;
            };
            if !execution.status.is_active() {
                continue;
            }
            request_ids.push(execution.request_id.clone());
            execution.status = TaskCommandExecutionStatus::Failed;
            state.status = TaskNodeStatus::Failed;
            state.output = None;
            state.error = Some(bound_text(
                "command request interrupted by service restart; provider state unavailable",
                MAX_TASK_NODE_ERROR_CHARS,
            ));
            state.finished_at = Some(now);
            state.updated_at = now;
        }
        if !request_ids.is_empty() {
            self.refresh_runnable(now);
        }
        request_ids
    }

    /// Return runnable nodes in graph definition order.  An invalidated node
    /// is considered runnable only after all its dependencies are succeeded;
    /// its status remains `Invalidated` until `start_node` is called so stale
    /// results remain visible to callers.
    pub fn runnable_nodes(&self) -> Vec<TaskNodeId> {
        self.node_states
            .iter()
            .filter(|state| match state.status {
                TaskNodeStatus::Runnable => true,
                TaskNodeStatus::Invalidated => self.dependencies_succeeded(&state.node_id),
                _ => false,
            })
            .map(|state| state.node_id.clone())
            .collect()
    }

    /// Re-evaluate pending nodes until all dependency consequences have
    /// propagated.  A failed, blocked, or cancelled predecessor blocks its
    /// descendants; only nodes whose complete dependency set succeeded become
    /// runnable.
    pub fn refresh_runnable(&mut self, now: i64) -> Vec<TaskNodeId> {
        loop {
            let status_by_id: HashMap<TaskNodeId, TaskNodeStatus> = self
                .node_states
                .iter()
                .map(|state| (state.node_id.clone(), state.status))
                .collect();
            let mut changed = false;

            for state in &mut self.node_states {
                let dependencies = self.graph.dependencies(&state.node_id);
                let dependency_failed = dependencies.iter().any(|dependency| {
                    matches!(
                        status_by_id.get(dependency),
                        Some(
                            TaskNodeStatus::Failed
                                | TaskNodeStatus::Blocked
                                | TaskNodeStatus::Cancelled
                        )
                    )
                });
                let dependencies_succeeded = dependencies.iter().all(|dependency| {
                    status_by_id.get(dependency) == Some(&TaskNodeStatus::Succeeded)
                });

                let next_status = match state.status {
                    TaskNodeStatus::Pending if dependency_failed => Some(TaskNodeStatus::Blocked),
                    TaskNodeStatus::Pending if dependencies_succeeded => {
                        Some(TaskNodeStatus::Runnable)
                    }
                    _ => None,
                };
                if let Some(next_status) = next_status {
                    state.status = next_status;
                    state.updated_at = now;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        self.runnable_nodes()
    }

    pub fn start_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        if self.node_state(node_id).is_none() {
            return Err(TaskSupervisorError::UnknownNode(node_id.to_string()));
        }
        if !self.dependencies_succeeded(node_id) {
            return Err(TaskSupervisorError::DependencyNotReady(node_id.clone()));
        }

        let state = self
            .node_state_mut(node_id)
            .expect("node existence checked immediately above");
        let from = state.status;
        if !matches!(from, TaskNodeStatus::Runnable | TaskNodeStatus::Invalidated) {
            return Err(TaskSupervisorError::InvalidTransition {
                node_id: node_id.clone(),
                from,
                to: TaskNodeStatus::Running,
            });
        }
        state.status = TaskNodeStatus::Running;
        state.attempts = state.attempts.saturating_add(1);
        state.started_at = Some(now);
        state.finished_at = None;
        state.updated_at = now;
        Ok(())
    }

    pub fn succeed_node(
        &mut self,
        node_id: &TaskNodeId,
        output: serde_json::Value,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        let focus_binding = self
            .graph
            .node(node_id)
            .ok_or_else(|| TaskSupervisorError::UnknownNode(node_id.to_string()))?
            .command_binding()?
            .filter(|binding| binding.command == DESKTOP_APP_FOCUS_COMMAND);
        let graph_revision = self.graph.revision;
        let state = self.require_state_mut(node_id)?;
        let from = state.status;
        if from != TaskNodeStatus::Running {
            return Err(TaskSupervisorError::InvalidTransition {
                node_id: node_id.clone(),
                from,
                to: TaskNodeStatus::Succeeded,
            });
        }
        if let Some(binding) = focus_binding {
            let verified = state.command_execution.as_ref().is_some_and(|execution| {
                execution.command == binding.command
                    && execution.app_id == binding.args.app_id
                    && execution.attempt == state.attempts
                    && execution.graph_revision == graph_revision
                    && execution.status == TaskCommandExecutionStatus::Verified
            });
            if !verified {
                return Err(TaskSupervisorError::CommandVerificationRequired(
                    node_id.clone(),
                ));
            }
        }
        state.status = TaskNodeStatus::Succeeded;
        state.output = Some(output);
        state.error = None;
        state.finished_at = Some(now);
        state.updated_at = now;
        self.refresh_runnable(now);
        Ok(())
    }

    pub fn fail_node(
        &mut self,
        node_id: &TaskNodeId,
        error: impl Into<String>,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        if let Some(execution) = self.node_state(node_id).and_then(|state| {
            state
                .command_execution
                .as_ref()
                .filter(|execution| execution.status.is_active())
        }) {
            return Err(TaskSupervisorError::ActiveCommandExecution {
                node_id: node_id.clone(),
                request_id: execution.request_id.clone(),
            });
        }
        let state = self.require_state_mut(node_id)?;
        let from = state.status;
        if from != TaskNodeStatus::Running {
            return Err(TaskSupervisorError::InvalidTransition {
                node_id: node_id.clone(),
                from,
                to: TaskNodeStatus::Failed,
            });
        }
        state.status = TaskNodeStatus::Failed;
        state.output = None;
        state.error = Some(bound_text(&error.into(), MAX_TASK_NODE_ERROR_CHARS));
        state.finished_at = Some(now);
        state.updated_at = now;
        self.refresh_runnable(now);
        Ok(())
    }

    pub fn cancel_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<(), TaskSupervisorError> {
        if let Some(execution) = self.node_state(node_id).and_then(|state| {
            state
                .command_execution
                .as_ref()
                .filter(|execution| execution.status.is_active())
        }) {
            return Err(TaskSupervisorError::ActiveCommandExecution {
                node_id: node_id.clone(),
                request_id: execution.request_id.clone(),
            });
        }
        let state = self.require_state_mut(node_id)?;
        let from = state.status;
        if from.is_terminal() {
            return Err(TaskSupervisorError::InvalidTransition {
                node_id: node_id.clone(),
                from,
                to: TaskNodeStatus::Cancelled,
            });
        }
        state.status = TaskNodeStatus::Cancelled;
        state.output = None;
        state.finished_at = Some(now);
        state.updated_at = now;
        self.refresh_runnable(now);
        Ok(())
    }

    /// Add one node through a validated graph candidate.  Existing runtime
    /// state is retained; the new node enters the normal dependency refresh.
    pub fn add_node(
        &mut self,
        node: TaskNode,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        let mut candidate = self.graph.clone();
        candidate.nodes.push(node);
        self.apply_graph_candidate(candidate, Vec::new(), now)
    }

    /// Replace one node definition.  The changed node and its transitive
    /// dependents lose stale runtime results while unrelated branches remain.
    pub fn update_node(
        &mut self,
        node: TaskNode,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        if self.graph.node(&node.id).is_none() {
            return Err(TaskSupervisorError::UnknownNode(node.id.to_string()));
        }
        let node_id = node.id.clone();
        let mut candidate = self.graph.clone();
        let target = candidate
            .node_mut(&node_id)
            .expect("node existence checked immediately above");
        *target = node;
        self.apply_graph_candidate(candidate, vec![node_id], now)
    }

    /// Remove a node and all incident edges.  Existing downstream nodes are
    /// invalidated because their dependency definition has changed.
    pub fn remove_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        if self.graph.node(node_id).is_none() {
            return Err(TaskSupervisorError::UnknownNode(node_id.to_string()));
        }
        let invalidation_roots = self.graph.dependents(node_id);
        let mut candidate = self.graph.clone();
        candidate.nodes.retain(|node| &node.id != node_id);
        candidate
            .edges
            .retain(|edge| &edge.from != node_id && &edge.to != node_id);
        self.apply_graph_candidate(candidate, invalidation_roots, now)
    }

    /// Add a dependency edge.  Candidate validation rejects missing endpoints,
    /// duplicate edges, self edges, and cycles before the revision advances.
    pub fn add_edge(
        &mut self,
        edge: TaskEdge,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        let invalidation_root = edge.to.clone();
        let mut candidate = self.graph.clone();
        candidate.edges.push(edge);
        self.apply_graph_candidate(candidate, vec![invalidation_root], now)
    }

    /// Remove one dependency edge and invalidate the affected downstream
    /// branch.  Removing an unknown edge is a no-op rejection.
    pub fn remove_edge(
        &mut self,
        edge: &TaskEdge,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        if !self.graph.edges.iter().any(|candidate| candidate == edge) {
            return Err(TaskSupervisorError::UnknownEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            });
        }
        let mut candidate = self.graph.clone();
        candidate.edges.retain(|candidate| candidate != edge);
        self.apply_graph_candidate(candidate, vec![edge.to.clone()], now)
    }

    /// Change one node's structured input and bump the graph revision.  The
    /// changed node and only its transitive dependents lose their old runtime
    /// result.  Unrelated branches retain their state, attempts, and output.
    pub fn update_node_input(
        &mut self,
        node_id: &TaskNodeId,
        input: serde_json::Value,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        let mut node = self
            .graph
            .node(node_id)
            .cloned()
            .ok_or_else(|| TaskSupervisorError::UnknownNode(node_id.to_string()))?;
        node.input = input;
        self.update_node(node, now)
    }

    pub fn checkpoint(&self, now: i64) -> TaskCheckpoint {
        TaskCheckpoint {
            id: TaskCheckpointId::generate(),
            graph_id: self.graph.id.clone(),
            graph_revision: self.graph.revision,
            graph: self.graph.clone(),
            node_states: self.node_states.clone(),
            created_at: now,
        }
    }

    /// Restore a complete graph/runtime snapshot atomically after validating
    /// graph identity and one-to-one state coverage.
    ///
    /// A restore is a new write to the live graph, so it always advances from
    /// the supervisor's current revision.  The checkpoint's graph definition
    /// and node state are restored, but its historical revision is never
    /// allowed to move the live revision backwards.
    pub fn restore(
        &mut self,
        checkpoint: &TaskCheckpoint,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        self.ensure_no_active_command_execution()?;
        if let Some((node_id, request_id)) = checkpoint.node_states.iter().find_map(|state| {
            state
                .command_execution
                .as_ref()
                .filter(|execution| execution.status.is_active())
                .map(|execution| (state.node_id.clone(), execution.request_id.clone()))
        }) {
            return Err(TaskSupervisorError::ActiveCommandExecution {
                node_id,
                request_id,
            });
        }
        self.validate_checkpoint(checkpoint)?;
        let next_revision = self.graph.revision.next()?;

        self.graph = checkpoint.graph.clone();
        self.graph.revision = next_revision;
        self.node_states = checkpoint.node_states.clone();
        for state in &mut self.node_states {
            if let Some(execution) = state.command_execution.as_mut() {
                execution.graph_revision = next_revision;
            }
        }
        Ok(next_revision)
    }

    /// Hydrate a persisted snapshot without creating a new live write.
    ///
    /// Persistence loading has already stored the graph revision as part of
    /// the snapshot.  It still uses the same fail-closed checkpoint
    /// validation as [`Self::restore`], but must preserve that stored revision
    /// exactly instead of manufacturing a new one during process startup.
    pub(crate) fn restore_exact(
        &mut self,
        checkpoint: &TaskCheckpoint,
    ) -> Result<(), TaskSupervisorError> {
        self.validate_checkpoint(checkpoint)?;
        self.graph = checkpoint.graph.clone();
        self.node_states = checkpoint.node_states.clone();
        Ok(())
    }

    fn validate_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<(), TaskSupervisorError> {
        if checkpoint.graph_id != self.graph.id {
            return Err(TaskSupervisorError::CheckpointGraphMismatch {
                expected: self.graph.id.clone(),
                actual: checkpoint.graph_id.clone(),
            });
        }
        if checkpoint.graph.id != checkpoint.graph_id {
            return Err(TaskSupervisorError::InvalidCheckpoint(
                "checkpoint graph identity does not match its graph_id".to_string(),
            ));
        }
        if checkpoint.graph.revision != checkpoint.graph_revision {
            return Err(TaskSupervisorError::InvalidCheckpoint(
                "checkpoint graph revision does not match its graph_revision".to_string(),
            ));
        }
        checkpoint.graph.validate()?;

        if checkpoint.node_states.len() != checkpoint.graph.nodes.len() {
            return Err(TaskSupervisorError::InvalidCheckpoint(
                "checkpoint node state count does not match graph".to_string(),
            ));
        }
        let graph_ids: Vec<&TaskNodeId> =
            checkpoint.graph.nodes.iter().map(|node| &node.id).collect();
        let mut state_ids = HashSet::with_capacity(checkpoint.node_states.len());
        for state in &checkpoint.node_states {
            if !graph_ids.iter().any(|node_id| *node_id == &state.node_id)
                || !state_ids.insert(&state.node_id)
            {
                return Err(TaskSupervisorError::InvalidCheckpoint(format!(
                    "checkpoint has invalid or duplicate node state: {}",
                    state.node_id
                )));
            }
        }
        if graph_ids.iter().any(|node_id| !state_ids.contains(node_id)) {
            return Err(TaskSupervisorError::InvalidCheckpoint(
                "checkpoint is missing a graph node state".to_string(),
            ));
        }
        for state in &checkpoint.node_states {
            let Some(execution) = state.command_execution.as_ref() else {
                continue;
            };
            let node = checkpoint
                .graph
                .node(&state.node_id)
                .expect("checkpoint state coverage was validated above");
            let binding = node.command_binding()?.ok_or_else(|| {
                TaskSupervisorError::InvalidCheckpoint(format!(
                    "command execution on node {} has no supported binding",
                    state.node_id
                ))
            })?;
            if execution.command != binding.command
                || execution.app_id != binding.args.app_id
                || execution.graph_revision != checkpoint.graph.revision
                || execution.attempt != state.attempts
                || execution.request_id.trim().is_empty()
                || execution.request_id.len() > 128
                || execution.approval_id.as_ref().is_some_and(|approval_id| {
                    approval_id.trim().is_empty() || approval_id.len() > 128
                })
            {
                return Err(TaskSupervisorError::InvalidCheckpoint(format!(
                    "command execution correlation is invalid for node {}",
                    state.node_id
                )));
            }
            let state_matches_phase = match execution.status {
                TaskCommandExecutionStatus::Dispatching
                | TaskCommandExecutionStatus::WaitingApproval
                | TaskCommandExecutionStatus::Running => state.status == TaskNodeStatus::Running,
                TaskCommandExecutionStatus::Verified => state.status == TaskNodeStatus::Succeeded,
                TaskCommandExecutionStatus::Failed => state.status == TaskNodeStatus::Failed,
                TaskCommandExecutionStatus::Cancelled => state.status == TaskNodeStatus::Cancelled,
            };
            if !state_matches_phase {
                return Err(TaskSupervisorError::InvalidCheckpoint(format!(
                    "command execution phase does not match node state for {}",
                    state.node_id
                )));
            }
        }
        Ok(())
    }

    fn dependencies_succeeded(&self, node_id: &TaskNodeId) -> bool {
        self.graph.dependencies(node_id).iter().all(|dependency| {
            self.node_state(dependency)
                .is_some_and(|state| state.status == TaskNodeStatus::Succeeded)
        })
    }

    fn apply_graph_candidate(
        &mut self,
        mut candidate: TaskGraph,
        invalidation_roots: Vec<TaskNodeId>,
        now: i64,
    ) -> Result<GraphRevision, TaskSupervisorError> {
        self.ensure_no_active_command_execution()?;
        if candidate.id != self.graph.id {
            return Err(TaskSupervisorError::InvalidCheckpoint(
                "graph edit cannot change graph identity".to_string(),
            ));
        }
        // Validate the complete candidate before touching the authoritative
        // graph or incrementing its revision.
        candidate.validate()?;
        let next_revision = self.graph.revision.next()?;
        candidate.revision = next_revision;
        candidate.validate()?;

        let affected = transitive_dependents(&candidate, invalidation_roots);
        let previous_states: HashMap<TaskNodeId, TaskNodeState> = self
            .node_states
            .iter()
            .cloned()
            .map(|state| (state.node_id.clone(), state))
            .collect();
        let mut next_states = Vec::with_capacity(candidate.nodes.len());
        for node in &candidate.nodes {
            let mut state = previous_states
                .get(&node.id)
                .cloned()
                .unwrap_or_else(|| TaskNodeState::new(node.id.clone(), now));
            if affected.contains(&node.id) {
                invalidate_state(&mut state, now);
            }
            if let Some(execution) = state.command_execution.as_mut() {
                // Active command executions are rejected above. A retained
                // terminal result belongs to this new graph revision, so its
                // correlation must move with the graph before persistence.
                execution.graph_revision = next_revision;
            }
            next_states.push(state);
        }

        self.graph = candidate;
        self.node_states = next_states;
        self.refresh_runnable(now);
        Ok(next_revision)
    }

    fn node_state_mut(&mut self, node_id: &TaskNodeId) -> Option<&mut TaskNodeState> {
        self.node_states
            .iter_mut()
            .find(|state| &state.node_id == node_id)
    }

    fn require_state_mut(
        &mut self,
        node_id: &TaskNodeId,
    ) -> Result<&mut TaskNodeState, TaskSupervisorError> {
        self.node_state_mut(node_id)
            .ok_or_else(|| TaskSupervisorError::UnknownNode(node_id.to_string()))
    }

    fn ensure_no_active_command_execution(&self) -> Result<(), TaskSupervisorError> {
        let Some(state) = self.node_states.iter().find(|state| {
            state
                .command_execution
                .as_ref()
                .is_some_and(|execution| execution.status.is_active())
        }) else {
            return Ok(());
        };
        let execution = state
            .command_execution
            .as_ref()
            .expect("active command predicate found a command execution");
        Err(TaskSupervisorError::ActiveCommandExecution {
            node_id: state.node_id.clone(),
            request_id: execution.request_id.clone(),
        })
    }

    fn require_command_execution_mut(
        &mut self,
        node_id: &TaskNodeId,
        request_id: &str,
        app_id: &str,
    ) -> Result<&mut TaskCommandExecution, TaskSupervisorError> {
        let current_graph_revision = self.graph.revision;
        let state = self.require_state_mut(node_id)?;
        let Some(execution) = state.command_execution.as_mut() else {
            return Err(TaskSupervisorError::InvalidCommandExecution(format!(
                "node {node_id} has no command execution"
            )));
        };
        if execution.request_id != request_id
            || execution.app_id != app_id
            || execution.attempt != state.attempts
            || execution.graph_revision != current_graph_revision
        {
            return Err(TaskSupervisorError::InvalidCommandExecution(
                "command result correlation does not match the active node attempt".to_string(),
            ));
        }
        Ok(execution)
    }
}

fn transitive_dependents(graph: &TaskGraph, roots: Vec<TaskNodeId>) -> HashSet<TaskNodeId> {
    let mut affected = HashSet::new();
    let mut pending = roots;
    while let Some(current) = pending.pop() {
        if !affected.insert(current.clone()) {
            continue;
        }
        pending.extend(graph.dependents(&current));
    }
    affected
}

fn command_phase_transition_allowed(
    from: TaskCommandExecutionStatus,
    to: TaskCommandExecutionStatus,
) -> bool {
    matches!(
        (from, to),
        (
            TaskCommandExecutionStatus::Dispatching,
            TaskCommandExecutionStatus::WaitingApproval
        ) | (
            TaskCommandExecutionStatus::Dispatching,
            TaskCommandExecutionStatus::Running
        ) | (
            TaskCommandExecutionStatus::Dispatching,
            TaskCommandExecutionStatus::Failed
        ) | (
            TaskCommandExecutionStatus::Dispatching,
            TaskCommandExecutionStatus::Cancelled
        ) | (
            TaskCommandExecutionStatus::WaitingApproval,
            TaskCommandExecutionStatus::WaitingApproval
        ) | (
            TaskCommandExecutionStatus::WaitingApproval,
            TaskCommandExecutionStatus::Running
        ) | (
            TaskCommandExecutionStatus::WaitingApproval,
            TaskCommandExecutionStatus::Failed
        ) | (
            TaskCommandExecutionStatus::WaitingApproval,
            TaskCommandExecutionStatus::Cancelled
        ) | (
            TaskCommandExecutionStatus::Running,
            TaskCommandExecutionStatus::Running
        ) | (
            TaskCommandExecutionStatus::Running,
            TaskCommandExecutionStatus::Failed
        ) | (
            TaskCommandExecutionStatus::Running,
            TaskCommandExecutionStatus::Cancelled
        )
    )
}

fn invalidate_state(state: &mut TaskNodeState, now: i64) {
    state.status = TaskNodeStatus::Invalidated;
    state.attempts = 0;
    state.output = None;
    state.error = None;
    state.started_at = None;
    state.finished_at = None;
    state.command_execution = None;
    state.updated_at = now;
}

fn bound_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let mut prefix = prefix;
        while prefix.chars().count() + 3 > max_chars {
            prefix.pop();
        }
        format!("{prefix}...")
    } else {
        prefix
    }
}
