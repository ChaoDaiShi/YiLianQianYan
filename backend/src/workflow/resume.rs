// ============================================================
// Workflow approval resume — secure continuation of a paused run.
//
// Resuming consumes a workflow-bound approval exactly once, reloads the run
// from its persisted snapshot, re-evaluates the original call through the
// gateway (TOCTOU-safe), and only then continues the scheduler. A stale
// definition or a changed subject role is handled by the gateway's own
// re-evaluation, never by trusting the original decision blindly.
// ============================================================

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::definition::WorkflowNodeId;
use super::executor::SecurityGatewayNodeExecutor;
use super::run::{NodeRunStatus, WorkflowRunId};
use super::runner::WorkflowRunner;
use crate::db::Database;
use crate::safety::execution_gateway::SecurityExecutionOutcome;
use crate::safety::{
    ApprovalStore, SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject,
};

/// Resolve a workflow approval (approve or reject) and, on approve, resume the
/// paused run.
pub async fn resolve_workflow_approval(
    gateway: &Arc<SecurityExecutionGateway>,
    approval_store: &Arc<ApprovalStore>,
    db: &Database,
    approval_id: &str,
    subject_id: &str,
    approve: bool,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let lookup = approval_store
        .get(approval_id)
        .ok_or_else(|| "审批不存在".to_string())?;

    // The subject must match the one that initiated the approval.
    if lookup.subject_id != subject_id {
        return Err("审批不属于当前安全主体".to_string());
    }

    let workflow_run_id = lookup
        .workflow_run_id
        .clone()
        .ok_or_else(|| "审批未绑定工作流运行".to_string())?;
    let workflow_node_id = lookup
        .workflow_node_id
        .clone()
        .ok_or_else(|| "审批未绑定工作流节点".to_string())?;

    // Consume exactly once (replay protection).
    let approval = if approve {
        approval_store.consume_for_approval(approval_id, &lookup.conversation_id)
    } else {
        approval_store.consume_for_rejection(approval_id, &lookup.conversation_id)
    }
    .map_err(|error| error.to_string())?;

    // Reload the run from its persisted snapshot (never the live definition).
    let run_id = WorkflowRunId::new(workflow_run_id).map_err(|error| error.to_string())?;
    let stored = db
        .get_workflow_run(&run_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "工作流运行不存在".to_string())?;
    let (graph_id, mut run) = (stored.workflow_graph_id, stored.run);

    let node_id = WorkflowNodeId::new(workflow_node_id).map_err(|error| error.to_string())?;
    if run.node(&node_id).is_none() {
        return Err("工作流节点不存在".to_string());
    }

    let now = chrono::Utc::now().timestamp_millis();
    if approve {
        // Re-evaluate and execute the original approved call (TOCTOU-safe).
        let request = SecurityExecutionRequest {
            conversation_id: approval.conversation_id.clone(),
            tool_call_id: approval.tool_call_id.clone(),
            tool_name: approval.tool_name.clone(),
            arguments: approval.arguments.clone(),
            subject: SecuritySubject::from_subject_id(approval.subject_id.clone()),
        };
        let outcome = gateway
            .execute_approved(&request, approval.risk_level)
            .await;

        run.transition_node(&node_id, NodeRunStatus::Running, now)
            .map_err(|error| error.to_string())?;
        match outcome {
            Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) if tool_result.ok => {
                run.transition_node(&node_id, NodeRunStatus::Completed, now)
                    .map_err(|error| error.to_string())?;
            }
            _ => {
                run.transition_node(&node_id, NodeRunStatus::Failed, now)
                    .map_err(|error| error.to_string())?;
            }
        }
    } else {
        // Reject: the execution path cannot continue — fail the node and run.
        run.transition_node(&node_id, NodeRunStatus::Running, now)
            .map_err(|error| error.to_string())?;
        run.transition_node(&node_id, NodeRunStatus::Failed, now)
            .map_err(|error| error.to_string())?;
    }
    db.update_workflow_run(&graph_id, &run)
        .map_err(|error| error.to_string())?;

    // Continue the scheduler for any remaining nodes (approve only).
    if approve {
        let executor =
            SecurityGatewayNodeExecutor::new(Arc::clone(gateway), Arc::clone(approval_store));
        let runner = WorkflowRunner::new(executor);
        let db = db.clone_connection();
        runner
            .run(&mut run, cancel, move |run| {
                db.update_workflow_run(&graph_id, run)
            })
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Cancel a workflow approval: consume it once and mark the bound run cancelled.
pub async fn cancel_workflow_approval(
    approval_store: &Arc<ApprovalStore>,
    db: &Database,
    approval_id: &str,
    subject_id: &str,
) -> Result<(), String> {
    let lookup = approval_store
        .get(approval_id)
        .ok_or_else(|| "审批不存在".to_string())?;
    if lookup.subject_id != subject_id {
        return Err("审批不属于当前安全主体".to_string());
    }
    let workflow_run_id = lookup
        .workflow_run_id
        .clone()
        .ok_or_else(|| "审批未绑定工作流运行".to_string())?;
    let workflow_node_id = lookup
        .workflow_node_id
        .clone()
        .ok_or_else(|| "审批未绑定工作流节点".to_string())?;

    let _approval = approval_store
        .cancel(approval_id, &lookup.conversation_id)
        .map_err(|error| error.to_string())?;

    let run_id = WorkflowRunId::new(workflow_run_id).map_err(|error| error.to_string())?;
    let stored = db
        .get_workflow_run(&run_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "工作流运行不存在".to_string())?;
    let (graph_id, mut run) = (stored.workflow_graph_id, stored.run);

    let node_id = WorkflowNodeId::new(workflow_node_id).map_err(|error| error.to_string())?;
    if run.node(&node_id).is_none() {
        return Err("工作流节点不存在".to_string());
    }

    run.cancel(chrono::Utc::now().timestamp_millis());
    db.update_workflow_run(&graph_id, &run)
        .map_err(|error| error.to_string())?;
    Ok(())
}
