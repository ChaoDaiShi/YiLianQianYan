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
        let attempt = existing.len() as u32 + 1;

        let now = chrono::Utc::now().timestamp_millis();
        let ctx = ExecutionContext::new(
            ExecutionId::generate(),
            task_owner_subject(&self.db, &task),
            "task-runner",
            None,
            now,
        );
        let execution = TaskExecution::new(
            TaskExecutionId::generate(),
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
            let result = orchestrator
                .run(&task_id_owned, &execution_id_owned, &token)
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
