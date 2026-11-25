//! Durable Task Harness -> Workflow linkage uses the existing parent_execution_id.
//! All reconciliation is observational: it never invokes a tool or replays a run.
#[path = "task_parent_legacy.rs"]
mod legacy;
use super::super::{WorkflowRun, WorkflowRunId, WorkflowRunStatus};
use crate::app::state::AppServer;
use crate::db::{Database, StoredWorkflowRun};
use crate::modules::task::runtime::TaskDispatchLease;
use crate::modules::task::validation::ValidationPolicy;
use crate::modules::task::{ExecutorDispatch, NodeExecution, NodeExecutionId, NodeExecutionStatus};
use crate::safety::{ApprovalStore, PendingApproval};
pub use legacy::cancel_child as cancel_legacy_child;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn run_ids(db: &Database, parent: Option<&str>) -> Result<Vec<String>, String> {
    let conn = db.conn();
    let mut statement = conn.prepare("SELECT run_id FROM workflow_runs WHERE parent_execution_id IS NOT NULL AND agent_name='task-harness-workflow' AND (?1 IS NULL OR parent_execution_id=?1) AND (?1 IS NOT NULL OR status NOT IN ('completed','failed','cancelled') OR EXISTS(SELECT 1 FROM task_node_executions e WHERE e.execution_id=workflow_runs.parent_execution_id AND (e.status IN ('dispatching','running','waiting_approval','validating') OR (workflow_runs.status='completed' AND e.status IN ('failed','cancelled','stale') AND e.failure_code IN ('execution_interrupted','external_mutation_uncertain','proofloop_budget_exhausted_result_unknown','paused','operator_reported_no_effect'))))) ORDER BY created_at,run_id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([parent], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn parent_execution(db: &Database, run: &WorkflowRun) -> Result<Option<NodeExecution>, String> {
    if run.execution_context.agent_name != "task-harness-workflow" {
        return Ok(None);
    }
    let Some(id) = run.execution_context.parent_execution_id.as_ref() else {
        return Ok(None);
    };
    let id = NodeExecutionId::new(id.as_str()).map_err(|e| e.to_string())?;
    db.load_node_execution(&id)
        .map_err(|e| e.to_string())?
        .map(Some)
        .ok_or_else(|| "workflow parent execution is unavailable; no automatic replay".into())
}

pub fn existing_child(
    db: &Database,
    execution: &NodeExecution,
    workflow_id: &str,
) -> Result<Option<StoredWorkflowRun>, String> {
    let ids = run_ids(db, Some(execution.id.as_str()))?;
    if ids.len() > 1 {
        return Err("multiple persisted child runs; manual review required".into());
    }
    let Some(id) = ids.first() else {
        return Ok(None);
    };
    let stored = db
        .get_workflow_run(&WorkflowRunId::new(id.as_str()).map_err(|e| e.to_string())?)?
        .ok_or("child workflow run missing")?;
    if stored.workflow_graph_id != workflow_id {
        return Err("child workflow binding changed; no replay".into());
    }
    Ok(Some(stored))
}

pub fn dispatch_result(
    stored: &StoredWorkflowRun,
    approvals: &ApprovalStore,
) -> Result<ExecutorDispatch, String> {
    match stored.run.status {
        WorkflowRunStatus::Completed => Ok(ExecutorDispatch::Completed {
            output: Some(
                serde_json::json!({"status":"completed","completed":true,"workflow_graph_id":stored.workflow_graph_id,"workflow_run_id":stored.run.run_id.as_str()}),
            ),
        }),
        WorkflowRunStatus::WaitingApproval => {
            let pending = approvals
                .list_pending()
                .into_iter()
                .filter(|approval| {
                    approval.workflow_run_id.as_deref() == Some(stored.run.run_id.as_str())
                })
                .collect::<Vec<_>>();
            if pending.len() != 1 {
                return Ok(ExecutorDispatch::Failed{code:"external_mutation_uncertain".into(),error:"Saved workflow is suspended but its approval is unavailable; review required, no replay".into()});
            }
            Ok(ExecutorDispatch::WaitingApproval {
                approval_ref: pending[0].approval_id.clone(),
            })
        }
        WorkflowRunStatus::Cancelled => Ok(ExecutorDispatch::Failed {
            code: "workflow_cancelled".into(),
            error: "Child workflow was cancelled".into(),
        }),
        WorkflowRunStatus::Failed => {
            let error = stored
                .run
                .node_states
                .iter()
                .find_map(|node| node.error.clone())
                .unwrap_or_else(|| "Child workflow failed".into());
            let code = if error.starts_with("external_mutation_uncertain:") {
                "external_mutation_uncertain"
            } else {
                "workflow_failed"
            };
            Ok(ExecutorDispatch::Failed {
                code: code.into(),
                error,
            })
        }
        _ => Ok(ExecutorDispatch::Failed {
            code: "external_mutation_uncertain".into(),
            error: "Child run has no settled result; manual review required, no replay".into(),
        }),
    }
}

/// Low-level approval API is also guarded: a caller cannot bypass the facade.
pub fn ensure_parent_waiting(
    db: &Database,
    run: &WorkflowRun,
    approval_id: &str,
) -> Result<(), String> {
    if let Some(parent) = parent_execution(db, run)? {
        if parent.status != NodeExecutionStatus::WaitingApproval
            || parent.approval_ref.as_deref() != Some(approval_id)
        {
            return Err("parent execution no longer waits for this exact approval".into());
        }
    }
    legacy::ensure_waiting(db, run)?;
    Ok(())
}

struct WorkflowApprovalLease {
    server: Arc<AppServer>,
    run_id: String,
    token: CancellationToken,
    _parent: Option<TaskDispatchLease>,
    legacy_execution: Option<String>,
    legacy_workspace: Option<crate::workspace::WorkspaceId>,
}
impl Drop for WorkflowApprovalLease {
    fn drop(&mut self) {
        self.server.active_workflow_runs.lock().remove(&self.run_id);
        if let Some(id) = &self.legacy_execution {
            self.server.active_task_executions.lock().remove(id);
        }
    }
}

fn claim_approval(
    server: &Arc<AppServer>,
    approval: &PendingApproval,
) -> Result<WorkflowApprovalLease, String> {
    let id = approval
        .workflow_run_id
        .as_deref()
        .ok_or("approval has no workflow run")?;
    let run = server
        .db
        .get_workflow_run(&WorkflowRunId::new(id).map_err(|e| e.to_string())?)?
        .ok_or("workflow run missing")?
        .run;
    if run.status != WorkflowRunStatus::WaitingApproval {
        return Err("workflow is not waiting for approval".into());
    }
    ensure_parent_waiting(&server.db, &run, &approval.approval_id)?;
    let parent = parent_execution(&server.db, &run)?
        .map(|parent| {
            server
                .task_world
                .claim_approval_dispatch(&parent.graph_id, &parent.id)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let token = parent
        .as_ref()
        .map(TaskDispatchLease::cancellation_token)
        .unwrap_or_default();
    let mut active = server.active_workflow_runs.lock();
    if server.cloud_revocation_retry_cancel.is_cancelled() {
        return Err("应用正在退出".into());
    }
    if active.contains_key(id) {
        return Err("workflow approval is already being resolved".into());
    }
    let legacy_parent = legacy::parent(&server.db, &run)?;
    let legacy_workspace = legacy_parent
        .as_ref()
        .map(|execution| {
            server
                .db
                .get_task(&execution.task_id)?
                .map(|task| task.workspace_id)
                .ok_or_else(|| "legacy task missing".to_string())
        })
        .transpose()?;
    let legacy_execution = legacy_parent.map(|execution| execution.id.to_string());
    let mut legacy_active = server.active_task_executions.lock();
    if legacy_execution
        .as_ref()
        .is_some_and(|id| legacy_active.contains_key(id))
    {
        return Err("legacy task workflow is already resolving".into());
    }
    if let Some(id) = &legacy_execution {
        legacy_active.insert(id.clone(), token.clone());
    }
    active.insert(id.to_owned(), token.clone());
    drop(legacy_active);
    drop(active);
    Ok(WorkflowApprovalLease {
        server: Arc::clone(server),
        run_id: id.to_owned(),
        token,
        _parent: parent,
        legacy_execution,
        legacy_workspace,
    })
}

pub async fn resolve_approval(
    server: &Arc<AppServer>,
    approval_id: &str,
    subject_id: &str,
    approve: bool,
) -> Result<(), String> {
    let approval = server
        .approval_store
        .get(approval_id)
        .ok_or("approval not found")?;
    let lease = claim_approval(server, &approval)?;
    let agent_executor: Arc<dyn crate::modules::workflow::WorkflowAgentExecutor> =
        match lease.legacy_workspace.clone() {
            Some(workspace) => Arc::new(
                crate::modules::workflow::LlmWorkflowAgentExecutor::in_workspace(
                    server.db.clone_connection(),
                    server.config.clone(),
                    server.secret_resolver.clone(),
                    workspace,
                    lease.run_id.clone(),
                ),
            ),
            None => super::run_service::build_agent_executor(server),
        };
    let result = super::approval_service::resolve_workflow_approval(
        &super::run_service::gateway_for_run(server, &lease.run_id).await?,
        &server.approval_store,
        &server.db,
        approval_id,
        subject_id,
        approve,
        &lease.token,
        agent_executor,
    )
    .await;
    let sync = sync_parent(server, &lease.run_id);
    result.and(sync)
}

pub async fn cancel_approval(
    server: &AppServer,
    approval_id: &str,
    subject_id: &str,
) -> Result<(), String> {
    let approval = server
        .approval_store
        .get(approval_id)
        .ok_or("approval not found")?;
    super::approval_service::cancel_workflow_approval(
        &server.approval_store,
        &server.db,
        approval_id,
        subject_id,
    )
    .await?;
    if let Some(id) = approval.workflow_run_id.as_deref() {
        if let Some(token) = server.active_workflow_runs.lock().get(id) {
            token.cancel();
        }
        sync_parent(server, id)?;
    }
    Ok(())
}

/// Fold the child's actual saved result into the original attempt. Terminal
/// parent results are authoritative, including cancellation and stale attempts.
pub fn sync_parent(server: &AppServer, run_id: &str) -> Result<(), String> {
    let stored = server
        .db
        .get_workflow_run(&WorkflowRunId::new(run_id).map_err(|e| e.to_string())?)?
        .ok_or("child workflow missing")?;
    if stored.run.execution_context.agent_name == "task-workflow" {
        return legacy::sync(server, &stored.run);
    }
    let parent = match parent_execution(&server.db, &stored.run) {
        Ok(Some(parent)) => parent,
        Ok(None) => return Ok(()),
        Err(_) if stored.run.status.is_terminal() => return Ok(()),
        Err(reason) => return Err(reason),
    };
    // A committed child can settle an unknown attempt after restart; explicit
    // cancellation, stale attempts, and other terminal decisions stay final.
    if parent.status.is_terminal() {
        if matches!(
            parent.status,
            NodeExecutionStatus::Failed
                | NodeExecutionStatus::Cancelled
                | NodeExecutionStatus::Stale
        ) && (crate::modules::task::execution::external_result_is_uncertain(
            parent.failure_code(),
        ) || matches!(
            parent.failure_code(),
            Some("paused" | "operator_reported_no_effect")
        )) && stored.run.status == WorkflowRunStatus::Completed
        {
            existing_child(&server.db, &parent, &stored.workflow_graph_id)?;
            if let Some((graph, _)) = server.task_world.find_execution(&parent.id) {
                if let ExecutorDispatch::Completed {
                    output: Some(output),
                } = dispatch_result(&stored, &server.approval_store)?
                {
                    server
                        .task_world
                        .reconcile_completed_workflow(
                            &graph,
                            &parent.id,
                            &stored.workflow_graph_id,
                            output,
                            chrono::Utc::now().timestamp_millis(),
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        return Ok(());
    }
    let Some((graph, current)) = server.task_world.find_execution(&parent.id) else {
        return Err("parent runtime execution missing".into());
    };
    if current.status.is_terminal() {
        return Ok(());
    }
    if current.executor_ref.as_ref().map(|r| r.as_str())
        != Some(format!("workflow://{}", stored.workflow_graph_id).as_str())
    {
        return Err("child workflow does not match parent executor".into());
    }
    let now = chrono::Utc::now().timestamp_millis();
    match dispatch_result(&stored, &server.approval_store)? {
        ExecutorDispatch::Completed {
            output: Some(output),
        } => {
            server
                .task_world
                .complete_execution(
                    &graph,
                    &parent.id,
                    output,
                    ValidationPolicy::WorkflowResult,
                    now,
                )
                .map_err(|e| e.to_string())?;
        }
        ExecutorDispatch::WaitingApproval { approval_ref } => {
            if current.status == NodeExecutionStatus::WaitingApproval
                && current.approval_ref.as_deref() == Some(approval_ref.as_str())
            {
                return Ok(());
            }
            if current.status == NodeExecutionStatus::WaitingApproval {
                server
                    .task_world
                    .update_execution(&graph, &parent.id, NodeExecutionStatus::Running, None, now)
                    .map_err(|e| e.to_string())?;
            }
            server
                .task_world
                .mark_execution_waiting_approval(&graph, &parent.id, &approval_ref, now)
                .map_err(|e| e.to_string())?;
        }
        ExecutorDispatch::Failed { code, error } => {
            if stored.run.status == WorkflowRunStatus::Cancelled {
                server
                    .task_world
                    .update_execution(
                        &graph,
                        &parent.id,
                        NodeExecutionStatus::Cancelled,
                        None,
                        now,
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                server
                    .task_world
                    .fail_execution(&graph, &parent.id, &code, &error, now)
                    .map_err(|e| e.to_string())?;
            }
        }
        _ => return Err("child workflow produced no result".into()),
    }
    Ok(())
}

/// Parent stop is durable even while no child future exists (waiting approval).
pub fn cancel_children(
    db: &Database,
    parent_execution_id: &NodeExecutionId,
    now: i64,
) -> Result<(), String> {
    for id in run_ids(db, Some(parent_execution_id.as_str()))? {
        let stored = db
            .get_workflow_run(&WorkflowRunId::new(id.as_str()).map_err(|e| e.to_string())?)?
            .ok_or("child workflow missing")?;
        if !stored.run.status.is_terminal() {
            let mut run = stored.run;
            run.cancel(now);
            db.update_workflow_run(&stored.workflow_graph_id, &run)?;
        }
    }
    Ok(())
}

pub fn reconcile_children(db: &Database) -> Result<(), String> {
    for id in run_ids(db, None)? {
        let outcome = (|| -> Result<(), String> {
            let stored = db
                .get_workflow_run(&WorkflowRunId::new(id.as_str()).map_err(|e| e.to_string())?)?
                .ok_or("child workflow missing")?;
            let parent = match parent_execution(db, &stored.run) {
                Ok(Some(parent)) => parent,
                Ok(None) => return Ok(()),
                Err(reason) => {
                    if !stored.run.status.is_terminal() {
                        let mut run = stored.run;
                        run.cancel(chrono::Utc::now().timestamp_millis());
                        if let Some(node) = run.node_states.first_mut() {
                            node.error = Some(reason);
                        }
                        db.update_workflow_run(&stored.workflow_graph_id, &run)?;
                    }
                    return Ok(());
                }
            };
            if parent.status.is_terminal() && !stored.run.status.is_terminal() {
                cancel_children(db, &parent.id, chrono::Utc::now().timestamp_millis())?;
            }
            Ok(())
        })();
        if let Err(reason) = outcome {
            tracing::error!(workflow_run_id=%id,%reason,"child workflow reconciliation deferred; other owners continue");
        }
    }
    Ok(())
}

pub fn reconcile(server: &AppServer) -> Result<(), String> {
    let legacy_failure = legacy::reconcile(server).err();
    reconcile_children(&server.db)?;
    for approval in server.approval_store.list_pending() {
        let outcome = (|| -> Result<(), String> {
            let Some(id) = approval.workflow_run_id.as_deref() else {
                return Ok(());
            };
            let Some(stored) = server
                .db
                .get_workflow_run(&WorkflowRunId::new(id).map_err(|e| e.to_string())?)?
            else {
                return Ok(());
            };
            if matches!(
                stored.run.execution_context.agent_name.as_str(),
                "task-harness-workflow" | "task-workflow"
            ) && stored.run.status.is_terminal()
            {
                server
                    .approval_store
                    .cancel(&approval.approval_id, &approval.conversation_id)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(reason) = outcome {
            tracing::error!(approval_id=%approval.approval_id,%reason,"workflow approval reconciliation deferred; other owners continue");
        }
    }
    for id in run_ids(&server.db, None)? {
        // Consuming approval and persisting its tool outcome are separate
        // boundaries. The owning future may still be executing the approved
        // call; absence from pending is not then an orphan or failure.
        if server.active_workflow_runs.lock().contains_key(&id) {
            continue;
        }
        let outcome = (|| -> Result<(), String> {
            let stored = server
                .db
                .get_workflow_run(&WorkflowRunId::new(id.as_str()).map_err(|e| e.to_string())?)?
                .ok_or("child workflow missing")?;
            if stored.run.status.is_terminal()
                || stored.run.status == WorkflowRunStatus::WaitingApproval
            {
                sync_parent(server, &id)?;
            }
            Ok(())
        })();
        if let Err(reason) = outcome {
            tracing::error!(workflow_run_id=%id,%reason,"parent outcome reconciliation deferred; other owners continue");
        }
    }
    legacy_failure.map_or(Ok(()), Err)
}

#[cfg(test)]
#[path = "task_parent_tests.rs"]
mod tests;
