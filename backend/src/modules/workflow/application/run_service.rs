//! Workflow run lifecycle use cases.
//!
//! Starting a run, executing an existing graph on behalf of the task harness,
//! and cancelling a run all assemble the same runtime components (gateway,
//! verifier, agent executor, runner). They live here so the route handlers only
//! extract, call, and map a typed error back onto HTTP.

use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::verifier::DefaultVerifier;
use crate::app::state::AppServer;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::modules::task::NodeContext;
use crate::safety::{SecurityExecutionGateway, SecuritySubject};

use super::super::definition::WorkflowNodeConfig;
use super::super::executor::{
    LlmWorkflowAgentExecutor, SecurityGatewayNodeExecutor, WorkflowAgentExecutor,
};
use super::super::run::{WorkflowRun, WorkflowRunId, WorkflowRunStatus};
use super::super::runner::WorkflowRunner;

/// The descriptor a client needs immediately after a run is created.
pub struct RunStarted {
    pub run_id: String,
    pub execution_id: String,
    pub status: String,
}

/// Why a run lifecycle request could not be honoured.
#[derive(Debug)]
pub enum RunServiceError {
    /// The run id was malformed.
    InvalidId(String),
    /// The workflow graph does not exist.
    GraphNotFound,
    /// The workflow run does not exist.
    RunNotFound,
    /// The run could not be constructed.
    InvalidRun(String),
    /// The database could not read a graph or run.
    Read(String),
    /// The database could not persist a run.
    Persist(String),
}

impl std::fmt::Display for RunServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(message) => write!(formatter, "{message}"),
            Self::GraphNotFound => write!(formatter, "工作流图不存在"),
            Self::RunNotFound => write!(formatter, "工作流运行不存在"),
            Self::InvalidRun(message) => write!(formatter, "{message}"),
            Self::Read(message) => write!(formatter, "{message}"),
            Self::Persist(message) => write!(formatter, "{message}"),
        }
    }
}

/// Create a run, persist it, register it as active, spawn the runner, and
/// return the descriptor immediately. The HTTP request never waits for the
/// workflow to finish.
pub async fn start_workflow_run(
    server: &AppServer,
    graph_id: &str,
) -> Result<RunStarted, RunServiceError> {
    let graph = server
        .db
        .get_workflow_graph(graph_id)
        .map_err(RunServiceError::Read)?
        .ok_or(RunServiceError::GraphNotFound)?;

    // Subject is resolved server-side (the local desktop user), never client-supplied.
    let now = chrono::Utc::now().timestamp_millis();
    let context = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "workflow-runner",
        None,
        now,
    );
    let run = WorkflowRun::new(
        WorkflowRunId::generate(),
        context,
        graph.definition.clone(),
        now,
    )
    .map_err(|error| RunServiceError::InvalidRun(error.to_string()))?;
    let run_id = run.run_id.clone();
    let execution_id = run.execution_context.execution_id.clone();
    let initial_status = run.status.to_string();
    let run_id_str = run_id.to_string();

    // Persist immediately so the run is visible even before the runner starts.
    server
        .db
        .create_workflow_run(graph_id, &run)
        .map_err(RunServiceError::Persist)?;

    // Register an active-run cancel token.
    let token = CancellationToken::new();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id_str.clone(), token.clone());

    // Build runtime components before spawning.
    let gateway = build_gateway(server).await;
    let agent_executor = build_agent_executor(server);
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store))
            .with_agent_executor(agent_executor);
    let runner = WorkflowRunner::new(executor);

    let db = server.db.clone_connection();
    let graph_id_owned = graph_id.to_string();
    let active_runs = server.active_workflow_runs.clone();
    let run_id_str_for_spawn = run_id_str.clone();
    let mut run = run;
    tokio::spawn(async move {
        let result = runner
            .run(&mut run, &token, |r| {
                db.update_workflow_run(&graph_id_owned, r)
            })
            .await;
        // Terminal cleanup: always remove the active-run entry.
        active_runs.lock().remove(&run_id_str_for_spawn);
        if let Err(error) = result {
            tracing::error!(
                run_id = %run_id_str_for_spawn,
                error = %error,
                "workflow run ended with an execution error"
            );
        }
    });

    Ok(RunStarted {
        run_id: run_id_str,
        execution_id: execution_id.as_str().to_string(),
        status: initial_status,
    })
}

/// Execute an existing WorkflowGraph for Task Harness without introducing a
/// second workflow state machine. The same persisted WorkflowRun,
/// SecurityExecutionGateway and WorkflowRunner used by the HTTP runtime are
/// retained; Task Harness only observes the bounded terminal result.
pub(crate) async fn execute_for_task_harness(
    server: Arc<AppServer>,
    workflow_graph_id: &str,
    cancel: CancellationToken,
    node_context: Option<&NodeContext>,
) -> Result<Option<Value>, String> {
    let mut graph = server
        .db
        .get_workflow_graph(workflow_graph_id)?
        .ok_or_else(|| format!("workflow graph not found: {workflow_graph_id}"))?;
    if let Some(node_context) = node_context {
        let context = bounded_workflow_task_context(node_context);
        for node in &mut graph.definition.nodes {
            match &mut node.config {
                WorkflowNodeConfig::Agent { prompt } => {
                    prompt.push_str(&context);
                }
                WorkflowNodeConfig::Subagent { task, .. } => {
                    task.push_str(&context);
                }
                _ => {}
            }
        }
        graph
            .definition
            .validate()
            .map_err(|error| error.to_string())?;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let context = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "task-harness-workflow",
        None,
        now,
    );
    let mut run = WorkflowRun::new(
        WorkflowRunId::generate(),
        context,
        graph.definition.clone(),
        now,
    )
    .map_err(|error| error.to_string())?;
    let run_id = run.run_id.clone();
    server.db.create_workflow_run(workflow_graph_id, &run)?;
    server
        .active_workflow_runs
        .lock()
        .insert(run_id.to_string(), cancel.clone());

    let gateway = build_gateway(&server).await;
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store))
            .with_agent_executor(build_agent_executor(&server));
    let runner = WorkflowRunner::new(executor);
    let db = server.db.clone_connection();
    let graph_id = workflow_graph_id.to_string();
    let result = runner
        .run(&mut run, &cancel, move |state| {
            db.update_workflow_run(&graph_id, state)
        })
        .await;
    server.active_workflow_runs.lock().remove(run_id.as_str());
    result.map_err(|error| error.to_string())?;

    match run.status {
        WorkflowRunStatus::Completed => Ok(Some(serde_json::json!({
            "status": "completed",
            "completed": true,
            "workflow_graph_id": workflow_graph_id,
            "workflow_run_id": run_id.as_str(),
        }))),
        WorkflowRunStatus::WaitingApproval => Err("workflow paused for approval".to_string()),
        WorkflowRunStatus::Cancelled => Err("workflow cancelled".to_string()),
        WorkflowRunStatus::Failed => Err("workflow failed".to_string()),
        status => Err(format!("workflow ended in non-terminal status {status}")),
    }
}

/// Cancel a run: terminal runs are a stable no-op; active runs are signalled,
/// their pending approvals cancelled, and the run persisted as cancelled.
/// Returns the graph id and the final run state for the caller to render.
pub async fn cancel_workflow_run(
    server: &AppServer,
    run_id: &str,
) -> Result<(String, WorkflowRun), RunServiceError> {
    let run_id = WorkflowRunId::new(run_id)
        .map_err(|error| RunServiceError::InvalidId(error.to_string()))?;
    let stored = server
        .db
        .get_workflow_run(&run_id)
        .map_err(RunServiceError::Read)?
        .ok_or(RunServiceError::RunNotFound)?;
    let (graph_id, mut run) = (stored.workflow_graph_id.clone(), stored.run);

    // Terminal runs: cancel is a stable no-op that returns the current state.
    if matches!(
        run.status,
        WorkflowRunStatus::Completed | WorkflowRunStatus::Failed | WorkflowRunStatus::Cancelled
    ) {
        return Ok((graph_id, run));
    }

    // Signal the active runner (if any) to stop scheduling further nodes.
    if let Some(token) = server.active_workflow_runs.lock().get(run_id.as_str()) {
        token.cancel();
    }
    // Remove from the active registry so a later re-run isn't blocked.
    server.active_workflow_runs.lock().remove(run_id.as_str());

    // Cancel any pending approval bound to this run.
    for approval in server.approval_store.list_pending() {
        if approval.workflow_run_id.as_deref() == Some(run_id.as_str()) {
            let _ = server
                .approval_store
                .cancel(&approval.approval_id, &approval.conversation_id);
        }
    }

    // Persist the terminal Cancelled state (the spawned runner's persist closure
    // will also see the cancelled token and persist — idempotent overwrite).
    run.cancel(chrono::Utc::now().timestamp_millis());
    server
        .db
        .update_workflow_run(&graph_id, &run)
        .map_err(RunServiceError::Persist)?;
    Ok((graph_id, run))
}

/// Build the production security gateway for the workflow runtime: MCP-aware
/// registry, verifier, audit and DB, with grant enforcement.
pub(crate) async fn build_gateway(server: &AppServer) -> Arc<SecurityExecutionGateway> {
    let tool_registry = server.build_agent_tool_registry().await;
    let config = server.config.read().clone();
    Arc::new(
        SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(server.db.clone_connection()))
        .with_grant_enforcement(),
    )
}

/// Build the LLM-only agent executor from the configured model. Node config can
/// never supply secrets — model/provider/base_url all come from app config.
pub(crate) fn build_agent_executor(server: &AppServer) -> Arc<dyn WorkflowAgentExecutor> {
    let config = server.config.read().clone();
    Arc::new(LlmWorkflowAgentExecutor::new(
        &config.model,
        Arc::clone(&server.secret_resolver),
    ))
}

pub(crate) fn bounded_workflow_task_context(context: &NodeContext) -> String {
    let mut text = format!(
        "\n\nTask context (authorized, bounded):\nGoal: {}\nInstructions: {}",
        context.goal, context.instructions
    );
    if !context.resources.is_empty() {
        text.push_str("\nResources:\n");
        text.push_str(&context.resources.join("\n"));
    }
    text.chars().take(8_000).collect()
}
