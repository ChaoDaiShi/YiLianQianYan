//! Workflow run lifecycle use cases.
//!
//! Starting a run, executing an existing graph on behalf of the task harness,
//! and cancelling a run all assemble the same runtime components (gateway,
//! verifier, agent executor, runner). They live here so the route handlers only
//! extract, call, and map a typed error back onto HTTP.

use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::app::state::AppServer;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::modules::harness_profiles as harness;
use crate::modules::task::{ExecutorDispatch, NodeContext, NodeExecution};
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
    start_workflow_run_with_inputs(server, graph_id, Default::default()).await
}

pub async fn start_workflow_run_with_inputs(
    server: &AppServer,
    graph_id: &str,
    inputs: super::run_inputs::WorkflowRunInputs,
) -> Result<RunStarted, RunServiceError> {
    start_workflow_run_identified(server, graph_id, inputs, WorkflowRunId::generate()).await
}

pub(crate) async fn start_workflow_run_identified(
    server: &AppServer,
    graph_id: &str,
    inputs: super::run_inputs::WorkflowRunInputs,
    run_id: WorkflowRunId,
) -> Result<RunStarted, RunServiceError> {
    let selection = harness::active(&server.db).map_err(RunServiceError::Read)?;
    start_workflow_run_bound(server, graph_id, inputs, run_id, selection).await
}

pub(crate) async fn start_workflow_run_selected(
    server: &AppServer,
    graph_id: &str,
    selection: Option<harness::Selection>,
) -> Result<RunStarted, RunServiceError> {
    start_workflow_run_bound(
        server,
        graph_id,
        Default::default(),
        WorkflowRunId::generate(),
        selection,
    )
    .await
}

async fn start_workflow_run_bound(
    server: &AppServer,
    graph_id: &str,
    inputs: super::run_inputs::WorkflowRunInputs,
    run_id: WorkflowRunId,
    selection: Option<harness::Selection>,
) -> Result<RunStarted, RunServiceError> {
    if server.cloud_revocation_retry_cancel.is_cancelled() {
        return Err(RunServiceError::InvalidRun("应用正在退出".into()));
    }
    ensure_harness_workflow(server, graph_id, selection.as_ref())
        .map_err(RunServiceError::InvalidRun)?;
    crate::modules::capability::import_store::require_active_plugin_resource(&server.db, graph_id)
        .map_err(RunServiceError::InvalidRun)?;
    let mut graph = server
        .db
        .get_workflow_graph(graph_id)
        .map_err(RunServiceError::Read)?
        .ok_or(RunServiceError::GraphNotFound)?;

    super::run_inputs::apply_run_inputs(&mut graph.definition, inputs)
        .map_err(RunServiceError::InvalidRun)?;

    // Subject is resolved server-side (the local desktop user), never client-supplied.
    let now = chrono::Utc::now().timestamp_millis();
    let mut context = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "workflow-runner",
        None,
        now,
    );
    context.agent_id = crate::safety::execution_gateway::current_agent_id()
        .map(crate::modules::task::AgentId::new)
        .transpose()
        .map_err(|e| RunServiceError::InvalidRun(e.to_string()))?;
    context.owner = Some(crate::execution::ExecutionOwner::Workflow {
        workflow_graph_id: graph_id.into(),
        run_id: run_id.to_string(),
    });
    let run = WorkflowRun::new(run_id, context, graph.definition.clone(), now)
        .map_err(|error| RunServiceError::InvalidRun(error.to_string()))?;
    let run_id = run.run_id.clone();
    let execution_id = run.execution_context.execution_id.clone();
    let initial_status = run.status.to_string();
    let run_id_str = run_id.to_string();
    let gateway = build_selected_gateway(server, selection.as_ref())
        .await
        .map_err(RunServiceError::Read)?;

    // Persist immediately so the run is visible even before the runner starts.
    server
        .db
        .create_workflow_run(graph_id, &run)
        .map_err(RunServiceError::Persist)?;
    record_harness(
        server,
        graph_id,
        run_id.as_str(),
        selection.as_ref(),
        Default::default(),
    )
    .map_err(RunServiceError::Persist)?;

    // Register an active-run cancel token.
    let token = server.cloud_revocation_retry_cancel.child_token();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id_str.clone(), token.clone());

    // Build runtime components before spawning.
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
    match execute_task_child(server, workflow_graph_id, cancel, node_context, None).await? {
        ExecutorDispatch::Completed { output } => Ok(output),
        ExecutorDispatch::WaitingApproval { .. } => Err("workflow paused for approval".into()),
        ExecutorDispatch::Failed { code, error } => Err(match code.as_str() {
            "workflow_cancelled" => "workflow cancelled".into(),
            "workflow_failed" => "workflow failed".into(),
            _ => error,
        }),
    }
}

pub(crate) async fn dispatch_for_task_harness(
    server: Arc<AppServer>,
    workflow_graph_id: &str,
    cancel: CancellationToken,
    execution: &NodeExecution,
) -> Result<ExecutorDispatch, String> {
    // Dispatch ownership is reserved by TaskWorld. Repeated observation may
    // reuse the saved child outcome, but never starts that child a second time.
    if let Some(stored) =
        super::task_parent::existing_child(&server.db, execution, workflow_graph_id)?
    {
        return super::task_parent::dispatch_result(&stored, &server.approval_store);
    }
    execute_task_child(
        server,
        workflow_graph_id,
        cancel,
        Some(&execution.context),
        Some(execution),
    )
    .await
}

async fn execute_task_child(
    server: Arc<AppServer>,
    workflow_graph_id: &str,
    cancel: CancellationToken,
    node_context: Option<&NodeContext>,
    parent: Option<&NodeExecution>,
) -> Result<ExecutorDispatch, String> {
    let selection = harness::active(&server.db)?;
    ensure_harness_workflow(&server, workflow_graph_id, selection.as_ref())?;
    crate::modules::capability::import_store::require_active_plugin_resource(
        &server.db,
        workflow_graph_id,
    )?;
    let mut graph = server
        .db
        .get_workflow_graph(workflow_graph_id)?
        .ok_or_else(|| format!("workflow graph not found: {workflow_graph_id}"))?;
    let adapted_context = node_context.map(|context| {
        let (text, mut manifest) =
            crate::execution::context_manifest::render_node_context_with_manifest(context, 8_000);
        if !graph.definition.nodes.iter().any(|node| {
            matches!(
                node.config,
                WorkflowNodeConfig::Agent { .. } | WorkflowNodeConfig::Subagent { .. }
            )
        }) {
            for entry in &mut manifest.entries {
                entry.status = crate::execution::context_manifest::ContextStatus::Unsupported;
                entry.chars = 0;
                entry.reason = Some(
                    "This workflow has no prompt consumer; projected inputs were not consumed"
                        .into(),
                );
            }
            manifest.total_chars = 0;
        }
        (text, manifest)
    });
    if let Some((context, _)) = &adapted_context {
        for node in &mut graph.definition.nodes {
            match &mut node.config {
                WorkflowNodeConfig::Agent { prompt } => prompt.push_str(&context),
                WorkflowNodeConfig::Subagent { task, .. } => task.push_str(&context),
                _ => {}
            }
        }
        graph
            .definition
            .validate()
            .map_err(|error| error.to_string())?;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let run_id = WorkflowRunId::generate();
    let mut context = ExecutionContext::new(
        ExecutionId::generate(),
        SecuritySubject::local_user().subject_id,
        "task-harness-workflow",
        parent
            .map(|p| ExecutionId::new(p.id.as_str()))
            .transpose()
            .map_err(|e| e.to_string())?,
        now,
    )
    .with_owner(crate::execution::ExecutionOwner::Workflow {
        workflow_graph_id: workflow_graph_id.into(),
        run_id: run_id.to_string(),
    });
    context.agent_id = crate::safety::execution_gateway::current_agent_id()
        .map(crate::modules::task::AgentId::new)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut run = WorkflowRun::new(run_id, context, graph.definition.clone(), now)
        .map_err(|e| e.to_string())?;
    let gateway = build_selected_gateway(&server, selection.as_ref()).await?;
    server.db.create_workflow_run(workflow_graph_id, &run)?;
    record_harness(
        &server,
        workflow_graph_id,
        run.run_id.as_str(),
        selection.as_ref(),
        adapted_context
            .map(|(_, manifest)| manifest)
            .unwrap_or_default(),
    )?;
    let run_id = run.run_id.clone();
    server
        .active_workflow_runs
        .lock()
        .insert(run_id.to_string(), cancel.clone());
    let executor =
        SecurityGatewayNodeExecutor::new(Arc::clone(&gateway), Arc::clone(&server.approval_store))
            .with_agent_executor(build_agent_executor(&server));
    let runner = WorkflowRunner::new(executor);
    let db = server.db.clone_connection();
    let graph_id = workflow_graph_id.to_owned();
    let result = runner
        .run(&mut run, &cancel, move |state| {
            db.update_workflow_run(&graph_id, state)
        })
        .await;
    server.active_workflow_runs.lock().remove(run_id.as_str());
    // Always read authoritative persistence after cancellation or a late write.
    let stored = server
        .db
        .get_workflow_run(&run_id)?
        .ok_or("saved child run missing")?;
    if result.is_err() && !stored.run.status.is_terminal() {
        return Ok(ExecutorDispatch::Failed{
            code:"external_mutation_uncertain".into(),
            error:format!("Workflow checkpoint failed; external effects require review and were not replayed: {}",result.unwrap_err()),
        });
    }
    super::task_parent::dispatch_result(&stored, &server.approval_store)
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
    super::task_parent::sync_parent(server, run.run_id.as_str())
        .map_err(RunServiceError::Persist)?;
    Ok((graph_id, run))
}

/// Build the production security gateway for the workflow runtime: MCP-aware
/// registry, verifier, audit and DB, with grant enforcement.
pub(crate) async fn build_gateway(server: &AppServer) -> Arc<SecurityExecutionGateway> {
    build_selected_gateway(server, None)
        .await
        .expect("default workflow gateway uses no Harness data")
}

fn ensure_harness_workflow(
    server: &AppServer,
    graph_id: &str,
    selection: Option<&harness::Selection>,
) -> Result<(), String> {
    if let Some(selection) = selection {
        if !selection.allows_workflow(&server.db, graph_id)? {
            return Err("当前 Harness 未启用此工作流".into());
        }
    }
    Ok(())
}

fn record_harness(
    server: &AppServer,
    graph_id: &str,
    run_id: &str,
    selection: Option<&harness::Selection>,
    mut manifest: crate::execution::context_manifest::ContextManifest,
) -> Result<(), String> {
    harness::record(&mut manifest, selection);
    crate::execution::context_manifest::record_manifest(
        &server.db,
        &crate::execution::ExecutionOwner::Workflow {
            workflow_graph_id: graph_id.into(),
            run_id: run_id.into(),
        },
        &manifest,
    )
}

pub(crate) async fn gateway_for_run(
    server: &AppServer,
    run_id: &str,
) -> Result<Arc<SecurityExecutionGateway>, String> {
    let id = WorkflowRunId::new(run_id).map_err(|error| error.to_string())?;
    let run = server.db.get_workflow_run(&id)?.ok_or("工作流运行不存在")?;
    let owner = crate::execution::ExecutionOwner::Workflow {
        workflow_graph_id: run.workflow_graph_id.clone(),
        run_id: run_id.into(),
    };
    let manifest = crate::execution::context_manifest::load_manifest(&server.db, &owner)?;
    let selection = harness::from_manifest(&server.db, manifest.as_ref())?;
    ensure_harness_workflow(server, &run.workflow_graph_id, selection.as_ref())?;
    build_selected_gateway(server, selection.as_ref()).await
}

async fn build_selected_gateway(
    server: &AppServer,
    selection: Option<&harness::Selection>,
) -> Result<Arc<SecurityExecutionGateway>, String> {
    let scope = selection
        .map(|selection| harness::tool_scope(server, selection, None))
        .transpose()?;
    Ok(Arc::new(crate::safety::production_gateway::build(
        server,
        &server.workspace_root,
        server
            .build_agent_tool_registry_scoped(scope.as_ref())
            .await,
        crate::safety::execution_gateway::current_agent_id(),
    )))
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
    crate::execution::context_manifest::render_node_context(context, 8_000)
}
