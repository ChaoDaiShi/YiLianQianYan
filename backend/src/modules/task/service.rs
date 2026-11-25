// ============================================================
// Task runner — manages the TaskExecution lifecycle (start / retry /
// resume / cancel) and the active-execution registry.
//
// This is NOT a replacement for WorkflowRunner or the Agent runtime — it
// orchestrates them. One task may only have one active execution at a time.
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use super::model::*;
use super::orchestrator::TaskOrchestrator;
use crate::db::Database;
use crate::execution::{ExecutionContext, ExecutionId};

/// Builds a fresh orchestrator (injected for testability).
#[async_trait]
pub trait OrchestratorBuilder: Send + Sync {
    async fn build(&self) -> TaskOrchestrator;
}

#[derive(Clone)]
pub struct TaskRunner {
    db: Database,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    builder: Arc<dyn OrchestratorBuilder>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStartResponse {
    pub task_id: String,
    pub task_execution_id: String,
    pub execution_id: String,
    pub status: String,
}

impl TaskRunner {
    pub fn new(
        db: Database,
        builder: Arc<dyn OrchestratorBuilder>,
        active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    ) -> Self {
        Self {
            db,
            active,
            builder,
        }
    }

    /// Start (or retry) a task: create a new TaskExecution with the next
    /// attempt number and spawn the orchestrator. Returns immediately.
    pub async fn start(&self, task_id: &TaskId) -> Result<TaskStartResponse, String> {
        let task = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| "task not found".to_string())?;
        if !matches!(
            task.status,
            TaskStatus::Draft | TaskStatus::Ready | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            // Running/waiting tasks can't start a second concurrent execution.
            if self.db.find_active_task_execution(task_id)?.is_some() {
                return Err("task already has an active execution".to_string());
            }
        }
        let existing = self.db.list_task_executions(task_id)?;
        ensure_replay_is_safe(&existing)?;
        let attempt = existing.len() as u32 + 1;

        let now = chrono::Utc::now().timestamp_millis();
        let task_execution_id = TaskExecutionId::generate();
        let ctx = ExecutionContext::new(
            ExecutionId::generate(),
            task_owner_subject(&self.db, &task),
            "task-runner",
            None,
            now,
        );
        let ctx = ctx.with_owner(crate::execution::ExecutionOwner::LegacyTask {
            task_id: task_id.to_string(),
            execution_id: task_execution_id.to_string(),
        });
        let execution = TaskExecution::new(
            task_execution_id,
            task_id.clone(),
            ctx.clone(),
            attempt,
            now,
        );
        self.db.create_task_execution(&execution)?;

        // Draft → Ready before the runner sets Running.
        let mut task = task;
        if task.status == TaskStatus::Draft {
            task.status = TaskStatus::Ready;
            task.updated_at = now;
            self.db.update_task(&task)?;
        }

        let token = CancellationToken::new();
        self.active
            .lock()
            .insert(execution.id.to_string(), token.clone());

        let task_id_str = task_id.to_string();
        let execution_id_str = execution.id.to_string();
        let execution_id_owned = execution.id.clone();
        let orchestrator = self.builder.build().await;
        let task_id_owned = task_id.clone();
        let active = Arc::clone(&self.active);
        let exec_key = execution_id_str.clone();
        tokio::spawn(async move {
            let owner = crate::execution::ExecutionOwner::LegacyTask {
                task_id: task_id_owned.to_string(),
                execution_id: execution_id_owned.to_string(),
            };
            let result = crate::execution::owner::CURRENT_EXECUTION_OWNER
                .scope(
                    owner,
                    orchestrator.run(&task_id_owned, &execution_id_owned, &token),
                )
                .await;
            active.lock().remove(&exec_key);
            if let Err(error) = result {
                tracing::error!(
                    task_id = %task_id_owned,
                    execution_id = %execution_id_owned,
                    error = %error,
                    "task execution ended with an error"
                );
            }
        });

        Ok(TaskStartResponse {
            task_id: task_id_str,
            task_execution_id: execution_id_str,
            execution_id: ctx.execution_id.to_string(),
            status: "running".to_string(),
        })
    }

    /// Retry: requires a terminal task; a new attempt is created.
    pub async fn retry(&self, task_id: &TaskId) -> Result<TaskStartResponse, String> {
        let task = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| "task not found".to_string())?;
        if !task.status.is_terminal() {
            return Err(format!("task is not in a retryable state: {}", task.status));
        }
        self.start(task_id).await
    }

    /// Cancel the current active execution of a task (if any) and persist the
    /// cancelled state immediately.
    pub async fn cancel(&self, task_id: &TaskId) -> Result<(), String> {
        let Some(active_execution) = self.db.find_active_task_execution(task_id)? else {
            return Ok(());
        };
        let execution_id = active_execution.id.clone();
        if let Some(token) = self.active.lock().get(execution_id.as_str()) {
            token.cancel();
        }
        self.active.lock().remove(execution_id.as_str());

        // Cancel any pending approval / decision bound to this execution.
        let now = chrono::Utc::now().timestamp_millis();
        let mut execution = active_execution;
        execution.status = TaskExecutionStatus::Cancelled;
        execution.finished_at = Some(now);
        execution.updated_at = now;
        self.db.update_task_execution(&execution)?;

        let mut task = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| "task not found".to_string())?;
        task.status = TaskStatus::Cancelled;
        task.updated_at = now;
        task.completed_at = None;
        self.db.update_task(&task)?;
        crate::modules::workflow::application::task_parent::cancel_legacy_child(
            &self.db, &execution, now,
        )?;
        Ok(())
    }
}

/// Server-side resolved owner subject for a task execution. The desktop user is
/// the local-user; this is never client-supplied.
fn task_owner_subject(db: &Database, _task: &Task) -> String {
    db.resolve_active_role_binding("local-user")
        .is_some()
        .then(|| "local-user".to_string())
        .unwrap_or_else(|| "local-user".to_string())
}

/// Unknown external effects require a fresh, explicitly reviewed delegation;
/// changing the old task status must not silently replay its last attempt.
fn ensure_replay_is_safe(executions: &[TaskExecution]) -> Result<(), String> {
    if let Some(latest) = executions
        .iter()
        .max_by_key(|e| (e.created_at, e.id.as_str()))
    {
        if latest.status == TaskExecutionStatus::Interrupted
            || latest
                .error
                .as_deref()
                .is_some_and(|e| e.contains("external_mutation_uncertain"))
        {
            return Err("Previous execution effects are uncertain. Verify the original execution before creating a new delegation; the old task was not replayed".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod integration_replay_tests {
    use super::*;
    #[test]
    fn integration_legacy_task_unknown_effects_cannot_be_started_or_retried() {
        let mut e = TaskExecution::new(
            TaskExecutionId::generate(),
            TaskId::generate(),
            ExecutionContext::new(ExecutionId::generate(), "local-user", "test", None, 1),
            1,
            1,
        );
        e.status = TaskExecutionStatus::Failed;
        e.error = Some("external_mutation_uncertain: response lost".into());
        assert!(ensure_replay_is_safe(&[e.clone()]).is_err());
        e.error = Some("definitive tool rejection".into());
        assert!(ensure_replay_is_safe(&[e.clone()]).is_ok());
        e.status = TaskExecutionStatus::Interrupted;
        assert!(ensure_replay_is_safe(&[e]).is_err());
    }
}
