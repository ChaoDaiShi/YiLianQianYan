//! Task-owned execution attempts.
//!
//! A [`NodeExecution`] is one immutable-at-creation attempt at running a
//! TaskGraph node.  It is intentionally separate from the graph's current
//! `TaskNodeState`: graph edits and partial reruns retain this history instead
//! of overwriting the last result.  The module contains no executor logic and
//! no native side effects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::executor_ref::ExecutorRef;
use super::task_graph::{TaskGraphId, TaskNodeId};

pub const MAX_NODE_EXECUTION_ID_CHARS: usize = 128;
pub const MAX_NODE_EXECUTION_ERROR_CHARS: usize = 2_000;
pub const MAX_NODE_EXECUTION_OUTPUT_CHARS: usize = 32_000;
pub const MAX_RETRY_ATTEMPTS: u32 = 8;
pub const MAX_RETRY_BACKOFF_MS: u64 = 60_000;
pub const MAX_RETRY_FAILURE_CODES: usize = 16;
pub const MAX_RETRY_FAILURE_CODE_CHARS: usize = 128;
pub const MAX_NODE_CONTEXT_GOAL_CHARS: usize = 4_000;
pub const MAX_NODE_CONTEXT_INSTRUCTION_CHARS: usize = 8_000;
pub const MAX_NODE_CONTEXT_ITEMS: usize = 64;
pub const MAX_NODE_CONTEXT_ITEM_CHARS: usize = 512;
pub const MAX_NODE_CONTEXT_DEPENDENCY_OUTPUTS: usize = 64;
pub const MAX_NODE_CONTEXT_DEPENDENCY_KEY_CHARS: usize = 128;
pub const MAX_NODE_CONTEXT_VALUE_CHARS: usize = 16_000;

/// Stable identity for one node execution attempt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeExecutionId(String);

impl NodeExecutionId {
    pub fn new(raw: impl Into<String>) -> Result<Self, NodeExecutionError> {
        let raw = raw.into();
        if raw.trim().is_empty()
            || raw.chars().count() > MAX_NODE_EXECUTION_ID_CHARS
            || raw.chars().any(char::is_control)
        {
            return Err(NodeExecutionError::InvalidId);
        }
        Ok(Self(raw))
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeExecutionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Lifecycle of one TaskGraph node execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeExecutionStatus {
    Pending,
    Ready,
    Dispatching,
    WaitingApproval,
    Running,
    Validating,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    Stale,
}

impl NodeExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled | Self::Stale
        )
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Dispatching | Self::WaitingApproval | Self::Running | Self::Validating
        )
    }
}

impl std::fmt::Display for NodeExecutionStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Dispatching => "dispatching",
            Self::WaitingApproval => "waiting_approval",
            Self::Running => "running",
            Self::Validating => "validating",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::Stale => "stale",
        };
        formatter.write_str(value)
    }
}

/// The result of independent validation. This is deliberately not folded
/// into the adapter output: a provider's nominal success is not completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub status: ValidationStatus,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<i64>,
}

impl ValidationResult {
    pub fn pending() -> Self {
        Self {
            status: ValidationStatus::Pending,
            issues: Vec::new(),
            checked_at: None,
        }
    }

    pub fn accepted() -> Self {
        Self {
            status: ValidationStatus::Accepted,
            issues: Vec::new(),
            checked_at: None,
        }
    }

    pub fn rejected(issues: Vec<String>) -> Self {
        Self {
            status: ValidationStatus::Rejected,
            issues: issues
                .into_iter()
                .map(|issue| bound_text(&issue, MAX_NODE_EXECUTION_ERROR_CHARS))
                .collect(),
            checked_at: None,
        }
    }

    pub fn verified(self) -> bool {
        self.status == ValidationStatus::Accepted
    }
}

/// Bounded retry configuration owned by the execution layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRetryPolicy {
    pub max_attempts: u32,
    #[serde(default)]
    pub backoff_ms: u64,
    #[serde(default)]
    pub retryable_failure_codes: Vec<String>,
}

impl Default for ExecutionRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_ms: 0,
            retryable_failure_codes: Vec::new(),
        }
    }
}

impl ExecutionRetryPolicy {
    pub fn new(
        max_attempts: u32,
        backoff_ms: u64,
        retryable_failure_codes: Vec<String>,
    ) -> Result<Self, NodeExecutionError> {
        if max_attempts == 0 || max_attempts > MAX_RETRY_ATTEMPTS {
            return Err(NodeExecutionError::InvalidRetryPolicy);
        }
        if backoff_ms > MAX_RETRY_BACKOFF_MS
            || retryable_failure_codes.len() > MAX_RETRY_FAILURE_CODES
            || retryable_failure_codes.iter().any(|code| {
                code.trim().is_empty()
                    || code.chars().count() > MAX_RETRY_FAILURE_CODE_CHARS
                    || code.chars().any(char::is_control)
            })
        {
            return Err(NodeExecutionError::InvalidRetryPolicy);
        }
        Ok(Self {
            max_attempts,
            backoff_ms,
            retryable_failure_codes,
        })
    }

    pub fn can_retry(&self, attempt: u32, failure_code: Option<&str>) -> bool {
        attempt < self.max_attempts
            && failure_code.is_some_and(|code| {
                self.retryable_failure_codes
                    .iter()
                    .any(|candidate| candidate == code)
            })
    }

    pub fn delay_ms(&self) -> u64 {
        self.backoff_ms
    }
}

/// The bounded context projection supplied to one node attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeContext {
    pub goal: String,
    pub instructions: String,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub dependency_outputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub memory_references: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
}

impl NodeContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        goal: impl Into<String>,
        instructions: impl Into<String>,
        resources: Vec<String>,
        dependency_outputs: Vec<(String, Value)>,
        memory_references: Vec<String>,
        capabilities: Vec<String>,
        constraints: Vec<String>,
        acceptance_criteria: Vec<String>,
    ) -> Result<Self, NodeExecutionError> {
        let context = Self {
            goal: goal.into(),
            instructions: instructions.into(),
            resources,
            dependency_outputs: dependency_outputs.into_iter().collect(),
            memory_references,
            capabilities,
            constraints,
            acceptance_criteria,
        };
        context.validate()?;
        Ok(context)
    }

    pub fn validate(&self) -> Result<(), NodeExecutionError> {
        if self.goal.chars().count() > MAX_NODE_CONTEXT_GOAL_CHARS
            || self.instructions.chars().count() > MAX_NODE_CONTEXT_INSTRUCTION_CHARS
            || self.dependency_outputs.len() > MAX_NODE_CONTEXT_DEPENDENCY_OUTPUTS
        {
            return Err(NodeExecutionError::ContextTooLarge);
        }
        for item in self
            .resources
            .iter()
            .chain(self.memory_references.iter())
            .chain(self.capabilities.iter())
            .chain(self.constraints.iter())
            .chain(self.acceptance_criteria.iter())
        {
            if item.chars().count() > MAX_NODE_CONTEXT_ITEM_CHARS
                || item.chars().any(char::is_control)
            {
                return Err(NodeExecutionError::ContextTooLarge);
            }
        }
        if self.resources.len() > MAX_NODE_CONTEXT_ITEMS
            || self.memory_references.len() > MAX_NODE_CONTEXT_ITEMS
            || self.capabilities.len() > MAX_NODE_CONTEXT_ITEMS
            || self.constraints.len() > MAX_NODE_CONTEXT_ITEMS
            || self.acceptance_criteria.len() > MAX_NODE_CONTEXT_ITEMS
        {
            return Err(NodeExecutionError::ContextTooLarge);
        }
        for (key, value) in &self.dependency_outputs {
            if key.trim().is_empty()
                || key.chars().count() > MAX_NODE_CONTEXT_DEPENDENCY_KEY_CHARS
                || key.chars().any(char::is_control)
                || value.to_string().chars().count() > MAX_NODE_CONTEXT_VALUE_CHARS
            {
                return Err(NodeExecutionError::ContextTooLarge);
            }
        }
        Ok(())
    }
}

/// One append-only execution attempt and its independent evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeExecution {
    pub id: NodeExecutionId,
    pub graph_id: TaskGraphId,
    pub node_id: TaskNodeId,
    pub attempt: u32,
    pub status: NodeExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_ref: Option<ExecutorRef>,
    pub context: NodeContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub retry_policy: ExecutionRetryPolicy,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

impl NodeExecution {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: NodeExecutionId,
        graph_id: TaskGraphId,
        node_id: TaskNodeId,
        attempt: u32,
        executor_ref: Option<ExecutorRef>,
        context: NodeContext,
        now: i64,
    ) -> Result<Self, NodeExecutionError> {
        if attempt == 0 {
            return Err(NodeExecutionError::InvalidAttempt);
        }
        context.validate()?;
        Ok(Self {
            id,
            graph_id,
            node_id,
            attempt,
            status: NodeExecutionStatus::Pending,
            executor_ref,
            context,
            output: None,
            validation: None,
            audit_ref: None,
            approval_ref: None,
            failure_code: None,
            error: None,
            retry_policy: ExecutionRetryPolicy::default(),
            created_at: now,
            updated_at: now,
            started_at: None,
            finished_at: None,
        })
    }

    pub fn with_retry_policy(
        mut self,
        policy: ExecutionRetryPolicy,
    ) -> Result<Self, NodeExecutionError> {
        ExecutionRetryPolicy::new(
            policy.max_attempts,
            policy.backoff_ms,
            policy.retryable_failure_codes.clone(),
        )?;
        self.retry_policy = policy;
        Ok(self)
    }

    pub fn transition(
        &mut self,
        to: NodeExecutionStatus,
        now: i64,
    ) -> Result<(), NodeExecutionError> {
        if !transition_allowed(self.status, to) {
            return Err(NodeExecutionError::InvalidTransition {
                from: self.status,
                to,
            });
        }
        self.status = to;
        self.updated_at = now;
        if matches!(
            to,
            NodeExecutionStatus::Dispatching | NodeExecutionStatus::Running
        ) && self.started_at.is_none()
        {
            self.started_at = Some(now);
        }
        if to.is_terminal() {
            self.finished_at = Some(now);
        }
        Ok(())
    }

    pub fn set_output(&mut self, output: Value) -> Result<(), NodeExecutionError> {
        if self.status.is_terminal() {
            return Err(NodeExecutionError::TerminalMutation);
        }
        if output.to_string().chars().count() > MAX_NODE_EXECUTION_OUTPUT_CHARS {
            return Err(NodeExecutionError::OutputTooLarge);
        }
        self.output = Some(output);
        Ok(())
    }

    pub fn set_validation(&mut self, validation: ValidationResult) {
        self.validation = Some(validation);
    }

    pub fn set_failure(&mut self, code: impl Into<String>, error: impl Into<String>) {
        self.failure_code = Some(bound_text(&code.into(), MAX_NODE_EXECUTION_ERROR_CHARS));
        self.error = Some(bound_text(&error.into(), MAX_NODE_EXECUTION_ERROR_CHARS));
    }

    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }

    /// Normalize active attempts on startup. A restarted process has no proof
    /// that the provider kept running, so it fails closed and leaves evidence
    /// for a future retry instead of pretending work continued.
    pub fn recover_after_restart(&mut self, now: i64) -> NodeExecutionStatus {
        if self.status.is_active() {
            self.status = NodeExecutionStatus::Failed;
            self.set_failure(
                "execution_interrupted",
                "execution interrupted by service restart; provider state unavailable",
            );
            self.updated_at = now;
            self.finished_at = Some(now);
        }
        self.status
    }

    /// Mark an existing attempt stale after a graph/input edit. The row is
    /// retained in history; only its current reconciliation status changes.
    pub fn mark_stale(&mut self, now: i64, reason: impl Into<String>) {
        if self.status.is_terminal() || self.status == NodeExecutionStatus::Pending {
            self.status = NodeExecutionStatus::Stale;
            self.validation = None;
            self.error = Some(bound_text(&reason.into(), MAX_NODE_EXECUTION_ERROR_CHARS));
            self.updated_at = now;
            self.finished_at = Some(now);
        }
    }
}

/// Short name retained inside the execution module for callers that import
/// `task::execution::*`; the graph-definition `task::RetryPolicy` remains a
/// separate wire-compatible type.
pub type RetryPolicy = ExecutionRetryPolicy;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NodeExecutionError {
    #[error("node execution id is invalid")]
    InvalidId,
    #[error("node execution attempt must be greater than zero")]
    InvalidAttempt,
    #[error("node execution transition is invalid: {from} -> {to}")]
    InvalidTransition {
        from: NodeExecutionStatus,
        to: NodeExecutionStatus,
    },
    #[error("retry policy is invalid or exceeds bounded limits")]
    InvalidRetryPolicy,
    #[error("node context exceeds bounded limits")]
    ContextTooLarge,
    #[error("node execution output exceeds bounded limits")]
    OutputTooLarge,
    #[error("terminal node execution cannot be mutated")]
    TerminalMutation,
}

fn transition_allowed(from: NodeExecutionStatus, to: NodeExecutionStatus) -> bool {
    matches!(
        (from, to),
        (NodeExecutionStatus::Pending, NodeExecutionStatus::Ready)
            | (NodeExecutionStatus::Pending, NodeExecutionStatus::Blocked)
            | (NodeExecutionStatus::Pending, NodeExecutionStatus::Cancelled)
            | (NodeExecutionStatus::Pending, NodeExecutionStatus::Stale)
            | (NodeExecutionStatus::Ready, NodeExecutionStatus::Dispatching)
            | (NodeExecutionStatus::Ready, NodeExecutionStatus::Blocked)
            | (NodeExecutionStatus::Ready, NodeExecutionStatus::Cancelled)
            | (NodeExecutionStatus::Ready, NodeExecutionStatus::Stale)
            | (
                NodeExecutionStatus::Dispatching,
                NodeExecutionStatus::WaitingApproval
            )
            | (
                NodeExecutionStatus::Dispatching,
                NodeExecutionStatus::Running
            )
            | (
                NodeExecutionStatus::Dispatching,
                NodeExecutionStatus::Failed
            )
            | (
                NodeExecutionStatus::Dispatching,
                NodeExecutionStatus::Cancelled
            )
            | (
                NodeExecutionStatus::WaitingApproval,
                NodeExecutionStatus::WaitingApproval
            )
            | (
                NodeExecutionStatus::WaitingApproval,
                NodeExecutionStatus::Running
            )
            | (
                NodeExecutionStatus::WaitingApproval,
                NodeExecutionStatus::Failed
            )
            | (
                NodeExecutionStatus::WaitingApproval,
                NodeExecutionStatus::Cancelled
            )
            | (NodeExecutionStatus::Running, NodeExecutionStatus::Running)
            | (
                NodeExecutionStatus::Running,
                NodeExecutionStatus::Validating
            )
            | (NodeExecutionStatus::Running, NodeExecutionStatus::Failed)
            | (NodeExecutionStatus::Running, NodeExecutionStatus::Cancelled)
            | (
                NodeExecutionStatus::Validating,
                NodeExecutionStatus::Succeeded
            )
            | (NodeExecutionStatus::Validating, NodeExecutionStatus::Failed)
            | (
                NodeExecutionStatus::Validating,
                NodeExecutionStatus::Cancelled
            )
    )
}

fn bound_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut value: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        while value.chars().count() + 3 > max_chars {
            value.pop();
        }
        value.push_str("...");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ExecutorRef, TaskGraphId, TaskNodeId};
    use serde_json::json;

    fn context() -> NodeContext {
        NodeContext::new(
            "prepare report",
            "Collect the report inputs",
            vec!["resource://brief".to_string()],
            vec![("upstream".to_string(), json!({"verified": true}))],
            vec!["memory:brief".to_string()],
            vec!["reports.read".to_string()],
            vec!["offline".to_string()],
            vec!["result.status == success".to_string()],
        )
        .expect("bounded context")
    }

    #[test]
    fn execution_attempt_transitions_are_legal_and_fail_closed() {
        let graph_id = TaskGraphId::new("graph").unwrap();
        let node_id = TaskNodeId::new("node").unwrap();
        let executor = ExecutorRef::new("agent://writer").unwrap();
        let mut execution = NodeExecution::new(
            NodeExecutionId::new("execution-1").unwrap(),
            graph_id,
            node_id.clone(),
            1,
            Some(executor),
            context(),
            100,
        )
        .unwrap();

        assert_eq!(execution.status, NodeExecutionStatus::Pending);
        execution
            .transition(NodeExecutionStatus::Ready, 101)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Dispatching, 102)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Running, 103)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Validating, 104)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Succeeded, 105)
            .unwrap();
        assert!(execution.status.is_terminal());
        assert!(execution
            .transition(NodeExecutionStatus::Running, 106)
            .is_err());
    }

    #[test]
    fn waiting_approval_and_recovery_transitions_are_explicit() {
        let mut execution = NodeExecution::new(
            NodeExecutionId::new("execution-approval").unwrap(),
            TaskGraphId::new("graph").unwrap(),
            TaskNodeId::new("node").unwrap(),
            1,
            None,
            context(),
            100,
        )
        .unwrap();
        execution
            .transition(NodeExecutionStatus::Ready, 101)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Dispatching, 102)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::WaitingApproval, 103)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Running, 104)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Validating, 105)
            .unwrap();
        assert_eq!(execution.status, NodeExecutionStatus::Validating);

        let recovered = execution.recover_after_restart(106);
        assert_eq!(recovered, NodeExecutionStatus::Failed);
        assert_eq!(execution.failure_code(), Some("execution_interrupted"));
    }

    #[test]
    fn retry_policy_is_bounded_and_attempts_are_distinct() {
        let default = RetryPolicy::default();
        assert_eq!(default.max_attempts, 1);
        assert!(RetryPolicy::new(0, 0, Vec::new()).is_err());
        assert!(RetryPolicy::new(MAX_RETRY_ATTEMPTS + 1, 0, Vec::new()).is_err());
        assert!(RetryPolicy::new(2, MAX_RETRY_BACKOFF_MS + 1, Vec::new()).is_err());

        let first = NodeExecutionId::new("attempt-1").unwrap();
        let second = NodeExecutionId::new("attempt-2").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn output_validation_and_result_are_kept_separate() {
        let mut execution = NodeExecution::new(
            NodeExecutionId::new("execution-result").unwrap(),
            TaskGraphId::new("graph").unwrap(),
            TaskNodeId::new("node").unwrap(),
            1,
            None,
            context(),
            100,
        )
        .unwrap();
        execution
            .transition(NodeExecutionStatus::Ready, 101)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Dispatching, 102)
            .unwrap();
        execution
            .transition(NodeExecutionStatus::Running, 103)
            .unwrap();
        execution.set_output(json!({"status": "success"})).unwrap();
        execution
            .transition(NodeExecutionStatus::Validating, 104)
            .unwrap();
        execution.set_validation(ValidationResult::rejected(vec![
            "required criterion not met".to_string(),
        ]));
        execution
            .transition(NodeExecutionStatus::Failed, 105)
            .unwrap();
        assert!(execution.output.is_some());
        assert_eq!(
            execution.validation.as_ref().unwrap().status,
            ValidationStatus::Rejected
        );
        assert_ne!(execution.status, NodeExecutionStatus::Succeeded);
    }
}
