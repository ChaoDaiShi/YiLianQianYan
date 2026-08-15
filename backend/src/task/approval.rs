// ============================================================
// Task-agent approval resolution — bounded, TOCTOU-safe resume.
//
// When a task agent pauses on a high-risk tool, approving/rejecting must:
//   - consume the approval exactly once,
//   - re-evaluate the *original* call through the Security Gateway
//     (current RBAC role, not the role at request time),
//   - mark the agent execution / task execution / task accordingly.
//
// This is a *bounded* resume: it executes the single approved tool call and
// finalizes the step. It does not re-enter a full ReAct loop (nested agent
// tool-use is out of scope for v0.6).
// ============================================================

use crate::db::Database;
use crate::safety::approval::ApprovalStore;
use crate::safety::execution_gateway::SecurityExecutionGateway;
use crate::safety::{SecurityExecutionRequest, SecuritySubject};
use crate::task::model::*;
use crate::task::timeline::TimelineService;

pub async fn resolve_task_agent_approval(
    gateway: &SecurityExecutionGateway,
    approval_store: &ApprovalStore,
    db: &Database,
    approval_id: &str,
    approve: bool,
) -> Result<(), String> {
    let approval = approval_store
        .get(approval_id)
        .ok_or_else(|| "approval not found".to_string())?;
    let subject_id = approval.subject_id.clone();
    let conversation_id = approval.conversation_id.clone();

    let consumed = if approve {
        approval_store.consume_for_approval(approval_id, &conversation_id)
    } else {
        approval_store.consume_for_rejection(approval_id, &conversation_id)
    }
    .map_err(|error| error.to_string())?;

    let task_id = TaskId::new(
        consumed
            .task_id
            .clone()
            .ok_or_else(|| "approval is not bound to a task agent".to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let task_execution_id = TaskExecutionId::new(
        consumed
            .task_execution_id
            .clone()
            .ok_or_else(|| "approval is not bound to a task execution".to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let agent_execution_id = AgentExecutionId::new(
        consumed
            .agent_execution_id
            .clone()
            .ok_or_else(|| "approval is not bound to an agent execution".to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let timeline = TimelineService::new(db.clone_connection());
    let mut task = db
        .get_task(&task_id)?
        .ok_or_else(|| "task not found".to_string())?;
    let mut task_execution = db
        .get_task_execution(&task_execution_id)?
        .ok_or_else(|| "task execution not found".to_string())?;
    let mut agent_execution = db
        .get_agent_execution(&agent_execution_id)?
        .ok_or_else(|| "agent execution not found".to_string())?;

    let now = chrono::Utc::now().timestamp_millis();

    let (agent_status, exec_status, task_status, summary) = if approve {
        let request = SecurityExecutionRequest {
            conversation_id: consumed.conversation_id.clone(),
            tool_call_id: consumed.tool_call_id.clone(),
            tool_name: consumed.tool_name.clone(),
            arguments: consumed.arguments.clone(),
            subject: SecuritySubject::from_subject_id(subject_id),
        };
        match gateway
            .execute_approved(&request, consumed.risk_level)
            .await
        {
            Ok(crate::safety::execution_gateway::SecurityExecutionOutcome::Executed {
                tool_result,
                ..
            }) if tool_result.ok => (
                AgentExecutionStatus::Completed,
                TaskExecutionStatus::Completed,
                TaskStatus::Completed,
                Some(crate::workflow::safe_tool_result_summary(
                    &tool_result.content,
                )),
            ),
            _ => (
                AgentExecutionStatus::Failed,
                TaskExecutionStatus::Failed,
                TaskStatus::Failed,
                None,
            ),
        }
    } else {
        (
            AgentExecutionStatus::Failed,
            TaskExecutionStatus::Failed,
            TaskStatus::Failed,
            None,
        )
    };

    agent_execution.status = agent_status;
    agent_execution.result_summary = summary;
    agent_execution.finished_at = Some(now);
    agent_execution.updated_at = now;
    if agent_execution.status == AgentExecutionStatus::Failed {
        agent_execution.error = Some(if approve {
            "审批通过后执行被安全策略拒绝或失败".to_string()
        } else {
            "审批被拒绝".to_string()
        });
    }
    db.update_agent_execution(&agent_execution)?;

    task_execution.status = exec_status;
    task_execution.finished_at = Some(now);
    task_execution.updated_at = now;
    if exec_status == TaskExecutionStatus::Failed {
        task_execution.error = Some(if approve {
            "审批通过后执行被安全策略拒绝或失败".to_string()
        } else {
            "审批被拒绝".to_string()
        });
    }
    db.update_task_execution(&task_execution)?;

    task.status = task_status;
    task.updated_at = now;
    if task_status == TaskStatus::Completed {
        task.completed_at = Some(now);
    }
    db.update_task(&task)?;

    timeline.record(
        &task.workspace_id,
        &task.id,
        Some(&task_execution.id),
        TaskEventType::ApprovalResolved,
        format!(
            "{}：{}",
            if approve {
                "审批通过"
            } else {
                "审批拒绝"
            },
            consumed.tool_name
        ),
        serde_json::json!({ "approval_id": approval_id }),
    )?;

    Ok(())
}
