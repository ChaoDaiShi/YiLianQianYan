use super::*;

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
        let model = server.effective_model_config();
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

pub(crate) fn planning_error(status: StatusCode, code: &str, message: String) -> Response {
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
