// ============================================================
// Workflow runner — deterministic sequential DAG scheduler.
//
// The runner drives a [`WorkflowRun`] to completion (or pause/failure) by
// repeatedly promoting eligible nodes, executing the first ready node in
// definition order through a [`WorkflowNodeExecutor`], and checkpointing after
// every state transition. Execution is deliberately sequential and
// deterministic in this first version — no parallelism.
// ============================================================

use tokio_util::sync::CancellationToken;

use super::definition::WorkflowNodeId;
use super::executor::{NodeExecutionOutcome, WorkflowExecutionError, WorkflowNodeExecutor};
use super::run::{NodeRunStatus, WorkflowRun};

pub struct WorkflowRunner<E> {
    executor: E,
}

impl<E: WorkflowNodeExecutor> WorkflowRunner<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    /// Run the workflow until it completes, fails, pauses for approval, or is
    /// cancelled. The outcome is reflected in `run.status`.
    pub async fn run(
        &self,
        run: &mut WorkflowRun,
        cancel: &CancellationToken,
        mut persist: impl FnMut(&WorkflowRun) -> Result<(), String>,
    ) -> Result<(), WorkflowExecutionError> {
        loop {
            if cancel.is_cancelled() {
                run.cancel(now_ms());
                let _ = persist(run);
                return Ok(());
            }

            // Promote every eligible pending node to Ready (definition order).
            let eligible = run.ready_nodes();
            for node_id in &eligible {
                run.transition_node(node_id, NodeRunStatus::Ready, now_ms())?;
            }
            if !eligible.is_empty() {
                persist(run).map_err(WorkflowExecutionError::Persistence)?;
            }

            // Sequential execution: run the first Ready node in definition order.
            let Some(node_id) = ready_to_run(run).into_iter().next() else {
                break;
            };

            run.transition_node(&node_id, NodeRunStatus::Running, now_ms())?;
            persist(run).map_err(WorkflowExecutionError::Persistence)?;

            let node = run
                .definition
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .expect("node id known to exist");
            let outcome = self.executor.execute(&run.execution_context, node).await;

            match outcome {
                Ok(NodeExecutionOutcome::Completed) => {
                    run.transition_node(&node_id, NodeRunStatus::Completed, now_ms())?;
                    persist(run).map_err(WorkflowExecutionError::Persistence)?;
                }
                Ok(NodeExecutionOutcome::WaitingApproval { .. }) => {
                    run.transition_node(&node_id, NodeRunStatus::WaitingApproval, now_ms())?;
                    persist(run).map_err(WorkflowExecutionError::Persistence)?;
                    break;
                }
                Ok(NodeExecutionOutcome::Failed) => {
                    run.transition_node(&node_id, NodeRunStatus::Failed, now_ms())?;
                    persist(run).map_err(WorkflowExecutionError::Persistence)?;
                    break;
                }
                Err(error) => {
                    run.transition_node(&node_id, NodeRunStatus::Failed, now_ms())?;
                    if let Some(state) = run.node_mut(&node_id) {
                        state.error = Some(error.to_string());
                    }
                    persist(run).map_err(WorkflowExecutionError::Persistence)?;
                    break;
                }
            }
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Nodes currently in the `Ready` state, in definition order.
fn ready_to_run(run: &WorkflowRun) -> Vec<WorkflowNodeId> {
    run.node_states
        .iter()
        .filter(|s| s.status == NodeRunStatus::Ready)
        .map(|s| s.node_id.clone())
        .collect()
}
