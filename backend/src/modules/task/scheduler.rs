//! Deterministic, bounded TaskGraph scheduling decisions.
//!
//! The scheduler is pure. It computes a ready set and stale/blocked
//! consequences; the Task Harness owns persistence and adapters, while
//! `TaskSupervisor` remains a graph-state owner and does not spawn work.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::execution::{NodeExecutionStatus, ValidationStatus};
use super::{TaskGraph, TaskNodeId};

pub const DEFAULT_MAX_PARALLEL_NODES: usize = 3;
pub const MAX_PARALLEL_NODES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerLimits {
    pub max_parallel_nodes: usize,
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            max_parallel_nodes: DEFAULT_MAX_PARALLEL_NODES,
        }
    }
}

impl SchedulerLimits {
    pub fn new(max_parallel_nodes: usize) -> Result<Self, SchedulerError> {
        if max_parallel_nodes == 0 || max_parallel_nodes > MAX_PARALLEL_NODES {
            return Err(SchedulerError::InvalidParallelLimit(max_parallel_nodes));
        }
        Ok(Self { max_parallel_nodes })
    }
}

/// Latest attempt projection consumed by the pure scheduler. A succeeded
/// status is not enough by itself: validation must be accepted as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerNodeState {
    pub status: NodeExecutionStatus,
    pub validation: Option<ValidationStatus>,
}

impl SchedulerNodeState {
    pub fn new(status: NodeExecutionStatus) -> Self {
        Self {
            status,
            validation: None,
        }
    }

    pub fn verified_success() -> Self {
        Self {
            status: NodeExecutionStatus::Succeeded,
            validation: Some(ValidationStatus::Accepted),
        }
    }

    pub fn is_verified_success(self) -> bool {
        self.status == NodeExecutionStatus::Succeeded
            && self.validation == Some(ValidationStatus::Accepted)
    }
}

impl From<NodeExecutionStatus> for SchedulerNodeState {
    fn from(status: NodeExecutionStatus) -> Self {
        Self::new(status)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleDecision {
    pub ready: Vec<TaskNodeId>,
    pub blocked: Vec<TaskNodeId>,
    pub active_count: usize,
    pub available_slots: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchedulerError {
    #[error("max_parallel_nodes must be between 1 and {MAX_PARALLEL_NODES}, got {0}")]
    InvalidParallelLimit(usize),
    #[error("task graph is invalid: {0}")]
    InvalidGraph(String),
}

#[derive(Debug, Clone)]
pub struct DeterministicScheduler {
    graph: TaskGraph,
    limits: SchedulerLimits,
}

pub type TaskScheduler = DeterministicScheduler;

impl DeterministicScheduler {
    pub fn new(graph: TaskGraph) -> Result<Self, SchedulerError> {
        Self::with_limits(graph, SchedulerLimits::default())
    }

    pub fn with_limits(graph: TaskGraph, limits: SchedulerLimits) -> Result<Self, SchedulerError> {
        graph
            .validate()
            .map_err(|error| SchedulerError::InvalidGraph(error.to_string()))?;
        Ok(Self { graph, limits })
    }

    pub fn graph(&self) -> &TaskGraph {
        &self.graph
    }

    pub fn limits(&self) -> SchedulerLimits {
        self.limits
    }

    /// Compute a bounded ready set using raw statuses. This compatibility
    /// helper treats `succeeded` as verified; callers with validation evidence
    /// should use [`Self::schedule`].
    pub fn ready_nodes(
        &self,
        statuses: &HashMap<TaskNodeId, NodeExecutionStatus>,
    ) -> Vec<TaskNodeId> {
        let states = statuses
            .iter()
            .map(|(node_id, status)| {
                let validation = (*status == NodeExecutionStatus::Succeeded)
                    .then_some(ValidationStatus::Accepted);
                (
                    node_id.clone(),
                    SchedulerNodeState {
                        status: *status,
                        validation,
                    },
                )
            })
            .collect();
        self.schedule(&states).ready
    }

    pub fn schedule(&self, states: &HashMap<TaskNodeId, SchedulerNodeState>) -> ScheduleDecision {
        let active_count = states
            .values()
            .filter(|state| state.status.is_active())
            .count();
        let available_slots = self.limits.max_parallel_nodes.saturating_sub(active_count);
        let mut ready = Vec::new();
        let mut blocked = Vec::new();

        for node in &self.graph.nodes {
            let state = states
                .get(&node.id)
                .copied()
                .unwrap_or_else(|| SchedulerNodeState::new(NodeExecutionStatus::Pending));
            if !matches!(
                state.status,
                NodeExecutionStatus::Pending
                    | NodeExecutionStatus::Ready
                    | NodeExecutionStatus::Stale
            ) {
                continue;
            }
            let dependencies = self.graph.dependencies(&node.id);
            if dependencies.iter().any(|dependency| {
                states
                    .get(dependency)
                    .is_some_and(|state| is_blocking(state.status))
            }) {
                blocked.push(node.id.clone());
                continue;
            }
            if dependencies.iter().all(|dependency| {
                states
                    .get(dependency)
                    .is_some_and(|state| state.is_verified_success())
            }) {
                ready.push(node.id.clone());
            }
        }

        ready.truncate(available_slots);
        ScheduleDecision {
            ready,
            blocked,
            active_count,
            available_slots,
        }
    }

    /// Check whether one explicitly selected node can start now. Unlike the
    /// bounded automatic ready list, this does not reject a later graph node
    /// merely because earlier independent nodes occupy the first ready slots.
    pub fn can_start_node(
        &self,
        states: &HashMap<TaskNodeId, SchedulerNodeState>,
        node_id: &TaskNodeId,
    ) -> bool {
        let active_count = states
            .values()
            .filter(|state| state.status.is_active())
            .count();
        if active_count >= self.limits.max_parallel_nodes {
            return false;
        }
        let Some(node) = self.graph.node(node_id) else {
            return false;
        };
        let state = states
            .get(node_id)
            .copied()
            .unwrap_or_else(|| SchedulerNodeState::new(NodeExecutionStatus::Pending));
        if !matches!(
            state.status,
            NodeExecutionStatus::Pending | NodeExecutionStatus::Ready | NodeExecutionStatus::Stale
        ) {
            return false;
        }
        let dependencies = self.graph.dependencies(&node.id);
        !dependencies.iter().any(|dependency| {
            states
                .get(dependency)
                .is_some_and(|state| is_blocking(state.status))
        }) && dependencies.iter().all(|dependency| {
            states
                .get(dependency)
                .is_some_and(|state| state.is_verified_success())
        })
    }

    /// Return the selected roots and every transitive dependent, preserving
    /// graph order. Unrelated branches are never included.
    pub fn affected_branch(&self, roots: &[TaskNodeId]) -> Vec<TaskNodeId> {
        let mut affected = HashSet::new();
        let mut pending = roots.to_vec();
        while let Some(node_id) = pending.pop() {
            if !affected.insert(node_id.clone()) {
                continue;
            }
            pending.extend(self.graph.dependents(&node_id));
        }
        self.graph
            .nodes
            .iter()
            .filter(|node| affected.contains(&node.id))
            .map(|node| node.id.clone())
            .collect()
    }

    pub fn stale_branch(&self, roots: &[TaskNodeId]) -> Vec<TaskNodeId> {
        self.affected_branch(roots)
    }

    pub fn mark_stale(
        &self,
        states: &mut HashMap<TaskNodeId, SchedulerNodeState>,
        roots: &[TaskNodeId],
    ) -> Vec<TaskNodeId> {
        let affected = self.affected_branch(roots);
        for node_id in &affected {
            if let Some(state) = states.get_mut(node_id) {
                state.status = NodeExecutionStatus::Stale;
                state.validation = None;
            }
        }
        affected
    }
}

fn is_blocking(status: NodeExecutionStatus) -> bool {
    matches!(
        status,
        NodeExecutionStatus::Failed
            | NodeExecutionStatus::Blocked
            | NodeExecutionStatus::Cancelled
            | NodeExecutionStatus::Stale
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{GraphRevision, TaskEdge, TaskGraphId, TaskNode, TaskNodeKind};
    use serde_json::json;

    fn graph(edges: &[(&str, &str)], ids: &[&str]) -> TaskGraph {
        let nodes = ids
            .iter()
            .map(|id| {
                TaskNode::new(
                    TaskNodeId::new(*id).unwrap(),
                    TaskNodeKind::Work,
                    *id,
                    json!({}),
                )
                .unwrap()
            })
            .collect();
        let edges = edges
            .iter()
            .map(|(from, to)| {
                TaskEdge::new(
                    TaskNodeId::new(*from).unwrap(),
                    TaskNodeId::new(*to).unwrap(),
                )
            })
            .collect();
        TaskGraph::new(
            TaskGraphId::new("scheduler-tests").unwrap(),
            GraphRevision::initial(),
            nodes,
            edges,
        )
        .unwrap()
    }

    #[test]
    fn chain_waits_for_verified_predecessor_and_is_deterministic() {
        let scheduler =
            DeterministicScheduler::new(graph(&[("a", "b"), ("b", "c")], &["c", "b", "a"]))
                .unwrap();
        let mut states = HashMap::new();
        let first = scheduler.schedule(&states);
        assert_eq!(first.ready, vec![TaskNodeId::new("a").unwrap()]);
        states.insert(
            TaskNodeId::new("a").unwrap(),
            SchedulerNodeState::verified_success(),
        );
        assert_eq!(
            scheduler.schedule(&states).ready,
            vec![TaskNodeId::new("b").unwrap()]
        );
    }

    #[test]
    fn failed_dependency_blocks_descendants_and_fork_is_capped_at_three() {
        let scheduler = DeterministicScheduler::new(graph(
            &[("a", "b"), ("a", "c"), ("a", "d"), ("a", "e")],
            &["a", "b", "c", "d", "e"],
        ))
        .unwrap();
        let mut states = HashMap::new();
        states.insert(
            TaskNodeId::new("a").unwrap(),
            SchedulerNodeState::verified_success(),
        );
        let decision = scheduler.schedule(&states);
        assert_eq!(decision.ready.len(), 3);
        assert_eq!(decision.ready[0], TaskNodeId::new("b").unwrap());

        states.insert(
            TaskNodeId::new("a").unwrap(),
            SchedulerNodeState::new(NodeExecutionStatus::Failed),
        );
        let decision = scheduler.schedule(&states);
        assert_eq!(decision.blocked.len(), 4);
    }

    #[test]
    fn affected_branch_preserves_unrelated_nodes() {
        let scheduler = DeterministicScheduler::new(graph(
            &[("a", "b"), ("b", "c"), ("x", "y")],
            &["a", "b", "c", "x", "y"],
        ))
        .unwrap();
        assert_eq!(
            scheduler.affected_branch(&[TaskNodeId::new("b").unwrap()]),
            vec![TaskNodeId::new("b").unwrap(), TaskNodeId::new("c").unwrap()]
        );
    }
}
