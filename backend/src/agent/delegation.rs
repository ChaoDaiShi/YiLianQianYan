//! Runtime context for bounded child execution, inherited through the existing tool gateway.
use crate::modules::task::resource_budget::SharedExecutionBudget;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct ExecutionScope {
    pub cancel: CancellationToken,
    pub budget: Option<Arc<SharedExecutionBudget>>,
    pub handoff_id: Option<String>,
}
tokio::task_local! { pub static EXECUTION_SCOPE: ExecutionScope; }
pub fn current_scope() -> Option<ExecutionScope> {
    EXECUTION_SCOPE.try_with(Clone::clone).ok()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelegationRequest {
    pub handoff_id: String,
    pub goal_id: String,
    pub goal_version: u32,
    pub role: String,
    pub task: String,
    pub allowed_tools: Vec<String>,
    pub input_evidence_ids: Vec<String>,
    pub required_artifact_names: Vec<String>,
}
pub struct DelegationContext {
    pub parent_cancel: CancellationToken,
    pub reserved_model_calls: u32,
    pub reserved_tokens: u64,
    pub timeout_ms: u64,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelegationResult {
    pub handoff_id: String,
    pub status: String,
    pub artifact_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub model_calls: u32,
    pub total_tokens: Option<u64>,
    pub elapsed_ms: u64,
}
