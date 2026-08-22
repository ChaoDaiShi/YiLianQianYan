// ============================================================
// Workflow run — the in-memory execution state of a single run.
//
// A [`WorkflowRun`] snapshots a [`WorkflowGraphDefinition`] together with the
// per-node lifecycle state and the overall run status. This module is
// persistence-free and scheduler-free: it only models state and the explicit,
// validated transitions between states.
// ============================================================

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::definition::{WorkflowGraphDefinition, WorkflowNodeId};
use super::state_machine;
use super::validation::WorkflowValidationError;
use crate::execution::ExecutionContext;

/// A validated workflow run identifier.
///
/// Mirrors [`crate::execution::ExecutionId`]: a non-empty, control-character-free
/// string. Fresh ids are generated as UUID v4.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowRunId(String);

impl WorkflowRunId {
    /// Construct a validated run id.
    pub fn new(raw: impl Into<String>) -> Result<Self, WorkflowRunError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(WorkflowRunError::InvalidRunId(
                "run id must not be empty".to_string(),
            ));
        }
        if raw.chars().any(char::is_control) {
            return Err(WorkflowRunError::InvalidRunId(
                "run id must not contain control characters".to_string(),
            ));
        }
        Ok(Self(raw))
    }

    /// Generate a fresh run id (UUID v4).
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkflowRunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The overall lifecycle status of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Created,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for WorkflowRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        })
    }
}

impl WorkflowRunStatus {
    /// Terminal states cannot transition to any other state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// The lifecycle status of a single workflow node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRunStatus {
    Pending,
    Ready,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

impl NodeRunStatus {
    /// Terminal states cannot transition to any other state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }
}

impl std::fmt::Display for NodeRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        })
    }
}

/// Maximum length (in chars) of a persisted node result summary. Truncation is
/// char-safe — it never splits a UTF-8 code point.
pub const MAX_WORKFLOW_NODE_RESULT_CHARS: usize = 8000;

/// A bounded, UI-safe textual result produced by a workflow node.
///
/// Only a truncated textual summary is ever stored — never raw tool arguments,
/// base64 payloads, API keys, secrets, or internal Debug output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRunResult {
    pub summary: String,
}

impl NodeRunResult {
    /// Build a result, truncating the summary to a char-safe bounded length.
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: truncate_result_summary(&summary.into()),
        }
    }
}

fn truncate_result_summary(text: &str) -> String {
    let mut chars = text.chars();
    let mut prefix: String = chars
        .by_ref()
        .take(MAX_WORKFLOW_NODE_RESULT_CHARS)
        .collect();
    if chars.next().is_some() {
        // Reserve room for the "..." suffix so the final length stays in budget.
        while prefix.chars().count() + 3 > MAX_WORKFLOW_NODE_RESULT_CHARS {
            prefix.pop();
        }
        format!("{prefix}...")
    } else {
        prefix
    }
}

/// Build a bounded, UI-safe summary from a raw tool result, replacing
/// binary-ish payloads (e.g. `data:image/...`) with a friendly label.
pub fn safe_tool_result_summary(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("data:image")
        || trimmed.starts_with("data:audio")
        || trimmed.starts_with("data:video")
        || trimmed.starts_with("data:application/octet-stream")
    {
        return "二进制结果已生成".to_string();
    }
    NodeRunResult::new(raw).summary
}

/// The runtime state of a single node.
///
/// Deliberately free of secrets and full tool arguments. It carries a bounded
/// textual result for UI display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRunState {
    pub node_id: WorkflowNodeId,
    pub status: NodeRunStatus,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub result: Option<NodeRunResult>,
}

impl NodeRunState {
    fn new(node_id: WorkflowNodeId, status: NodeRunStatus) -> Self {
        Self {
            node_id,
            status,
            started_at: None,
            finished_at: None,
            error: None,
            result: None,
        }
    }

    /// Apply a state transition, validating that it is legal.
    pub fn transition(&mut self, to: NodeRunStatus, now: i64) -> Result<(), WorkflowRunError> {
        let from = self.status;
        if !state_machine::is_allowed_transition(from, to) {
            return Err(WorkflowRunError::InvalidTransition {
                node_id: self.node_id.clone(),
                from,
                to,
            });
        }
        self.status = to;
        if to == NodeRunStatus::Running {
            if self.started_at.is_none() {
                self.started_at = Some(now);
            }
        } else if to.is_terminal() {
            self.finished_at = Some(now);
        }
        Ok(())
    }
}

/// A safe, non-leaking error surface for workflow run state.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkflowRunError {
    #[error("invalid run id: {0}")]
    InvalidRunId(String),
    #[error("invalid workflow definition: {0}")]
    InvalidDefinition(#[from] WorkflowValidationError),
    #[error("invalid state transition for node {node_id}: {from} -> {to}")]
    InvalidTransition {
        node_id: WorkflowNodeId,
        from: NodeRunStatus,
        to: NodeRunStatus,
    },
    #[error("unknown node: {0}")]
    UnknownNode(String),
}

/// The in-memory state of a single workflow run.
///
/// The definition is a snapshot: editing the source graph later does not affect
/// an in-flight run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub run_id: WorkflowRunId,
    pub execution_context: ExecutionContext,
    pub definition: WorkflowGraphDefinition,
    pub status: WorkflowRunStatus,
    pub node_states: Vec<NodeRunState>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl WorkflowRun {
    /// Create a new run from a validated definition.
    ///
    /// The definition is re-validated, the entry node is marked `Ready`, every
    /// other node `Pending`, and the run starts as `Created`.
    pub fn new(
        run_id: WorkflowRunId,
        execution_context: ExecutionContext,
        definition: WorkflowGraphDefinition,
        created_at: i64,
    ) -> Result<Self, WorkflowRunError> {
        definition.validate()?;

        let node_states = definition
            .nodes
            .iter()
            .map(|node| {
                let status = if node.id == definition.entry_node_id {
                    NodeRunStatus::Ready
                } else {
                    NodeRunStatus::Pending
                };
                NodeRunState::new(node.id.clone(), status)
            })
            .collect();

        Ok(Self {
            run_id,
            execution_context,
            definition,
            status: WorkflowRunStatus::Created,
            node_states,
            created_at,
            updated_at: created_at,
        })
    }

    /// Look up a node's state.
    pub fn node(&self, node_id: &WorkflowNodeId) -> Option<&NodeRunState> {
        self.node_states.iter().find(|s| &s.node_id == node_id)
    }

    pub fn node_mut(&mut self, node_id: &WorkflowNodeId) -> Option<&mut NodeRunState> {
        self.node_states.iter_mut().find(|s| &s.node_id == node_id)
    }

    /// Transition a single node and refresh the run status.
    pub fn transition_node(
        &mut self,
        node_id: &WorkflowNodeId,
        to: NodeRunStatus,
        now: i64,
    ) -> Result<(), WorkflowRunError> {
        let state = self
            .node_mut(node_id)
            .ok_or_else(|| WorkflowRunError::UnknownNode(node_id.to_string()))?;
        state.transition(to, now)?;
        self.updated_at = now;
        self.refresh_status();
        Ok(())
    }

    /// Recompute the run status from node states.
    pub fn refresh_status(&mut self) {
        self.status = state_machine::derive_run_status(&self.node_states);
    }

    /// The currently eligible (pending, deps satisfied) nodes, in definition
    /// order.
    pub fn ready_nodes(&self) -> Vec<WorkflowNodeId> {
        state_machine::ready_nodes(&self.definition, &self.node_states)
    }

    /// Cancel the run: mark the run and every non-terminal node as cancelled.
    pub fn cancel(&mut self, now: i64) {
        for state in &mut self.node_states {
            if !state.status.is_terminal() {
                state.status = NodeRunStatus::Cancelled;
                state.finished_at = Some(now);
            }
        }
        self.status = WorkflowRunStatus::Cancelled;
        self.updated_at = now;
    }
}
