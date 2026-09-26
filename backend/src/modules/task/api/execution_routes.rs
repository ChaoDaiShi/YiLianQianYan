use super::*;

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

pub(crate) fn executor_resolver(
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

pub(crate) struct ExistingWorkflowProvider {
    server: Arc<AppServer>,
    cancel: tokio_util::sync::CancellationToken,
}

#[async_trait]
impl WorkflowExecutionProvider for ExistingWorkflowProvider {
    async fn execute(
        &self,
        workflow_id: &str,
        context: &NodeContext,
    ) -> Result<Option<Value>, AdapterError> {
        crate::api::workflow_runtime::execute_for_task_harness(
            Arc::clone(&self.server),
            workflow_id,
            self.cancel.clone(),
            Some(context),
        )
        .await
        .map_err(AdapterError::Execution)
    }
}

pub(crate) async fn dispatch_execution(
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

pub(crate) fn validation_policy(
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
