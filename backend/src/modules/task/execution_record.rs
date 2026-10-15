//! Read-only execution dossiers retain attempts instead of flattening to the newest status.
use super::NodeExecution;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionRecordItem {
    pub graph_id: String,
    pub graph_revision: Option<u64>,
    pub node_id: String,
    pub execution_id: String,
    pub attempt: u32,
    pub status: super::NodeExecutionStatus,
    pub approval_ref: Option<String>,
    pub artifact_ids: Vec<String>,
    pub report_ids: Vec<String>,
    pub failure_code: Option<String>,
}
pub fn project_execution_record(
    executions: &[NodeExecution],
    artifacts: &std::collections::HashMap<String, Vec<String>>,
    reports: &std::collections::HashMap<String, Vec<String>>,
    revisions: &std::collections::HashMap<String, u64>,
) -> Vec<ExecutionRecordItem> {
    let mut source: Vec<_> = executions.iter().collect();
    source.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then(a.id.as_str().cmp(b.id.as_str()))
    });
    source
        .into_iter()
        .map(|e| ExecutionRecordItem {
            graph_id: e.graph_id.to_string(),
            graph_revision: revisions.get(e.id.as_str()).copied(),
            node_id: e.node_id.to_string(),
            execution_id: e.id.to_string(),
            attempt: e.attempt,
            status: e.status,
            approval_ref: e.approval_ref.clone(),
            artifact_ids: artifacts.get(e.id.as_str()).cloned().unwrap_or_default(),
            report_ids: reports.get(e.id.as_str()).cloned().unwrap_or_default(),
            failure_code: e.failure_code.clone(),
        })
        .collect()
}
