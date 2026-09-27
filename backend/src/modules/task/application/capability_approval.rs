//! Resume exactly the approved Task Harness call, never a later Canvas draft.
use super::super::validation::ValidationPolicy;
use super::super::{ExecutorDispatch, NodeExecution, NodeExecutionId, NodeExecutionStatus};
use crate::app::state::AppServer;
use crate::safety::{
    ApprovalStatus, AuditEventInput, AuditEventType, PendingApproval, SecurityExecutionRequest,
    SecuritySubject,
};
use serde_json::json;
use std::sync::Arc;

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn bound_execution(
    server: &AppServer,
    approval: &PendingApproval,
) -> Result<NodeExecution, String> {
    let id = NodeExecutionId::new(
        approval
            .node_execution_id
            .as_deref()
            .ok_or("InvalidBinding")?,
    )
    .map_err(|_| "InvalidBinding")?;
    let (_, execution) = server
        .task_world
        .find_execution(&id)
        .ok_or("task_execution_unavailable")?;
    if approval.task_graph_id.as_deref() != Some(execution.graph_id.as_str())
        || approval.task_node_id.as_deref() != Some(execution.node_id.as_str())
        || execution.approval_ref.as_deref() != Some(&approval.approval_id)
        || execution.status != NodeExecutionStatus::WaitingApproval
    {
        return Err("task_execution_not_waiting_for_this_approval".into());
    }
    Ok(execution)
}
fn fail(
    server: &AppServer,
    execution: &NodeExecution,
    code: &str,
    message: &str,
) -> Result<NodeExecution, String> {
    if let Some((_, current)) = server.task_world.find_execution(&execution.id) {
        if current.status == NodeExecutionStatus::Cancelled {
            return Ok(current);
        }
    }
    server
        .task_world
        .fail_execution(&execution.graph_id, &execution.id, code, message, now())
        .map_err(|e| e.to_string())
}
fn record_decision(server: &AppServer, approval: &PendingApproval) -> Result<(), String> {
    server.audit_recorder.record(AuditEventInput{
        event_type:AuditEventType::ApprovalResolved,correlation_id:approval.tool_call_id.clone(),request_id:approval.tool_call_id.clone(),
        subject_id:approval.subject_id.clone(),role_key:server.db.resolve_active_role_binding(&approval.subject_id).unwrap_or_else(||"restricted".into()),
        conversation_id:Some(approval.conversation_id.clone()),tool_call_id:Some(approval.tool_call_id.clone()),tool_name:Some(approval.tool_name.clone()),
        risk_level:Some(approval.risk_level.to_string()),decision_status:Some(approval.status.to_string()),
        details:json!({"approval_id":approval.approval_id,"target":"task_node_execution","task_graph_id":approval.task_graph_id,"task_node_id":approval.task_node_id,"node_execution_id":approval.node_execution_id}),
        ..Default::default()
    }).map(|_|()).map_err(|_|"approval_audit_failed".into())
}
pub async fn resolve(
    server: Arc<AppServer>,
    approval: &PendingApproval,
    approve: bool,
) -> Result<NodeExecution, String> {
    if approval.status != ApprovalStatus::Pending {
        return Err("approval_already_processed".into());
    }
    let execution = match bound_execution(&server, approval) {
        Ok(execution) => execution,
        Err(error) => {
            // A cancelled/restarted/retired attempt can never approve a later remote call.
            let _ = server
                .approval_store
                .cancel(&approval.approval_id, &approval.conversation_id);
            return Err(error);
        }
    };
    let ownership = server
        .task_world
        .claim_approval_dispatch(&execution.graph_id, &execution.id)
        .map_err(|e| e.to_string())?;
    let cancel = ownership.cancellation_token();
    let consumed = if approve {
        server
            .approval_store
            .consume_for_approval(&approval.approval_id, &approval.conversation_id)
    } else {
        server
            .approval_store
            .consume_for_rejection(&approval.approval_id, &approval.conversation_id)
    }
    .map_err(|e| e.to_string())?;
    if record_decision(&server, &consumed).is_err() {
        return fail(
            &server,
            &execution,
            "approval_audit_failed",
            "审批审计不可用，未执行远程工具",
        );
    }
    if !approve {
        return fail(
            &server,
            &execution,
            "approval_rejected",
            "用户拒绝了远程工具调用",
        );
    }
    let capability_id = execution
        .executor_ref
        .as_ref()
        .and_then(|reference| reference.as_str().strip_prefix("capability://"))
        .ok_or("InvalidBinding")?;
    if consumed
        .tool_name
        .strip_prefix("mcp_")
        .map(|suffix| format!("mcp.{suffix}"))
        != Some(capability_id.to_string())
    {
        return fail(
            &server,
            &execution,
            "invalid_binding",
            "审批工具与执行能力不匹配",
        );
    }
    let gateway = match super::capability_execution::gateway(&server, &consumed.tool_name).await {
        Ok(gateway) => gateway,
        Err(_) => {
            return fail(
                &server,
                &execution,
                "provider_unavailable",
                "能力当前不可用",
            )
        }
    };
    if cancel.is_cancelled() {
        return Err("task_execution_cancelled".into());
    }
    // Frozen arguments/subject from the consumed approval, not current node.input.
    let request = SecurityExecutionRequest {
        conversation_id: consumed.conversation_id,
        tool_call_id: consumed.tool_call_id,
        tool_name: consumed.tool_name,
        arguments: consumed.arguments,
        subject: SecuritySubject::from_subject_id(consumed.subject_id),
    };
    let outcome = tokio::select! {biased;
        _=cancel.cancelled()=>return Err("task_execution_cancelled".into()),
        outcome=gateway.execute_approved(&request,consumed.risk_level)=>outcome,
    };
    if let Some((_, current)) = server.task_world.find_execution(&execution.id) {
        if current.status == NodeExecutionStatus::Cancelled {
            return Ok(current);
        }
    }
    let result = outcome
        .map_err(|_| "security_gateway_error".to_string())
        .and_then(|outcome| {
            super::capability_execution::completed_result(capability_id, outcome)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(ExecutorDispatch::Completed {
            output: Some(output),
        }) => server
            .task_world
            .complete_execution(
                &execution.graph_id,
                &execution.id,
                output,
                ValidationPolicy::StructuredResult,
                now(),
            )
            .map_err(|e| e.to_string()),
        Ok(ExecutorDispatch::Failed { code, error }) => fail(&server, &execution, &code, &error),
        _ => fail(
            &server,
            &execution,
            "capability_execution_failed",
            "远程工具未返回可验证的结果",
        ),
    }
}
pub fn cancel(server: &AppServer, approval: &PendingApproval) -> Result<NodeExecution, String> {
    let execution = bound_execution(server, approval)?;
    let consumed = server
        .approval_store
        .cancel(&approval.approval_id, &approval.conversation_id)
        .map_err(|e| e.to_string())?;
    let _ = record_decision(server, &consumed);
    server
        .task_world
        .update_execution(
            &execution.graph_id,
            &execution.id,
            NodeExecutionStatus::Cancelled,
            None,
            now(),
        )
        .map_err(|e| e.to_string())
}
