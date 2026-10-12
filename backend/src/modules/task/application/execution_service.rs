//! Task node execution use cases.
//!
//! The HTTP surface only needs to *start* an execution and map the outcome.
//! Resolving which executor should run a node, building the adapter registry,
//! dispatching and folding the result back into the persisted execution all
//! live here, so the route handlers stay extraction plus response mapping.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::app::state::AppServer;

use super::super::validation::ValidationPolicy;
use super::super::{
    AdapterError, AdapterRegistry, CommandExecutor, ExecutorDispatch, ExecutorKind, ExecutorRef,
    ExecutorResolutionError, ExecutorResolver, NodeContext, NodeExecution, NodeExecutionId,
    NodeExecutionStatus, ResolvedExecutionPlan, TaskGraphId, TaskHarnessError, TaskNodeId,
    TaskWorldRuntimeError, WorkflowExecutionProvider, WorkflowExecutor,
};

/// Resolve the executor a node should run under, or explain why it cannot.
pub async fn executor_resolver(
    server: &AppServer,
    graph_id: &TaskGraphId,
    node_id: &TaskNodeId,
) -> Result<ExecutorResolver, TaskWorldRuntimeError> {
    let proof_budget = super::super::proofloop::budget::graph_execution_budget(server, graph_id)
        .map_err(|error| TaskHarnessError::Graph(format!("{}: {}", error.code, error.message)))?;
    let graph = server
        .task_world
        .get_graph(graph_id)
        .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
    let node = graph.node(node_id).ok_or_else(|| {
        TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownNode(node_id.clone()))
    })?;
    if proof_budget.is_some() {
        ensure_workflow_budget_supported(server, node)?;
    }
    executor_resolver_for_node(server, node).await
}

pub async fn executor_resolver_for_node(
    server: &AppServer,
    node: &super::super::TaskNode,
) -> Result<ExecutorResolver, TaskWorldRuntimeError> {
    let Some(reference) = node.input.get("executor_ref").and_then(Value::as_str) else {
        return Err(TaskWorldRuntimeError::Harness(TaskHarnessError::Resolver(
            ExecutorResolutionError::MissingExecutorRef,
        )));
    };
    let parsed = ExecutorRef::new(reference).map_err(|error| {
        TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(error.to_string()))
    })?;
    let target = parsed
        .as_str()
        .split_once("://")
        .map(|(_, target)| target)
        .unwrap_or_default();
    let mut resolver = ExecutorResolver::new();
    match parsed.scheme() {
        "capability" => {
            let binding = super::capability_binding::resolve_binding(server, &node.input)
                .await
                .map_err(|error| TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(error)))?;
            resolver.register_capability(binding.capability_id);
        }
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

/// A workflow executor that reuses the persisted WorkflowRun machinery rather
/// than introducing a second workflow state machine.
pub(crate) struct ExistingWorkflowProvider {
    server: Arc<AppServer>,
    cancel: CancellationToken,
}

pub fn ensure_workflow_budget_supported(
    server: &AppServer,
    node: &super::super::TaskNode,
) -> Result<(), TaskWorldRuntimeError> {
    if let Some(workflow_id) = node
        .input
        .get("executor_ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("workflow://"))
    {
        if server
            .db
            .get_workflow_graph(workflow_id)
            .map_err(|error| TaskHarnessError::Graph(error))?
            .is_some_and(|workflow| {
                workflow.definition.nodes.iter().any(|node| {
                    matches!(
                        node.kind,
                        crate::modules::workflow::WorkflowNodeKind::Agent
                            | crate::modules::workflow::WorkflowNodeKind::Subagent
                    )
                })
            })
        {
            return Err(TaskHarnessError::Graph("proofloop_usage_unavailable: 此工作流中的模型循环尚不能计入目标预算，请使用既有工作流入口单独运行".into()).into());
        }
    }
    Ok(())
}

#[async_trait]
impl WorkflowExecutionProvider for ExistingWorkflowProvider {
    async fn execute(
        &self,
        workflow_id: &str,
        context: &NodeContext,
    ) -> Result<Option<Value>, AdapterError> {
        crate::modules::workflow::application::run_service::execute_for_task_harness(
            Arc::clone(&self.server),
            workflow_id,
            self.cancel.clone(),
            Some(context),
        )
        .await
        .map_err(AdapterError::Execution)
    }
}

/// Dispatch a claimed execution through the adapter registry and fold the
/// outcome back into the persisted execution row.
pub async fn dispatch_execution(
    server: Arc<AppServer>,
    graph_id: TaskGraphId,
    execution_id: NodeExecutionId,
) -> Result<NodeExecution, TaskWorldRuntimeError> {
    let remaining_budget =
        match super::super::proofloop::budget::graph_execution_budget(&server, &graph_id) {
            Ok(remaining) => remaining,
            Err(error) => {
                return server.task_world.fail_execution(
                    &graph_id,
                    &execution_id,
                    &error.code,
                    &error.message,
                    now(),
                )
            }
        };
    let execution = server
        .task_world
        .find_execution(&execution_id)
        .map(|(_, execution)| execution)
        .ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
    if execution.status == NodeExecutionStatus::Cancelled {
        return Ok(execution);
    }
    let ownership = server
        .task_world
        .claim_execution_dispatch(&graph_id, &execution_id)?;
    let cancel = ownership.cancellation_token();
    let budget_cancel = cancel.clone();
    let plan = server.task_world.execution_plan(&graph_id, &execution_id)?;
    let registry = AdapterRegistry::new()
        .with_adapter(CommandExecutor::new(server.command_router.clone()))
        .with_adapter(WorkflowExecutor::new(Arc::new(ExistingWorkflowProvider {
            server: Arc::clone(&server),
            cancel: cancel.clone(),
        })))
        .with_adapter(super::super::CapabilityExecutor::new(Arc::new(
            super::capability_execution::ExistingCapabilityProvider {
                server: Arc::clone(&server),
                cancel,
            },
        )));
    let result = if let Some(remaining) = remaining_budget {
        match tokio::time::timeout(remaining, registry.dispatch(&plan, &execution)).await {
            Ok(result) => result,
            Err(_) => {
                budget_cancel.cancel();
                return server.task_world.fail_execution(
                    &graph_id,
                    &execution_id,
                    "proofloop_budget_exhausted_result_unknown",
                    "执行耗时预算已耗尽，外部操作结果需核实，未自动重试",
                    now(),
                );
            }
        }
    } else {
        registry.dispatch(&plan, &execution).await
    };
    // A cooperative cancellation may settle after an in-flight atomic call.
    // Preserve the authoritative cancelled row and discard its late output.
    if let Some((_, current)) = server.task_world.find_execution(&execution_id) {
        if current.status == NodeExecutionStatus::Cancelled {
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

/// Pick the validation policy an executor's output must satisfy.
pub fn validation_policy(
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

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
