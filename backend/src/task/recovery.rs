// ============================================================
// Recovery — on startup, any execution left "running" by a previous process
// must not keep pretending to run. They become Interrupted, and their tasks
// become Blocked (a safe state the user can Retry / Resume from).
// ============================================================

use crate::db::Database;
use crate::task::model::{TaskEventType, TaskStatus};
use crate::task::timeline::TimelineService;

#[derive(Debug, Clone, Default)]
pub struct RecoveryReport {
    pub interrupted_executions: usize,
    pub interrupted_agent_executions: usize,
    pub blocked_tasks: usize,
}

pub fn recover_interrupted(db: &Database) -> Result<RecoveryReport, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut report = RecoveryReport::default();

    report.interrupted_agent_executions = db.interrupt_running_agent_executions(now)?;
    report.interrupted_executions = db.interrupt_running_executions(now)?;

    // Block every task whose latest execution was interrupted (or that is still
    // marked running / waiting).
    let timeline = TimelineService::new(db.clone_connection());
    for task in db.list_tasks(&Default::default())? {
        if matches!(
            task.status,
            TaskStatus::Running | TaskStatus::WaitingApproval | TaskStatus::WaitingUser
        ) {
            let mut blocked = task.clone();
            blocked.status = TaskStatus::Blocked;
            blocked.updated_at = now;
            db.update_task(&blocked)?;
            report.blocked_tasks += 1;
            let _ = timeline.record(
                &task.workspace_id,
                &task.id,
                None,
                TaskEventType::ExecutionInterrupted,
                "上次运行因应用退出而中断",
                serde_json::json!({}),
            );
        }
    }

    Ok(report)
}
