//! Compatibility adapter for the existing TaskExecution.workflow_run_id link.
//! This adapter only settles status, never recreates output files or runs tools.
use crate::app::state::AppServer;
use crate::db::Database;
use crate::modules::task::model::{
    TaskExecution, TaskExecutionId, TaskExecutionStatus, TaskStatus,
};
use crate::modules::workflow::{WorkflowRun, WorkflowRunStatus};

pub fn parent(db: &Database, run: &WorkflowRun) -> Result<Option<TaskExecution>, String> {
    if run.execution_context.agent_name != "task-workflow" {
        return Ok(None);
    }
    let ids = {
        let conn = db.conn();
        let mut query = conn
            .prepare("SELECT id FROM task_executions WHERE workflow_run_id=?1 LIMIT 2")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([run.run_id.as_str()], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    if ids.len() > 1 {
        return Err("workflow is linked to multiple legacy task attempts".into());
    }
    if ids.is_empty() {
        return Err(
            "legacy workflow parent attempt is unavailable; no automatic continuation".into(),
        );
    }
    ids.first()
        .map(|id| {
            db.get_task_execution(&TaskExecutionId::new(id.as_str()).map_err(|e| e.to_string())?)?
                .ok_or("legacy task attempt is unavailable".into())
        })
        .transpose()
}

pub fn ensure_waiting(db: &Database, run: &WorkflowRun) -> Result<(), String> {
    if let Some(parent) = parent(db, run)? {
        let task = db.get_task(&parent.task_id)?.ok_or("legacy task missing")?;
        let latest = db
            .list_task_executions(&parent.task_id)?
            .into_iter()
            .max_by_key(|e| e.attempt);
        let workspace = db
            .get_workspace(&task.workspace_id)?
            .ok_or("legacy workspace missing")?;
        if parent.status != TaskExecutionStatus::WaitingApproval
            || task.status != TaskStatus::WaitingApproval
            || latest.as_ref().map(|e| &e.id) != Some(&parent.id)
            || workspace.status != crate::workspace::WorkspaceStatus::Active
        {
            return Err("legacy task no longer waits for this original workflow".into());
        }
    }
    Ok(())
}

pub fn sync(server: &AppServer, run: &WorkflowRun) -> Result<(), String> {
    let Some(parent) = parent(&server.db, run)? else {
        return Ok(());
    };
    if parent.status.is_terminal() {
        return Ok(());
    }
    if parent.status == TaskExecutionStatus::WaitingApproval
        && run.status == WorkflowRunStatus::WaitingApproval
    {
        return Ok(());
    }
    let (execution_status, task_status, error) = match run.status {
        WorkflowRunStatus::Completed => ("completed", "completed", None),
        WorkflowRunStatus::Failed => (
            "failed",
            "failed",
            Some(
                run.node_states
                    .iter()
                    .find_map(|n| n.error.clone())
                    .unwrap_or_else(|| "Child workflow failed".into()),
            ),
        ),
        WorkflowRunStatus::Cancelled => ("cancelled", "cancelled", None),
        WorkflowRunStatus::WaitingApproval => ("waiting_approval", "waiting_approval", None),
        _ => return Ok(()),
    };
    let now = chrono::Utc::now().timestamp_millis();
    let terminal = run.status.is_terminal();
    let mut conn = server.db.conn();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let changed=tx.execute("UPDATE task_executions SET status=?1,error=?2,finished_at=?3,updated_at=?4 WHERE id=?5 AND workflow_run_id=?6 AND status IN ('running','waiting_approval') AND NOT EXISTS(SELECT 1 FROM task_executions newer WHERE newer.task_id=task_executions.task_id AND newer.attempt>task_executions.attempt)",rusqlite::params![execution_status,error,terminal.then_some(now),now,parent.id.as_str(),run.run_id.as_str()]).map_err(|e|e.to_string())?;
    if changed > 0 {
        tx.execute("UPDATE tasks SET status=?1,completed_at=?2,updated_at=?3 WHERE id=?4 AND status IN ('running','waiting_approval')",rusqlite::params![task_status,(task_status=="completed").then_some(now),now,parent.task_id.as_str()]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cancel_child(db: &Database, execution: &TaskExecution, now: i64) -> Result<(), String> {
    let Some(run_id) = execution.workflow_run_id.as_ref() else {
        return Ok(());
    };
    let Some(stored) = db.get_workflow_run(run_id)? else {
        return Ok(());
    };
    if let Some(actual) = parent(db, &stored.run)? {
        if actual.id != execution.id {
            return Err("legacy workflow owner mismatch".into());
        }
        if !stored.run.status.is_terminal() {
            let mut run = stored.run;
            run.cancel(now);
            db.update_workflow_run(&stored.workflow_graph_id, &run)?;
        }
    }
    Ok(())
}

pub fn reconcile(server: &AppServer) -> Result<(), String> {
    let ids = {
        let conn = server.db.conn();
        let mut query=conn.prepare("SELECT e.id FROM task_executions e JOIN workflow_runs w ON w.run_id=e.workflow_run_id WHERE w.agent_name='task-workflow' AND (e.status IN ('running','waiting_approval') OR w.status NOT IN ('completed','failed','cancelled')) ORDER BY e.created_at,e.id").map_err(|e|e.to_string())?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut failure = None;
    for id in ids {
        let result = (|| -> Result<(), String> {
            let execution = server
                .db
                .get_task_execution(&TaskExecutionId::new(id.as_str()).map_err(|e| e.to_string())?)?
                .ok_or("legacy task attempt missing")?;
            if server.active_task_executions.lock().contains_key(&id) {
                return Ok(());
            }
            if execution.status.is_terminal() {
                return cancel_child(
                    &server.db,
                    &execution,
                    chrono::Utc::now().timestamp_millis(),
                );
            }
            let Some(run_id) = execution.workflow_run_id.as_ref() else {
                return Ok(());
            };
            let Some(stored) = server.db.get_workflow_run(run_id)? else {
                return Ok(());
            };
            // A missing in-memory approval after restart is uncertain; retain
            // an explicit non-replayable reason instead of continuing the DAG.
            if stored.run.status == WorkflowRunStatus::WaitingApproval
                && !server
                    .approval_store
                    .list_pending()
                    .iter()
                    .any(|a| a.workflow_run_id.as_deref() == Some(run_id.as_str()))
            {
                let mut run = stored.run;
                for node in &mut run.node_states {
                    if !node.status.is_terminal() {
                        node.status = crate::modules::workflow::NodeRunStatus::Failed;
                        node.error = Some(
                            "external_mutation_uncertain: saved approval is unavailable; no replay"
                                .into(),
                        );
                    }
                }
                run.refresh_status();
                server
                    .db
                    .update_workflow_run(&stored.workflow_graph_id, &run)?;
                return sync(server, &run);
            }
            sync(server, &stored.run)
        })();
        if let Err(reason) = result {
            tracing::error!(task_execution_id=%id,%reason,"legacy child workflow reconciliation deferred");
            failure.get_or_insert(reason);
        }
    }
    failure.map_or(Ok(()), Err)
}
