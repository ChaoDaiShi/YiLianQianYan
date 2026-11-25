use super::*;

use crate::modules::task::application::execution_service;

#[derive(Default, Deserialize)]
pub struct ExecutionEvidenceQuery {
    #[serde(default)]
    pub include_output: bool,
    #[serde(default)]
    pub include_snapshot: bool,
    pub execution_id: Option<String>,
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
    let resolver = match execution_service::executor_resolver(&server, &graph_id, &node_id).await {
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
        Err(error) => execution_runtime_error(error),
    }
}

pub async fn list_executions(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, node_id)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<ExecutionEvidenceQuery>,
) -> Response {
    let graph_id = match parse_graph_id(graph_id) {
        Ok(graph_id) => graph_id,
        Err(error) => return runtime_error(error),
    };
    let node_id = match TaskNodeId::new(node_id) {
        Ok(node_id) => node_id,
        Err(error) => return runtime_error(TaskWorldRuntimeError::Graph(error)),
    };
    match server.task_world.list_node_executions(&graph_id, &node_id) {
        Ok(executions) => {
            let summaries: Vec<_> = executions.iter().map(execution_summary).collect();
            let mut response = json!({"executions":summaries});
            if query.include_output {
                if let Some(id) = &query.execution_id {
                    response["execution_output"] = executions
                        .iter()
                        .find(|e| {
                            e.id.as_str() == id
                                && matches!(
                                    e.status,
                                    crate::modules::task::NodeExecutionStatus::Succeeded
                                        | crate::modules::task::NodeExecutionStatus::Stale
                                )
                        })
                        .and_then(|e| e.output.clone())
                        .unwrap_or(Value::Null);
                } else {
                    response["latest_successful_output"] = executions
                        .iter()
                        .filter(|e| {
                            e.status == crate::modules::task::NodeExecutionStatus::Succeeded
                        })
                        .max_by_key(|e| e.attempt)
                        .and_then(|e| e.output.clone())
                        .unwrap_or(Value::Null);
                }
            }
            if query.include_snapshot {
                let selected = query
                    .execution_id
                    .as_ref()
                    .map(|id| executions.iter().find(|e| e.id.as_str() == id))
                    .unwrap_or_else(|| executions.iter().max_by_key(|e| e.attempt));
                response["execution_snapshot"] = selected.map(|e| json!(e)).unwrap_or(Value::Null);
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(error) => runtime_error(error),
    }
}

fn execution_runtime_error(error: TaskWorldRuntimeError) -> Response {
    match &error {
        TaskWorldRuntimeError::Harness(TaskHarnessError::ExternalResultConflict(execution_id)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error":"external_result_conflict", "message":error.to_string(), "execution_id":execution_id.as_str() })),
        ).into_response(),
        TaskWorldRuntimeError::Harness(TaskHarnessError::ExternalResultUnknown(execution_id)) => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "external_mutation_uncertain",
                "message": "上次调用的外部结果尚未核实，当前不能再次执行该节点",
                "execution_id": execution_id.as_str(),
            })),
        )
            .into_response(),
        _ => runtime_error(error),
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedOutcome {
    NoEffect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewExecutionRequest {
    pub expected_revision: u64,
    pub outcome: ReviewedOutcome,
    pub note: String,
}

pub async fn review_execution_outcome(
    State(server): State<Arc<AppServer>>,
    Path((graph_id, execution_id)): Path<(String, String)>,
    Json(request): Json<ReviewExecutionRequest>,
) -> Response {
    let graph = match parse_graph_id(graph_id) {
        Ok(id) => id,
        Err(error) => return runtime_error(error),
    };
    let execution_id = match NodeExecutionId::new(execution_id) {
        Ok(id) => id,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_execution", "message":error.to_string()})),
            )
                .into_response()
        }
    };
    let note = request.note.trim();
    if note.is_empty() || note.chars().count() > 1000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_review", "message":"请填写 1–1000 字的外部核验依据"})),
        )
            .into_response();
    }
    let ReviewedOutcome::NoEffect = request.outcome;
    match server.task_world.record_no_effect_review(
        &graph,
        &execution_id,
        request.expected_revision,
        note,
        now(),
    ) {
        Ok(execution) => {
            if let Some(pending) = server
                .approval_store
                .list_pending()
                .into_iter()
                .find(|item| execution.approval_ref.as_deref() == Some(item.approval_id.as_str()))
            {
                let _ = server
                    .approval_store
                    .cancel(&pending.approval_id, &pending.conversation_id);
            }
            (
                StatusCode::OK,
                Json(json!({"execution":execution_summary(&execution)})),
            )
                .into_response()
        }
        Err(error) => execution_runtime_error(error),
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
    if let Err(error) =
        execution_service::executor_resolver(&server, &graph_id, &request.node_id).await
    {
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
        Err(error) => execution_runtime_error(error),
    }
}
