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
    let (nodes, edges) = if let Some(goal) = request.goal {
        if !request.nodes.is_empty() || !request.edges.is_empty() {
            return planning_error(
                StatusCode::BAD_REQUEST,
                "invalid_plan_request",
                "目标规划不能同时提交手工节点。".into(),
            );
        }
        if server.task_world.get_graph(&graph_id).is_some() {
            return runtime_error(TaskWorldRuntimeError::GraphAlreadyExists(
                graph_id.to_string(),
            ));
        }
        let workflows = match server.db.list_workflow_graphs() {
            Ok(workflows) => workflows
                .into_iter()
                .take(crate::task::MAX_PLANNER_CAPABILITIES)
                .map(|workflow| crate::task::PlannerCapability {
                    capability_id: format!("workflow.{}", workflow.id),
                    executor_type: "workflow".into(),
                    executor_ref: format!("workflow://{}", workflow.id),
                    name: crate::utils::text::truncate_chars(&workflow.name, 120),
                    description: crate::utils::text::truncate_chars(&workflow.description, 200),
                })
                .collect(),
            Err(_) => {
                return planning_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "planning_context_failed",
                    "无法读取已有工作流，未创建任务图；请重试。".into(),
                )
            }
        };
        let model = server.config.read().model.clone();
        let planner = crate::task::LlmTaskPlanner::new(&model, Arc::clone(&server.secret_resolver));
        match planner.plan_graph(&graph_id, &goal, workflows).await {
            Ok(graph) => (graph.nodes, graph.edges),
            Err(crate::task::TaskPlannerError::Llm(message)) => {
                return planning_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "planner_unavailable",
                    message,
                )
            }
            Err(error) => {
                return planning_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_plan",
                    error.to_string(),
                )
            }
        }
    } else {
        (request.nodes, request.edges)
    };
    match server
        .task_world
        .create_graph(graph_id, nodes, edges, now())
    {
        Ok(graph) => (StatusCode::CREATED, Json(json!({ "graph": graph }))).into_response(),
        Err(error) => runtime_error(error),
    }
}

fn planning_error(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({"error":code,"message":message}))).into_response()
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
        return Err(TaskWorldRuntimeError::Harness(TaskHarnessError::Resolver(
            crate::task::ExecutorResolutionError::MissingExecutorRef,
        )));
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
    resolver
        .resolve_node(node)
        .map_err(TaskHarnessError::from)?;
    Ok(resolver)
}

struct ExistingWorkflowProvider {
    server: Arc<AppServer>,
    cancel: tokio_util::sync::CancellationToken,
}

#[async_trait]
impl WorkflowExecutionProvider for ExistingWorkflowProvider {
    async fn execute(
        &self,
        workflow_id: &str,
        _context: &NodeContext,
    ) -> Result<Option<Value>, AdapterError> {
        super::workflow_runtime::execute_for_task_harness(
            Arc::clone(&self.server),
            workflow_id,
            self.cancel.clone(),
        )
        .await
        .map_err(AdapterError::Execution)
    }
}

async fn dispatch_execution(
    server: Arc<AppServer>,
    graph_id: TaskGraphId,
    execution_id: NodeExecutionId,
) -> Result<NodeExecution, TaskWorldRuntimeError> {
    let execution = server
        .task_world
        .find_execution(&execution_id)
        .map(|(_, execution)| execution)
        .ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
    if execution.status == crate::task::NodeExecutionStatus::Cancelled {
        return Ok(execution);
    }
    let ownership = server
        .task_world
        .claim_execution_dispatch(&graph_id, &execution_id)?;
    let cancel = ownership.cancellation_token();
    let plan = server.task_world.execution_plan(&graph_id, &execution_id)?;
    let registry = AdapterRegistry::new()
        .with_adapter(CommandExecutor::new(server.command_router.clone()))
        .with_adapter(WorkflowExecutor::new(Arc::new(ExistingWorkflowProvider {
            server: Arc::clone(&server),
            cancel,
        })));
    let result = registry.dispatch(&plan, &execution).await;
    // A cooperative cancellation may settle after an in-flight atomic call.
    // Preserve the authoritative cancelled row and discard its late output.
    if let Some((_, current)) = server.task_world.find_execution(&execution_id) {
        if current.status == crate::task::NodeExecutionStatus::Cancelled {
            return Ok(current);
        }
    }
    match result {
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
    if let Err(error) = executor_resolver(&server, &graph_id, &request.node_id) {
        return runtime_error(error);
    }
    match server.task_world.prepare_rerun_from_node(
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

#[cfg(test)]
mod core_path_tests {
    use super::*;
    use crate::task::{GraphRevision, TaskGraph, TaskWorldRuntime};
    use axum::body::to_bytes;

    fn server() -> Arc<AppServer> {
        let server = AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            crate::safety::ControlSession::new(uuid::Uuid::new_v4().to_string().repeat(2)).unwrap(),
        )
        .unwrap();
        {
            let mut config = server.config.write();
            config.model.api_key.clear();
            config.model.api_key_env.clear();
            config.model.api_key_ref = None;
        }
        Arc::new(server)
    }

    async fn body(response: Response) -> Value {
        serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn cancellation_and_pause_stop_real_workflow_after_inflight_model_returns() {
        for pause in [false, true] {
            let server = server();
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let provider = axum::Router::new().route("/chat/completions", axum::routing::post({
                let entered = entered.clone();
                let release = release.clone();
                move || { let entered = entered.clone(); let release = release.clone(); async move {
                    entered.notify_one();
                    release.notified().await;
                    Json(json!({"id":"blocking-model-test","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Local model result"}}]}))
                }}
            }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let provider_handle = tokio::spawn(async move {
                axum::serve(listener, provider).await.unwrap();
            });
            {
                let mut config = server.config.write();
                config.model.base_url = format!("http://{address}");
                config.model.api_key = "local-test-placeholder".into();
                config.model.invoke_timeout_ms = 5000;
            }
            server.db.create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: "cancellable".into(), name: "Cancellable".into(), description: String::new(), created_at: 1, updated_at: 1,
                definition: serde_json::from_value(json!({"schema_version":1,"entry_node_id":"model","nodes":[
                    {"id":"model","kind":"agent","config":{"type":"agent","prompt":"Return local test text"}},
                    {"id":"after","kind":"output","config":{"type":"output","template":null}}
                ],"edges":[{"from":"model","to":"after"}]})).unwrap(),
            }).unwrap();
            let id = TaskGraphId::new("cancel-graph").unwrap();
            let node_id = TaskNodeId::new("work").unwrap();
            server
                .task_world
                .create_graph(
                    id.clone(),
                    vec![TaskNode::new(
                        node_id.clone(),
                        TaskNodeKind::Work,
                        "Cancellable",
                        json!({"executor_ref":"workflow://cancellable"}),
                    )
                    .unwrap()],
                    vec![],
                    1,
                )
                .unwrap();
            let execution = server
                .task_world
                .start_execution_with_resolver(
                    &id,
                    &node_id,
                    1,
                    executor_resolver(&server, &id, &node_id).unwrap(),
                    2,
                )
                .unwrap();
            let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
            let running = tokio::spawn(dispatch_execution(
                server.clone(),
                id.clone(),
                execution.id.clone(),
            ));
            tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
                .await
                .unwrap();
            assert!(server
                .task_world
                .cancel_execution(&id, &execution.id, 99, 3)
                .is_err());
            if pause {
                server.task_world.pause_task(&id, 4).unwrap();
            } else {
                server
                    .task_world
                    .cancel_execution(&id, &execution.id, 1, 4)
                    .unwrap();
            }
            if pause {
                server.task_world.resume_task(&id, 5).unwrap();
            }
            let graph_before = server.task_world.get_graph(&id).unwrap();
            let history_before = serde_json::to_value(
                server
                    .task_world
                    .list_node_executions(&id, &node_id)
                    .unwrap(),
            )
            .unwrap();
            let mut changed = graph_before.nodes[0].clone();
            changed.title = "Must remain blocked until provider settles".into();
            assert!(server.task_world.update_node(&id, changed, 1, 6).is_err());
            assert!(server.task_world.delete_node(&id, &node_id, 1, 6).is_err());
            assert!(server
                .task_world
                .restore(&id, checkpoint.id.as_str(), 1, 6)
                .is_err());
            assert!(server
                .task_world
                .prepare_rerun_from_node(&id, &node_id, 1, 6)
                .is_err());
            assert!(server
                .task_world
                .start_execution_with_resolver(
                    &id,
                    &node_id,
                    1,
                    executor_resolver(&server, &id, &node_id).unwrap(),
                    6
                )
                .is_err());
            assert_eq!(server.task_world.get_graph(&id).unwrap(), graph_before);
            assert_eq!(
                serde_json::to_value(
                    server
                        .task_world
                        .list_node_executions(&id, &node_id)
                        .unwrap()
                )
                .unwrap(),
                history_before
            );
            release.notify_one();
            let settled = tokio::time::timeout(std::time::Duration::from_secs(5), running)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(settled.status, crate::task::NodeExecutionStatus::Cancelled);
            assert!(server
                .task_world
                .execution_cancellation_token(&execution.id)
                .is_none());
            let runs = server
                .db
                .list_workflow_runs(&crate::db::WorkflowRunQuery::default())
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(
                runs[0].run.status,
                crate::workflow::WorkflowRunStatus::Cancelled
            );
            assert_ne!(
                runs[0].run.node_states[1].status,
                crate::workflow::NodeRunStatus::Completed
            );
            assert_eq!(
                server
                    .task_world
                    .list_node_executions(&id, &node_id)
                    .unwrap()
                    .len(),
                1
            );
            server
                .task_world
                .prepare_rerun_from_node(&id, &node_id, 1, 7)
                .unwrap();
            let retry = server
                .task_world
                .start_execution_with_resolver(
                    &id,
                    &node_id,
                    1,
                    executor_resolver(&server, &id, &node_id).unwrap(),
                    8,
                )
                .unwrap();
            assert_eq!(retry.attempt, 2);
            server
                .task_world
                .cancel_execution(&id, &retry.id, 1, 9)
                .unwrap();
            provider_handle.abort();
        }
    }

    #[tokio::test]
    async fn review_restore_executes_restored_definitions_and_keeps_archived_attempts() {
        let server = server();
        for workflow in ["primary", "alternate"] {
            server.db.create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: workflow.into(), name: workflow.into(), description: String::new(), created_at: 1, updated_at: 1,
                definition: serde_json::from_value(json!({"schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":null}}],"edges":[]})).unwrap(),
            }).unwrap();
        }
        let id = TaskGraphId::new("restored-execution").unwrap();
        let a = TaskNodeId::new("a").unwrap();
        let b = TaskNodeId::new("b").unwrap();
        let c = TaskNodeId::new("c").unwrap();
        let nodes = [&a, &b].into_iter().map(|node_id| TaskNode::new(node_id.clone(), TaskNodeKind::Work, format!("Original {node_id}"), json!({"executor_ref":"workflow://primary", "instruction":format!("original-{node_id}")})).unwrap()).collect();
        server
            .task_world
            .create_graph(id.clone(), nodes, vec![], 1)
            .unwrap();
        let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
        let mut view = server.task_world.get_canvas_view(&id).unwrap();
        view.viewport.x = 42.0;
        let view_revision = server
            .task_world
            .save_canvas_view(&id, view.clone(), view.view_revision, 2)
            .unwrap()
            .view_revision;
        server
            .task_world
            .update_node(
                &id,
                TaskNode::new(
                    a.clone(),
                    TaskNodeKind::Work,
                    "Changed",
                    json!({"executor_ref":"workflow://alternate","instruction":"changed-A"}),
                )
                .unwrap(),
                1,
                3,
            )
            .unwrap();
        server.task_world.delete_node(&id, &b, 2, 4).unwrap();
        server
            .task_world
            .add_node(
                &id,
                TaskNode::new(
                    c.clone(),
                    TaskNodeKind::Work,
                    "Archived",
                    json!({"executor_ref":"workflow://alternate"}),
                )
                .unwrap(),
                3,
                5,
            )
            .unwrap();
        let archived = server
            .task_world
            .start_execution_with_resolver(
                &id,
                &c,
                4,
                executor_resolver(&server, &id, &c).unwrap(),
                6,
            )
            .unwrap();
        dispatch_execution(server.clone(), id.clone(), archived.id.clone())
            .await
            .unwrap();
        let restored = server
            .task_world
            .restore(&id, checkpoint.id.as_str(), 4, 7)
            .unwrap();
        assert_eq!(restored.revision.value(), 5);
        assert!(restored.node(&c).is_none());
        assert_eq!(
            server
                .task_world
                .get_canvas_view(&id)
                .unwrap()
                .view_revision,
            view_revision
        );
        for node in [&b, &a] {
            let attempt = server
                .task_world
                .start_execution_with_resolver(
                    &id,
                    node,
                    5,
                    executor_resolver(&server, &id, node).unwrap(),
                    8,
                )
                .unwrap();
            assert_eq!(attempt.context.instructions, format!("original-{node}"));
            assert_eq!(
                attempt.executor_ref.as_ref().unwrap().as_str(),
                "workflow://primary"
            );
            let done = dispatch_execution(server.clone(), id.clone(), attempt.id)
                .await
                .unwrap();
            assert_eq!(done.status, crate::task::NodeExecutionStatus::Succeeded);
            assert_eq!(done.output.unwrap()["workflow_graph_id"], "primary");
        }
        let reloaded =
            TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
        assert_eq!(reloaded.get_graph(&id).unwrap(), restored);
        assert_eq!(
            reloaded.list_node_executions(&id, &c).unwrap()[0].id,
            archived.id
        );
        assert_eq!(reloaded.list_node_executions(&id, &b).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_model_credentials_never_persist_a_partial_plan() {
        let server = server();
        let response = create_graph(
            State(server.clone()),
            Json(CreateGraphRequest {
                id: "missing-model".into(),
                goal: Some("Organize an editable plan".into()),
                nodes: vec![],
                edges: vec![],
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body(response).await["error"], "planner_unavailable");
        assert!(server.task_world.list_graphs().is_empty());
    }

    #[tokio::test]
    async fn configured_llm_path_validates_before_persisting() {
        let server = server();
        let provider = axum::Router::new().route("/chat/completions", axum::routing::post(|Json(request): Json<Value>| async move {
            assert!(request.get("tools").is_none());
            let input: Value = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            let content = if input["goal"] == "malformed" { "not a graph".into() } else {
                json!({"schema_version":1,"id":input["graph_id"],"revision":1,"nodes":[{
                    "id":"draft","kind":"work","title":"Editable task","input":{"instruction":"Organize source material","acceptance_criteria":[]},"retry_policy":{"max_attempts":1}
                }],"edges":[]}).to_string()
            };
            Json(json!({"id":"local-provider-test","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content}}]}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, provider).await.unwrap();
        });
        {
            let mut config = server.config.write();
            config.model.base_url = format!("http://{address}");
            config.model.api_key = "local-test-placeholder".into();
            config.model.invoke_timeout_ms = 2000;
        }
        for (id, goal, expected) in [
            ("bad", "malformed", StatusCode::UNPROCESSABLE_ENTITY),
            ("planned", "organize", StatusCode::CREATED),
        ] {
            let response = create_graph(
                State(server.clone()),
                Json(CreateGraphRequest {
                    id: id.into(),
                    goal: Some(goal.into()),
                    nodes: vec![],
                    edges: vec![],
                }),
            )
            .await;
            assert_eq!(response.status(), expected);
        }
        let reloaded =
            TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
        assert_eq!(reloaded.list_graphs().len(), 1);
        assert_eq!(reloaded.list_graphs()[0].id.as_str(), "planned");
        handle.abort();
    }

    #[tokio::test]
    async fn real_workflow_harness_edit_rerun_checkpoint_and_attempt_history() {
        let server = server();
        let definition = serde_json::from_value(json!({
            "schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":null}}],"edges":[]
        })).unwrap();
        server
            .db
            .create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: "local-output".into(),
                name: "Local output".into(),
                description: "Test of the actual WorkflowRunner".into(),
                definition,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        let id = TaskGraphId::new("harness-flow").unwrap();
        let node_id = TaskNodeId::new("run").unwrap();
        let node = TaskNode::new(node_id.clone(), TaskNodeKind::Work, "Run workflow", json!({"executor_ref":"workflow://local-output","instruction":"Execute configured workflow"})).unwrap();
        server
            .task_world
            .create_graph(
                id.clone(),
                vec![
                    node.clone(),
                    TaskNode::new(
                        TaskNodeId::new("editable").unwrap(),
                        TaskNodeKind::Work,
                        "Editable only",
                        json!({}),
                    )
                    .unwrap(),
                ],
                vec![],
                1,
            )
            .unwrap();
        let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
        let resolver = executor_resolver(&server, &id, &node_id).unwrap();
        let first = server
            .task_world
            .start_execution_with_resolver(&id, &node_id, 1, resolver, 3)
            .unwrap();
        let completed = dispatch_execution(server.clone(), id.clone(), first.id.clone())
            .await
            .unwrap();
        assert_eq!(
            completed.status,
            crate::task::NodeExecutionStatus::Succeeded
        );
        assert_eq!(
            completed.output.as_ref().unwrap()["workflow_graph_id"],
            "local-output"
        );
        let mut edited = node;
        edited.title = "Edited workflow task".into();
        let graph = server.task_world.update_node(&id, edited, 1, 4).unwrap();
        assert_eq!(graph.revision, GraphRevision::new(2).unwrap());
        assert!(server
            .task_world
            .rerun_from_node(&id, &node_id, 1, 5)
            .is_err());
        let response = rerun(
            State(server.clone()),
            Path(id.to_string()),
            Json(RerunRequest {
                expected_revision: 2,
                node_id: node_id.clone(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(server
            .task_world
            .list_node_executions(&id, &TaskNodeId::new("editable").unwrap())
            .unwrap()
            .is_empty());
        let second = server
            .task_world
            .start_execution_with_resolver(
                &id,
                &node_id,
                2,
                executor_resolver(&server, &id, &node_id).unwrap(),
                7,
            )
            .unwrap();
        assert_ne!(first.id, second.id);
        server
            .task_world
            .cancel_execution(&id, &second.id, 2, 8)
            .unwrap();
        let reloaded =
            TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
        let detail = reloaded.get_graph_detail(&id).unwrap();
        assert_eq!(detail.nodes[0].execution_history.len(), 2);
        assert_eq!(
            detail.nodes[0].execution_history[1].status,
            crate::task::NodeExecutionStatus::Cancelled
        );
        assert_eq!(
            detail.checkpoints[0].checkpoint_id,
            checkpoint.id.to_string()
        );
        let graph: TaskGraph = reloaded.get_graph(&id).unwrap();
        assert_eq!(graph.revision.value(), 2);
    }
}
