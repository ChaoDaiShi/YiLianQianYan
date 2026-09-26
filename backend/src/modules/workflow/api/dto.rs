use super::*;

#[derive(serde::Deserialize)]
pub struct CreateWorkflowGraphRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: WorkflowGraphDefinition,
}

#[derive(serde::Deserialize)]
pub struct UpdateWorkflowGraphRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<WorkflowGraphDefinition>,
}

/// Query params for `GET /api/workflow-runs`.
#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowRunsParams {
    pub workflow_graph_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
