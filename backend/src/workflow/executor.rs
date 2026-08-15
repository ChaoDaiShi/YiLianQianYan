// ============================================================
// Workflow node executor — the execution boundary for a single node.
//
// Executors NEVER modify the [`super::run::WorkflowRun`] directly; they return
// an outcome and let the runner decide the resulting state transition. This
// keeps scheduling and security concerns in one place (the runner).
// ============================================================

use async_trait::async_trait;
use thiserror::Error;

use super::definition::WorkflowNodeDefinition;
use super::run::WorkflowRunError;
use crate::execution::ExecutionContext;

/// The result of executing a single node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeExecutionOutcome {
    Completed,
    WaitingApproval { approval_id: String },
    Failed,
}

/// An execution-layer error (safe, non-leaking).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkflowExecutionError {
    #[error("workflow node execution failed: {0}")]
    Execution(String),
    #[error("workflow run persistence failed: {0}")]
    Persistence(String),
    #[error("workflow run state error: {0}")]
    State(#[from] WorkflowRunError),
}

/// Executes a single workflow node.
#[async_trait]
pub trait WorkflowNodeExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &ExecutionContext,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError>;
}
