//! In-process Task Harness orchestration over independent node attempts.
//!
//! This module owns no provider runtime. It creates/persists-ready attempt
//! records, applies deterministic scheduling and validation, and leaves
//! adapter side effects to `adapters.rs`.

use std::collections::HashMap;

use serde_json::Value;
use thiserror::Error;

use super::context::{NodeContextBuildError, NodeContextBuilder};
use super::execution::{
    ExecutionRetryPolicy, NodeExecution, NodeExecutionError, NodeExecutionId, NodeExecutionStatus,
    ValidationResult, ValidationStatus, MAX_RETRY_ATTEMPTS,
};
use super::executor::{ExecutorResolutionError, ExecutorResolver, ResolvedExecutionPlan};
use super::scheduler::{
    DeterministicScheduler, ScheduleDecision, SchedulerError, SchedulerNodeState,
};
use super::validation::{DeterministicValidator, ValidationPolicy, ValidationRequest};
use super::{TaskGraph, TaskGraphId, TaskNodeId};

const RETRYABLE_FAILURE_CODES: &[&str] = &[
    "validation_rejected",
    "provider_error",
    "execution_interrupted",
];

#[derive(Debug, Error)]
pub enum TaskHarnessError {
    #[error("task graph validation failed: {0}")]
    Graph(String),
    #[error("task node is not ready for execution: {0}")]
    NodeNotReady(TaskNodeId),
    #[error("task node is unknown: {0}")]
    UnknownNode(TaskNodeId),
    #[error("node execution is unknown: {0}")]
    UnknownExecution(NodeExecutionId),
    #[error("node execution is still active: {0}")]
    ActiveExecution(NodeExecutionId),
    #[error("node execution cannot be validated in status {0}")]
    InvalidValidationStatus(NodeExecutionStatus),
    #[error("node execution has no output to validate: {0}")]
    MissingOutput(NodeExecutionId),
    #[error("node execution retry is not allowed: {0}")]
    RetryNotAllowed(NodeExecutionId),
    #[error("node execution retry policy is invalid: {0}")]
    RetryPolicy(String),
    #[error("node execution failed: {0}")]
    Execution(#[from] NodeExecutionError),
    #[error("executor resolution failed: {0}")]
    Resolver(#[from] ExecutorResolutionError),
    #[error("node context build failed: {0}")]
    Context(#[from] NodeContextBuildError),
    #[error("scheduler rejected graph: {0}")]
    Scheduler(#[from] SchedulerError),
}

/// Task-owned execution history and reconciliation state for one graph.
#[derive(Debug, Clone)]
pub struct TaskHarness {
    graph: TaskGraph,
    resolver: ExecutorResolver,
    scheduler: DeterministicScheduler,
    context_builder: NodeContextBuilder,
    validator: DeterministicValidator,
    attempts: HashMap<TaskNodeId, Vec<NodeExecution>>,
}

impl TaskHarness {
    pub fn new(graph: TaskGraph, resolver: ExecutorResolver) -> Result<Self, TaskHarnessError> {
        graph
            .validate()
            .map_err(|error| TaskHarnessError::Graph(error.to_string()))?;
        let scheduler = DeterministicScheduler::new(graph.clone())?;
        Ok(Self {
            graph,
            resolver,
            scheduler,
            context_builder: NodeContextBuilder::new(),
            validator: DeterministicValidator::new(),
            attempts: HashMap::new(),
        })
    }

    pub fn graph(&self) -> &TaskGraph {
        &self.graph
    }

    pub fn graph_id(&self) -> &TaskGraphId {
        &self.graph.id
    }

    pub fn resolver(&self) -> &ExecutorResolver {
        &self.resolver
    }

    pub fn set_resolver(&mut self, resolver: ExecutorResolver) {
        self.resolver = resolver;
    }

    /// Rehydrate append-only attempts loaded from the v1 persistence layer.
    /// Rows are validated before entering the authoritative harness map.
    pub fn from_attempts(
        graph: TaskGraph,
        resolver: ExecutorResolver,
        attempts: Vec<NodeExecution>,
    ) -> Result<Self, TaskHarnessError> {
        let mut harness = Self::new(graph, resolver)?;
        for execution in attempts {
            if execution.graph_id != harness.graph.id
                || harness.graph.node(&execution.node_id).is_none()
            {
                return Err(TaskHarnessError::Graph(
                    "execution row does not belong to the harness graph".to_string(),
                ));
            }
            execution.context.validate()?;
            harness
                .attempts
                .entry(execution.node_id.clone())
                .or_default()
                .push(execution);
        }
        for attempts in harness.attempts.values_mut() {
            attempts.sort_by_key(|execution| execution.attempt);
        }
        Ok(harness)
    }

    pub fn scheduler(&self) -> &DeterministicScheduler {
        &self.scheduler
    }

    pub fn update_graph(&mut self, graph: TaskGraph) -> Result<(), TaskHarnessError> {
        if graph.id != self.graph.id {
            return Err(TaskHarnessError::Graph(
                "harness graph identity cannot change".to_string(),
            ));
        }
        graph
            .validate()
            .map_err(|error| TaskHarnessError::Graph(error.to_string()))?;
        self.scheduler = DeterministicScheduler::new(graph.clone())?;
        self.graph = graph;
        Ok(())
    }

    pub fn mark_stale_nodes(
        &mut self,
        node_ids: &[TaskNodeId],
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        for node_id in node_ids {
            if self.graph.node(node_id).is_none() {
                return Err(TaskHarnessError::UnknownNode(node_id.clone()));
            }
            if let Some(attempts) = self.attempts.get_mut(node_id) {
                for execution in attempts {
                    if execution.status.is_active() {
                        return Err(TaskHarnessError::ActiveExecution(execution.id.clone()));
                    }
                    execution.mark_stale(now, "node result invalidated by graph edit");
                }
            }
        }
        Ok(())
    }

    pub fn attempt_history(&self, node_id: &TaskNodeId) -> Vec<NodeExecution> {
        let mut attempts = self.attempts.get(node_id).cloned().unwrap_or_default();
        attempts.sort_by_key(|execution| execution.attempt);
        attempts
    }

    pub fn all_attempts(&self) -> Vec<NodeExecution> {
        let mut attempts = self
            .attempts
            .values()
            .flat_map(|attempts| attempts.iter().cloned())
            .collect::<Vec<_>>();
        attempts.sort_by(|left, right| {
            left.graph_id
                .as_str()
                .cmp(right.graph_id.as_str())
                .then_with(|| left.node_id.as_str().cmp(right.node_id.as_str()))
                .then(left.attempt.cmp(&right.attempt))
        });
        attempts
    }

    pub fn execution_histories(&self) -> HashMap<TaskNodeId, Vec<NodeExecution>> {
        self.attempts.clone()
    }

    pub fn latest_execution(&self, node_id: &TaskNodeId) -> Option<&NodeExecution> {
        self.attempts
            .get(node_id)
            .and_then(|attempts| attempts.iter().max_by_key(|execution| execution.attempt))
    }

    pub fn execution(&self, execution_id: &NodeExecutionId) -> Option<&NodeExecution> {
        self.attempts
            .values()
            .flat_map(|attempts| attempts.iter())
            .find(|execution| &execution.id == execution_id)
    }

    pub fn resolve_execution_plan(
        &self,
        execution_id: &NodeExecutionId,
    ) -> Result<ResolvedExecutionPlan, TaskHarnessError> {
        let execution = self
            .execution(execution_id)
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))?;
        let node = self
            .graph
            .node(&execution.node_id)
            .ok_or_else(|| TaskHarnessError::UnknownNode(execution.node_id.clone()))?;
        self.resolver.resolve_node(node).map_err(Into::into)
    }

    /// Create the next attempt for a scheduler-ready node. No adapter is
    /// invoked; the returned attempt is persisted-ready in `dispatching`.
    pub fn start_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<NodeExecutionId, TaskHarnessError> {
        let states = self.latest_states();
        if !self.scheduler.can_start_node(&states, node_id) {
            return Err(TaskHarnessError::NodeNotReady(node_id.clone()));
        }
        let node = self
            .graph
            .node(node_id)
            .ok_or_else(|| TaskHarnessError::UnknownNode(node_id.clone()))?;
        let dependency_outputs = self.dependency_outputs(node_id);
        let context = self
            .context_builder
            .build(&self.graph, node_id, dependency_outputs)?;
        let executor_ref = if node.input.get("executor_ref").is_some() {
            Some(self.resolver.resolve_node(node)?.executor_ref)
        } else {
            None
        };
        let attempt = self
            .latest_execution(node_id)
            .map_or(1, |execution| execution.attempt.saturating_add(1));
        let retry_policy = retry_policy_for_node(node.retry_policy.max_attempts)?;
        let execution = NodeExecution::new(
            NodeExecutionId::generate(),
            self.graph.id.clone(),
            node_id.clone(),
            attempt,
            executor_ref,
            context,
            now,
        )?
        .with_retry_policy(retry_policy)?;
        let execution_id = execution.id.clone();
        let attempts = self.attempts.entry(node_id.clone()).or_default();
        attempts.push(execution);
        let execution = attempts
            .last_mut()
            .expect("attempt was pushed immediately above");
        execution.transition(NodeExecutionStatus::Ready, now)?;
        execution.transition(NodeExecutionStatus::Dispatching, now)?;
        Ok(execution_id)
    }

    pub fn start_ready_nodes(
        &mut self,
        now: i64,
    ) -> Result<Vec<NodeExecutionId>, TaskHarnessError> {
        let ready = self.schedule().ready;
        ready
            .iter()
            .map(|node_id| self.start_node(node_id, now))
            .collect()
    }

    pub fn mark_waiting_approval(
        &mut self,
        execution_id: &NodeExecutionId,
        approval_ref: impl Into<String>,
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        execution.approval_ref = Some(approval_ref.into());
        execution.transition(NodeExecutionStatus::WaitingApproval, now)?;
        Ok(())
    }

    pub fn mark_running(
        &mut self,
        execution_id: &NodeExecutionId,
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        execution.transition(NodeExecutionStatus::Running, now)?;
        Ok(())
    }

    pub fn record_output(
        &mut self,
        execution_id: &NodeExecutionId,
        output: Value,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        execution.set_output(output)?;
        Ok(())
    }

    /// Finish one adapter attempt as failed while preserving its bounded
    /// provider evidence for retry/recovery decisions.
    pub fn fail_execution(
        &mut self,
        execution_id: &NodeExecutionId,
        code: impl Into<String>,
        error: impl Into<String>,
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        if execution.status.is_terminal() {
            return Err(TaskHarnessError::InvalidValidationStatus(execution.status));
        }
        execution.set_failure(code, error);
        execution.transition(NodeExecutionStatus::Failed, now)?;
        Ok(())
    }

    pub fn cancel_execution(
        &mut self,
        execution_id: &NodeExecutionId,
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        if execution.status.is_terminal() {
            return Err(TaskHarnessError::InvalidValidationStatus(execution.status));
        }
        execution.transition(NodeExecutionStatus::Cancelled, now)?;
        Ok(())
    }

    /// Cancel a local, safely pausable attempt while preserving an explicit
    /// resumable marker.  External provider attempts must not use this path:
    /// their owner remains responsible for cancellation and verification.
    pub fn cancel_execution_for_pause(
        &mut self,
        execution_id: &NodeExecutionId,
        now: i64,
    ) -> Result<(), TaskHarnessError> {
        let execution = self.require_execution_mut(execution_id)?;
        if execution.status.is_terminal() {
            return Err(TaskHarnessError::InvalidValidationStatus(execution.status));
        }
        execution.set_failure(
            "paused",
            "execution paused before completion; no external action rollback claimed",
        );
        execution.transition(NodeExecutionStatus::Cancelled, now)?;
        Ok(())
    }

    /// Independently validate one adapter output and reconcile its terminal
    /// state. Validation rejection is recorded as a failed attempt and never
    /// becomes a successful node merely because dispatch returned normally.
    pub fn validate_execution(
        &mut self,
        execution_id: &NodeExecutionId,
        policy: ValidationPolicy,
        now: i64,
    ) -> Result<ValidationResult, TaskHarnessError> {
        let output = self
            .execution(execution_id)
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))?
            .output
            .clone();
        let status = self
            .execution(execution_id)
            .map(|execution| execution.status)
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))?;
        if !matches!(
            status,
            NodeExecutionStatus::Dispatching
                | NodeExecutionStatus::WaitingApproval
                | NodeExecutionStatus::Running
        ) {
            return Err(TaskHarnessError::InvalidValidationStatus(status));
        }
        if status == NodeExecutionStatus::WaitingApproval {
            return Err(TaskHarnessError::InvalidValidationStatus(status));
        }
        if status == NodeExecutionStatus::Dispatching {
            self.mark_running(execution_id, now)?;
        }
        let dependency_outputs = self
            .execution(execution_id)
            .map(|execution| execution.context.dependency_outputs.clone())
            .unwrap_or_default();
        let result = self.validator.validate(
            ValidationRequest::new(output, policy, now).with_dependency_outputs(dependency_outputs),
        );
        let execution = self.require_execution_mut(execution_id)?;
        execution.transition(NodeExecutionStatus::Validating, now)?;
        execution.set_validation(result.clone());
        if result.status == ValidationStatus::Accepted {
            execution.transition(NodeExecutionStatus::Succeeded, now)?;
        } else {
            execution.set_failure(
                "validation_rejected",
                result
                    .issues
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "deterministic validation rejected output".to_string()),
            );
            execution.transition(NodeExecutionStatus::Failed, now)?;
        }
        Ok(result)
    }

    pub fn retry_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<NodeExecutionId, TaskHarnessError> {
        let latest = self
            .latest_execution(node_id)
            .ok_or_else(|| TaskHarnessError::RetryNotAllowed(NodeExecutionId::generate()))?;
        if latest.status != NodeExecutionStatus::Failed
            || !latest
                .retry_policy
                .can_retry(latest.attempt, latest.failure_code())
        {
            return Err(TaskHarnessError::RetryNotAllowed(latest.id.clone()));
        }
        // A retry must be a fresh row with a distinct identity and the same
        // bounded context/executor evidence as the failed attempt.
        let attempt = latest.attempt.saturating_add(1);
        let mut execution = NodeExecution::new(
            NodeExecutionId::generate(),
            latest.graph_id.clone(),
            latest.node_id.clone(),
            attempt,
            latest.executor_ref.clone(),
            latest.context.clone(),
            now,
        )?
        .with_retry_policy(latest.retry_policy.clone())?;
        execution.transition(NodeExecutionStatus::Ready, now)?;
        execution.transition(NodeExecutionStatus::Dispatching, now)?;
        let execution_id = execution.id.clone();
        self.attempts
            .entry(node_id.clone())
            .or_default()
            .push(execution);
        Ok(execution_id)
    }

    /// Mark exactly the selected node and all transitive dependants stale.
    /// Unrelated verified attempts remain untouched and queryable.
    pub fn rerun_from_node(
        &mut self,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<Vec<TaskNodeId>, TaskHarnessError> {
        if self.graph.node(node_id).is_none() {
            return Err(TaskHarnessError::UnknownNode(node_id.clone()));
        }
        let affected = self
            .scheduler
            .affected_branch(std::slice::from_ref(node_id));
        // Reject the whole branch before invalidating any earlier result.
        for affected_node in &affected {
            if let Some(execution) = self.attempts.get(affected_node).and_then(|attempts| {
                attempts
                    .iter()
                    .find(|execution| execution.status.is_active())
            }) {
                return Err(TaskHarnessError::ActiveExecution(execution.id.clone()));
            }
        }
        for affected_node in &affected {
            if let Some(attempts) = self.attempts.get_mut(affected_node) {
                for execution in attempts {
                    execution.mark_stale(now, "node selected for partial rerun");
                }
            }
        }
        Ok(affected)
    }

    pub fn recover_after_restart(&mut self, now: i64) -> usize {
        let mut recovered = 0;
        for attempts in self.attempts.values_mut() {
            for execution in attempts {
                if execution.status.is_active() {
                    execution.recover_after_restart(now);
                    recovered += 1;
                }
            }
        }
        recovered
    }

    pub fn schedule(&self) -> ScheduleDecision {
        let states = self.latest_states();
        self.scheduler.schedule(&states)
    }

    fn latest_states(&self) -> HashMap<TaskNodeId, SchedulerNodeState> {
        self.graph
            .nodes
            .iter()
            .filter_map(|node| {
                let execution = self.latest_execution(&node.id)?;
                Some((
                    node.id.clone(),
                    SchedulerNodeState {
                        // A local execution cancelled by Task pause is eligible
                        // for a fresh attempt after resume; the cancelled row is
                        // still retained as history.
                        status: if (execution.status == NodeExecutionStatus::Cancelled
                            && execution.failure_code() == Some("paused"))
                            || (execution.status == NodeExecutionStatus::Failed
                                && execution.failure_code() == Some("execution_interrupted"))
                        {
                            // A pause-cancelled or restart-interrupted
                            // attempt remains history, but its node is
                            // eligible for a fresh scheduler reservation.
                            NodeExecutionStatus::Pending
                        } else {
                            execution.status
                        },
                        validation: execution.validation.as_ref().map(|result| result.status),
                    },
                ))
            })
            .collect()
    }

    fn dependency_outputs(&self, node_id: &TaskNodeId) -> Vec<(String, Value)> {
        self.graph
            .dependencies(node_id)
            .into_iter()
            .filter_map(|dependency| {
                let execution = self.latest_execution(&dependency)?;
                (execution.status == NodeExecutionStatus::Succeeded)
                    .then(|| {
                        execution
                            .output
                            .clone()
                            .map(|output| (dependency.to_string(), output))
                    })
                    .flatten()
            })
            .collect()
    }

    fn require_execution_mut(
        &mut self,
        execution_id: &NodeExecutionId,
    ) -> Result<&mut NodeExecution, TaskHarnessError> {
        self.attempts
            .values_mut()
            .flat_map(|attempts| attempts.iter_mut())
            .find(|execution| &execution.id == execution_id)
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))
    }
}

fn retry_policy_for_node(max_attempts: u32) -> Result<ExecutionRetryPolicy, TaskHarnessError> {
    if max_attempts == 0 || max_attempts > MAX_RETRY_ATTEMPTS {
        return Err(TaskHarnessError::RetryPolicy(format!(
            "max_attempts must be between 1 and {MAX_RETRY_ATTEMPTS}"
        )));
    }
    ExecutionRetryPolicy::new(
        max_attempts,
        0,
        RETRYABLE_FAILURE_CODES
            .iter()
            .map(|code| (*code).to_string())
            .collect(),
    )
    .map_err(|error| TaskHarnessError::RetryPolicy(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{GraphRevision, RetryPolicy, TaskEdge, TaskNode, TaskNodeKind};
    use serde_json::json;

    fn id(raw: &str) -> TaskNodeId {
        TaskNodeId::new(raw).unwrap()
    }

    fn harness() -> TaskHarness {
        let graph = TaskGraph::new(
            TaskGraphId::new("harness-tests").unwrap(),
            GraphRevision::initial(),
            vec![
                TaskNode::new(id("source"), TaskNodeKind::Work, "Source", json!({})).unwrap(),
                TaskNode::new(
                    id("dependent"),
                    TaskNodeKind::Work,
                    "Dependent",
                    json!({
                        "acceptance_criteria": ["status is success"]
                    }),
                )
                .unwrap(),
                TaskNode::new(id("unrelated"), TaskNodeKind::Work, "Unrelated", json!({})).unwrap(),
            ],
            vec![TaskEdge::new(id("source"), id("dependent"))],
        )
        .unwrap();
        TaskHarness::new(graph, ExecutorResolver::new()).unwrap()
    }

    #[test]
    fn explicitly_selected_node_is_not_limited_to_the_automatic_ready_prefix() {
        let nodes = ["first", "second", "third", "fourth"]
            .into_iter()
            .map(|name| TaskNode::new(id(name), TaskNodeKind::Work, name, json!({})).unwrap())
            .collect();
        let graph = TaskGraph::new(
            TaskGraphId::new("manual-selection").unwrap(),
            GraphRevision::initial(),
            nodes,
            vec![],
        )
        .unwrap();
        let mut harness = TaskHarness::new(graph, ExecutorResolver::new()).unwrap();

        let execution_id = harness.start_node(&id("fourth"), 10).unwrap();

        assert_eq!(
            harness.execution(&execution_id).unwrap().node_id,
            id("fourth")
        );
    }

    fn complete(
        harness: &mut TaskHarness,
        node_id: &TaskNodeId,
        output: Value,
        now: i64,
    ) -> NodeExecutionId {
        let execution_id = harness.start_node(node_id, now).unwrap();
        harness.mark_running(&execution_id, now + 1).unwrap();
        harness.record_output(&execution_id, output).unwrap();
        harness
            .validate_execution(&execution_id, ValidationPolicy::StructuredResult, now + 2)
            .unwrap();
        execution_id
    }

    #[test]
    fn nominal_output_without_acceptance_evidence_is_failed() {
        let mut harness = harness();
        complete(&mut harness, &id("source"), json!({"ok": true}), 9);
        let execution_id = harness.start_node(&id("dependent"), 10).unwrap();
        harness.mark_running(&execution_id, 11).unwrap();
        harness
            .record_output(&execution_id, json!({"status": "success"}))
            .unwrap();
        let result = harness
            .validate_execution(
                &execution_id,
                ValidationPolicy::RequiredFields {
                    fields: vec!["missing".to_string()],
                },
                12,
            )
            .unwrap();
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert_eq!(
            harness.execution(&execution_id).unwrap().status,
            NodeExecutionStatus::Failed
        );
    }

    #[test]
    fn retry_creates_second_append_only_attempt() {
        // The graph's public retry policy is intentionally copied into the
        // attempt policy by a small test graph below.
        let graph = TaskGraph::new(
            TaskGraphId::new("retry-graph").unwrap(),
            GraphRevision::initial(),
            vec![
                TaskNode::new(id("retry"), TaskNodeKind::Work, "Retry", json!({}))
                    .unwrap()
                    .with_retry_policy(RetryPolicy::new(2).unwrap()),
            ],
            vec![],
        )
        .unwrap();
        let mut harness = TaskHarness::new(graph, ExecutorResolver::new()).unwrap();
        let execution_id = harness.start_node(&id("retry"), 20).unwrap();
        harness.mark_running(&execution_id, 21).unwrap();
        harness
            .record_output(&execution_id, json!({"ok": true}))
            .unwrap();
        harness
            .validate_execution(
                &execution_id,
                ValidationPolicy::RequiredFields {
                    fields: vec!["missing".to_string()],
                },
                22,
            )
            .unwrap();
        let retry_id = harness.retry_node(&id("retry"), 23).unwrap();
        assert_ne!(execution_id, retry_id);
        assert_eq!(harness.attempt_history(&id("retry")).len(), 2);
    }

    #[test]
    fn recovery_and_partial_rerun_are_truthful_and_branch_limited() {
        let mut harness = harness();
        let source_id = complete(&mut harness, &id("source"), json!({"ok": true}), 30);
        let unrelated_id = complete(&mut harness, &id("unrelated"), json!({"ok": true}), 40);
        assert_eq!(harness.schedule().ready, vec![id("dependent")]);
        let affected = harness.rerun_from_node(&id("source"), 50).unwrap();
        assert_eq!(affected, vec![id("source"), id("dependent")]);
        assert_eq!(
            harness.execution(&source_id).unwrap().status,
            NodeExecutionStatus::Stale
        );
        assert_eq!(
            harness.execution(&unrelated_id).unwrap().status,
            NodeExecutionStatus::Succeeded
        );

        let active = harness.start_node(&id("source"), 60).unwrap();
        assert_eq!(harness.recover_after_restart(61), 1);
        assert_eq!(
            harness.execution(&active).unwrap().failure_code(),
            Some("execution_interrupted")
        );
    }
}
