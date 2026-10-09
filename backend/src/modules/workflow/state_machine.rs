// ============================================================
// Workflow state machine — deterministic transition + readiness logic.
//
// Pure functions over run state: which node transitions are legal, which
// pending nodes are eligible to run, and what overall run status the node
// states imply. No execution, no scheduler, no persistence.
// ============================================================

use std::collections::HashMap;

use super::definition::{WorkflowGraphDefinition, WorkflowNodeId};
use super::run::{NodeRunState, NodeRunStatus, WorkflowRunStatus};

/// Whether `from` may transition to `to`.
///
/// Terminal states (`Completed`, `Failed`, `Skipped`, `Cancelled`) are
/// immutable: they have no outgoing transitions. Any non-terminal state may
/// transition to `Cancelled`.
pub fn is_allowed_transition(from: NodeRunStatus, to: NodeRunStatus) -> bool {
    use NodeRunStatus::*;
    if to == Cancelled {
        return matches!(from, Pending | Ready | Running | WaitingApproval);
    }
    match from {
        Pending => to == Ready,
        Ready => to == Running,
        Running => matches!(to, Completed | Failed | WaitingApproval),
        WaitingApproval => to == Running,
        Completed | Failed | Skipped | Cancelled => false,
    }
}

/// Compute the set of node ids that are currently eligible to run.
///
/// A `Pending` node is eligible when every incoming dependency is `Completed`.
/// The entry node is excluded here because it is initialized to `Ready`, not
/// `Pending`. Results are returned in definition order (deterministic).
pub fn ready_nodes(
    definition: &WorkflowGraphDefinition,
    node_states: &[NodeRunState],
) -> Vec<WorkflowNodeId> {
    let status_of: HashMap<&str, NodeRunStatus> = node_states
        .iter()
        .map(|s| (s.node_id.as_str(), s.status))
        .collect();

    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in &definition.nodes {
        incoming.entry(node.id.as_str()).or_default();
    }
    for edge in &definition.edges {
        incoming
            .entry(edge.to.as_str())
            .or_default()
            .push(edge.from.as_str());
    }

    node_states
        .iter()
        .filter(|s| s.status == NodeRunStatus::Pending)
        .filter(|s| {
            incoming
                .get(s.node_id.as_str())
                .map(|deps| {
                    deps.iter()
                        .all(|dep| status_of.get(dep).copied() == Some(NodeRunStatus::Completed))
                })
                .unwrap_or(true)
        })
        .map(|s| s.node_id.clone())
        .collect()
}

/// Derive the overall run status from the node states.
pub fn derive_run_status(node_states: &[NodeRunState]) -> WorkflowRunStatus {
    if node_states.is_empty() {
        return WorkflowRunStatus::Created;
    }

    let any = |status: NodeRunStatus| node_states.iter().any(|s| s.status == status);

    if any(NodeRunStatus::Failed) {
        WorkflowRunStatus::Failed
    } else if any(NodeRunStatus::Cancelled) {
        WorkflowRunStatus::Cancelled
    } else if any(NodeRunStatus::WaitingApproval) {
        WorkflowRunStatus::WaitingApproval
    } else if any(NodeRunStatus::Running) || any(NodeRunStatus::Ready) {
        WorkflowRunStatus::Running
    } else if node_states.iter().all(|s| s.status.is_terminal()) {
        WorkflowRunStatus::Completed
    } else {
        // Pending nodes remain but nothing is active yet — still in progress.
        WorkflowRunStatus::Running
    }
}
