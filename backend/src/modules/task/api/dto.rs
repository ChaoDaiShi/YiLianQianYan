use super::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGraphRequest {
    pub id: String,
    pub goal: Option<String>,
    #[serde(default)]
    pub nodes: Vec<TaskNode>,
    #[serde(default)]
    pub edges: Vec<TaskEdge>,
}

#[derive(Debug, Deserialize)]
pub struct AddNodeRequest {
    pub expected_revision: u64,
    pub node: TaskNode,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNodeRequest {
    pub expected_revision: u64,
    pub kind: TaskNodeKind,
    pub title: String,
    pub input: Value,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
}

#[derive(Debug, Deserialize)]
pub struct AddEdgeRequest {
    pub expected_revision: u64,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ExpectedRevisionRequest {
    pub expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewGraphRequest {
    pub expected_revision: u64,
}

#[derive(Debug, Deserialize)]
pub struct CancelCommandRequest {
    pub expected_revision: u64,
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CancelExecutionRequest {
    pub expected_revision: u64,
}

#[derive(Debug, Deserialize)]
pub struct RerunRequest {
    pub expected_revision: u64,
    pub node_id: TaskNodeId,
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    pub expected_revision: u64,
    pub checkpoint_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCanvasViewRequest {
    pub expected_view_revision: u64,
    #[serde(default = "default_canvas_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub view_revision: Option<u64>,
    pub graph_revision_seen: u64,
    pub viewport: CanvasViewport,
    #[serde(default)]
    pub node_layouts: Vec<CanvasNodeLayout>,
    #[serde(default)]
    pub selection: Vec<TaskNodeId>,
    #[serde(default)]
    pub groups: Vec<CanvasGroup>,
}

pub(crate) fn default_canvas_schema_version() -> u32 {
    CANVAS_VIEW_SCHEMA_VERSION
}
