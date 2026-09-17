//! Controlled-session REST surface for the v1 Task World graph state.
//!
//! Graph endpoints mutate only product state. The controlled command endpoint
//! submits one validated request through the shared CommandRouter; it never
//! performs native desktop work in v1.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::AppServer;
use crate::task::validation::ValidationPolicy;
use crate::task::{
    execution_summary, AdapterError, AdapterRegistry, CanvasNodeLayout, CanvasView, CanvasViewport,
    CommandExecutor, ExecutorDispatch, ExecutorKind, ExecutorResolver, NodeContext, NodeExecution,
    NodeExecutionId, ResolvedExecutionPlan, RetryPolicy, TaskEdge, TaskGraphId, TaskHarnessError,
    TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntimeError, WorkflowExecutionProvider,
    WorkflowExecutor, CANVAS_VIEW_SCHEMA_VERSION,
};

#[derive(Debug, Deserialize)]
pub struct CreateGraphRequest {
    pub id: String,
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
}

fn default_canvas_schema_version() -> u32 {
    CANVAS_VIEW_SCHEMA_VERSION
}

pub async fn list_graphs(State(server): State<Arc<AppServer>>) -> Response {
    let graphs = server.task_world.list_graphs();
    (StatusCode::OK, Json(json!({ "graphs": graphs }))).into_response()
}

pub async fn create_graph(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CreateGraphRequest>,
) -> Response {
    let graph_id = match TaskGraphId::new(request.id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server
        .task_world
        .create_graph(graph_id, request.nodes, request.edges, now())
    {
        Ok(graph) => (StatusCode::CREATED, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn get_graph(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.get_graph(&graph_id) {
        Some(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        None => runtime_error(TaskWorldRuntimeError::GraphNotFound(graph_id.to_string())),
    }
}

pub async fn get_graph_detail(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    if let Err(error) = server.task_world.reconcile_active_command_requests(
        &graph_id,
        &server.command_router,
        now(),
    ) {
        return runtime_error(error);
    }
    match server.task_world.get_graph_detail(&graph_id) {
        Ok(detail) => (StatusCode::OK, Json(json!({ "detail": detail }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn get_canvas_view(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.get_canvas_view(&graph_id) {
        Ok(view) => (StatusCode::OK, Json(json!({ "view": view }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn put_canvas_view(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<UpdateCanvasViewRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let updated_at = now();
    let view = CanvasView {
        schema_version: request.schema_version,
        graph_id: graph_id.clone(),
        view_revision: request
            .view_revision
            .unwrap_or(request.expected_view_revision),
        graph_revision_seen: request.graph_revision_seen,
        viewport: request.viewport,
        node_layouts: request.node_layouts,
        selection: request.selection,
        updated_at,
    };
    match server.task_world.save_canvas_view(
        &graph_id,
        view,
        request.expected_view_revision,
        updated_at,
    ) {
        Ok(view) => (StatusCode::OK, Json(json!({ "view": view }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn add_node(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<AddNodeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server
        .task_world
        .add_node(&graph_id, request.node, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn update_node(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<UpdateNodeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let node = match TaskNode::new(node_id, request.kind, request.title, request.input) {
        Ok(node) => node.with_retry_policy(request.retry_policy),
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server
        .task_world
        .update_node(&graph_id, node, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn delete_node(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server
        .task_world
        .delete_node(&graph_id, &node_id, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn add_edge(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<AddEdgeRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let from = match TaskNodeId::new(request.from) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let to = match TaskNodeId::new(request.to) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.add_edge(
        &graph_id,
        TaskEdge::new(from, to),
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn delete_edge(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, from, to)): Path<(String, String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let from = match TaskNodeId::new(from) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let to = match TaskNodeId::new(to) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.delete_edge(
        &graph_id,
        &TaskEdge::new(from, to),
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn start_node(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server
        .task_world
        .start_node(&graph_id, &node_id, request.expected_revision, now())
    {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn start_execution(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    let resolver = match executor_resolver(&server, &graph_id, &node_id) {
        Ok(resolver) => resolver,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.start_execution_with_resolver(
        &graph_id,
        &node_id,
        request.expected_revision,
        resolver,
        now(),
    ) {
        Ok(execution) => (
            {
                if execution.executor_ref.is_some() {
                    let server = Arc::clone(&server);
                    let graph_id = graph_id.clone();
                    let execution_id = execution.id.clone();
                    tokio::spawn(async move {
                        if let Err(error) =
                            dispatch_execution(server, graph_id.clone(), execution_id.clone()).await
                        {
                            tracing::error!(
                                graph_id = %graph_id,
                                execution_id = %execution_id,
                                error = %error,
                                "task harness dispatch failed"
                            );
                        }
                    });
                }
                StatusCode::CREATED
            },
            Json(json!({ "execution": execution_summary(&execution) })),
        )
            .into_response(),
        Err(error) => runtime_error(error),
    }
}

fn executor_resolver(
    server: &AppServer,
    graph_id: &TaskGraphId,
    node_id: &TaskNodeId,
) -> Result<ExecutorResolver, TaskWorldRuntimeError> {
    let graph = server
        .task_world
        .get_graph(graph_id)
        .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
    let node = graph.node(node_id).ok_or_else(|| {
        TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownNode(node_id.clone()))
    })?;
    let Some(reference) = node.input.get("executor_ref").and_then(Value::as_str) else {
        return Ok(ExecutorResolver::new());
    };
    let parsed = crate::task::ExecutorRef::new(reference).map_err(|error| {
        TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(error.to_string()))
    })?;
    let target = parsed
        .as_str()
        .split_once("://")
        .map(|(_, target)| target)
        .unwrap_or_default();
    let mut resolver = ExecutorResolver::new();
    match parsed.scheme() {
        "workflow"
            if server
                .db
                .get_workflow_graph(target)
                .map_err(|error| TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(error)))?
                .is_some() =>
        {
            resolver.register_workflow(target);
        }
        _ => {}
    }
    Ok(resolver)
}

struct ExistingWorkflowProvider {
    server: Arc<AppServer>,
}

#[async_trait]
impl WorkflowExecutionProvider for ExistingWorkflowProvider {
    async fn execute(
        &self,
        workflow_id: &str,
        _context: &NodeContext,
    ) -> Result<Option<Value>, AdapterError> {
        super::workflow_runtime::execute_for_task_harness(Arc::clone(&self.server), workflow_id)
            .await
            .map_err(AdapterError::Execution)
    }
}

async fn dispatch_execution(
    server: Arc<AppServer>,
    graph_id: TaskGraphId,
    execution_id: NodeExecutionId,
) -> Result<NodeExecution, TaskWorldRuntimeError> {
    let plan = server.task_world.execution_plan(&graph_id, &execution_id)?;
    let execution = server
        .task_world
        .find_execution(&execution_id)
        .map(|(_, execution)| execution)
        .ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
    let registry = AdapterRegistry::new()
        .with_adapter(CommandExecutor::new(server.command_router.clone()))
        .with_adapter(WorkflowExecutor::new(Arc::new(ExistingWorkflowProvider {
            server: Arc::clone(&server),
        })));
    match registry.dispatch(&plan, &execution).await {
        Ok(ExecutorDispatch::Completed {
            output: Some(output),
        }) => {
            let policy = validation_policy(&plan)?;
            server
                .task_world
                .complete_execution(&graph_id, &execution_id, output, policy, now())
        }
        Ok(ExecutorDispatch::Completed { output: None }) => server.task_world.fail_execution(
            &graph_id,
            &execution_id,
            "missing_output",
            "executor completed without a result",
            now(),
        ),
        Ok(ExecutorDispatch::WaitingApproval { approval_ref }) => server
            .task_world
            .mark_execution_waiting_approval(&graph_id, &execution_id, &approval_ref, now()),
        Ok(ExecutorDispatch::Failed { code, error }) => {
            server
                .task_world
                .fail_execution(&graph_id, &execution_id, &code, &error, now())
        }
        Err(error) => server.task_world.fail_execution(
            &graph_id,
            &execution_id,
            "adapter_error",
            &error.to_string(),
            now(),
        ),
    }
}

fn validation_policy(
    plan: &ResolvedExecutionPlan,
) -> Result<ValidationPolicy, TaskWorldRuntimeError> {
    match plan.kind {
        ExecutorKind::Workflow => Ok(ValidationPolicy::WorkflowResult),
        ExecutorKind::Command => {
            let binding = plan.command_binding.as_ref().ok_or_else(|| {
                TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(
                    "command execution is missing a validated binding".to_string(),
                ))
            })?;
            Ok(ValidationPolicy::CommandVerification {
                command: binding.command.clone(),
                app_id: binding.args.app_id.clone(),
            })
        }
        _ => Ok(ValidationPolicy::StructuredResult),
    }
}

pub async fn list_executions(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server
        .task_world
        .list_node_execution_summaries(&graph_id, &node_id)
    {
        Ok(executions) => {
            (StatusCode::OK, Json(json!({ "executions": executions }))).into_response()
        }
        Err(error) => runtime_error(error),
    }
}

pub async fn cancel_execution(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, execution_id)): Path<(String, String)>,
    Json(request): Json<CancelExecutionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let execution_id = match NodeExecutionId::new(execution_id) {
        Ok(execution_id) => execution_id,
        Err(error) => {
            return runtime_error(TaskWorldRuntimeError::Harness(TaskHarnessError::Execution(
                error,
            )))
        }
    };
    match server.task_world.cancel_execution(
        &graph_id,
        &execution_id,
        request.expected_revision,
        now(),
    ) {
        Ok(execution) => (
            StatusCode::OK,
            Json(json!({ "execution": execution_summary(&execution) })),
        )
            .into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn rerun(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<RerunRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.rerun_from_node(
        &graph_id,
        &request.node_id,
        request.expected_revision,
        now(),
    ) {
        Ok(affected_nodes) => (
            StatusCode::OK,
            Json(json!({
                "graph_id": graph_id,
                "node_id": request.node_id,
                "affected_nodes": affected_nodes,
            })),
        )
            .into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn execute_command(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.execute_node(
        &graph_id,
        &node_id,
        request.expected_revision,
        &server.command_router,
        now(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn cancel_command(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    Json(request): Json<CancelCommandRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.cancel_command(
        &graph_id,
        &node_id,
        request.expected_revision,
        &request.request_id,
        &server.command_router,
        now(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn checkpoint(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<ExpectedRevisionRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server
        .task_world
        .checkpoint(&graph_id, request.expected_revision, now())
    {
        Ok(checkpoint) => (
            StatusCode::CREATED,
            Json(json!({ "checkpoint": checkpoint })),
        )
            .into_response(),
        Err(error) => runtime_error(error),
    }
}

pub async fn restore(
    State(server): State<Arc<AppServer>>,
    Path(graph_id): Path<String>,
    Json(request): Json<RestoreRequest>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    match server.task_world.restore(
        &graph_id,
        &request.checkpoint_id,
        request.expected_revision,
        now(),
    ) {
        Ok(graph) => (StatusCode::OK, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

fn parse_graph_id(raw: String) -> Result<TaskGraphId, TaskWorldRuntimeError> {
    TaskGraphId::new(raw).map_err(TaskWorldRuntimeError::Graph)
}

fn runtime_error(error: TaskWorldRuntimeError) -> Response {
    let (status, code) = match &error {
        TaskWorldRuntimeError::GraphNotFound(_) | TaskWorldRuntimeError::CheckpointNotFound(_) => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        TaskWorldRuntimeError::GraphAlreadyExists(_) => (StatusCode::CONFLICT, "already_exists"),
        TaskWorldRuntimeError::StaleRevision { .. } => (StatusCode::CONFLICT, "stale_revision"),
        TaskWorldRuntimeError::StaleCanvasViewRevision { .. } => {
            (StatusCode::CONFLICT, "stale_view_revision")
        }
        TaskWorldRuntimeError::CommandExecutionActive { .. }
        | TaskWorldRuntimeError::CommandExecutionNotActive { .. } => {
            (StatusCode::CONFLICT, "command_execution_conflict")
        }
        TaskWorldRuntimeError::CommandRequiresExecution(_) => {
            (StatusCode::BAD_REQUEST, "command_execution_required")
        }
        TaskWorldRuntimeError::InvalidCommandResult(_) => {
            (StatusCode::BAD_REQUEST, "invalid_command_result")
        }
        TaskWorldRuntimeError::ExecutionPaused(_) => (StatusCode::CONFLICT, "task_paused"),
        TaskWorldRuntimeError::ExecutionCancelled(_) => (StatusCode::CONFLICT, "task_cancelled"),
        TaskWorldRuntimeError::ExecutionControlGenerationOverflow => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::Persistence(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::ExecutionPersistence(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_execution_error")
        }
        TaskWorldRuntimeError::Harness(_) => (StatusCode::BAD_REQUEST, "task_execution_error"),
        TaskWorldRuntimeError::CanvasViewRevisionOverflow => {
            (StatusCode::INTERNAL_SERVER_ERROR, "task_world_error")
        }
        TaskWorldRuntimeError::Graph(_)
        | TaskWorldRuntimeError::Supervisor(_)
        | TaskWorldRuntimeError::Canvas(_) => (StatusCode::BAD_REQUEST, "task_world_error"),
    };
    (
        status,
        Json(json!({
            "error": code,
            "message": error.to_string(),
        })),
    )
        .into_response()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
