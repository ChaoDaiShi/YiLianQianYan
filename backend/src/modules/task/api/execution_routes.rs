use super::*;

use crate::modules::task::application::execution_service;

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
    let resolver = match execution_service::executor_resolver(&server, &graph_id, &node_id) {
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
                        if let Err(error) = execution_service::dispatch_execution(
                            server,
                            graph_id.clone(),
                            execution_id.clone(),
                        )
                        .await
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
    if let Err(error) = execution_service::executor_resolver(&server, &graph_id, &request.node_id) {
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
