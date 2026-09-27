//! Production Task capability dispatch. Only the Gateway may invoke the Tool.
use super::super::{AdapterError, CapabilityExecutionProvider, ExecutorDispatch, NodeExecution};
use crate::agent::verifier::DefaultVerifier;
use crate::app::state::AppServer;
use crate::safety::execution_gateway::SecurityExecutionOutcome;
use crate::safety::{SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject};
use crate::tools::trait_def::RiskLevel;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct ExistingCapabilityProvider {
    pub server: Arc<AppServer>,
    pub cancel: CancellationToken,
}

pub async fn gateway(
    server: &AppServer,
    tool_name: &str,
) -> Result<SecurityExecutionGateway, AdapterError> {
    let registry = server.build_agent_tool_registry().await;
    let tool = registry
        .get(tool_name)
        .ok_or(AdapterError::ProviderUnavailable)?;
    // Recheck at approval resume too: discovery/schema may have changed while
    // the user was deciding. Never promote persisted arguments into headers.
    if super::capability_binding::has_header_binding(&tool.parameters()) {
        return Err(AdapterError::ProviderUnavailable);
    }
    // Reject any non-MCP mapping even if a descriptor were malformed.
    let descriptor = tool
        .security_descriptor(&json!({}))
        .map_err(|_| AdapterError::ProviderUnavailable)?;
    if tool.risk_level() != RiskLevel::High
        || !descriptor
            .resources
            .iter()
            .any(|r| matches!(r, crate::safety::ResourceDescriptor::Mcp { .. }))
    {
        return Err(AdapterError::ProviderUnavailable);
    }
    let config = server.config.read().clone();
    Ok(
        SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config.sandbox,
            server.workspace_root.clone(),
            registry,
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(server.db.clone_connection()))
        .with_grant_enforcement(),
    )
}

pub fn completed_result(
    capability_id: &str,
    outcome: SecurityExecutionOutcome,
) -> Result<ExecutorDispatch, AdapterError> {
    match outcome {
        SecurityExecutionOutcome::Executed {
            tool_result,
            verification,
        } => {
            if !tool_result.ok || !verification.success {
                return Ok(ExecutorDispatch::Failed {
                    code: "capability_execution_failed".into(),
                    error: crate::utils::text::truncate_chars(
                        tool_result.error.as_deref().unwrap_or(&verification.reason),
                        2000,
                    ),
                });
            }
            let output = json!({"kind":"capability_result","capability_id":capability_id,"summary":crate::utils::text::truncate_chars(&tool_result.content,1000),"content":tool_result.content,"ok":true});
            if output.to_string().chars().count() > 32000 {
                return Err(AdapterError::OutputTooLarge);
            }
            Ok(ExecutorDispatch::Completed {
                output: Some(output),
            })
        }
        SecurityExecutionOutcome::Denied { .. } => Ok(ExecutorDispatch::Failed {
            code: "security_denied".into(),
            error: "当前安全策略或授权拒绝此操作".into(),
        }),
        SecurityExecutionOutcome::RequiresApproval { .. } => {
            Err(AdapterError::Execution("approval_not_consumed".into()))
        }
    }
}
#[async_trait]
impl CapabilityExecutionProvider for ExistingCapabilityProvider {
    async fn dispatch(
        &self,
        capability_id: &str,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        if self.cancel.is_cancelled() {
            return Err(AdapterError::Execution("cancelled".into()));
        }
        let graph = self
            .server
            .task_world
            .get_graph(&execution.graph_id)
            .ok_or(AdapterError::ProviderUnavailable)?;
        let node = graph
            .node(&execution.node_id)
            .ok_or(AdapterError::ProviderUnavailable)?;
        let binding = super::capability_binding::resolve_binding(&self.server, &node.input)
            .await
            .map_err(|_| AdapterError::ProviderUnavailable)?;
        if binding.capability_id != capability_id {
            return Err(AdapterError::ProviderUnavailable);
        }
        let gateway = gateway(&self.server, &binding.tool_registry_name).await?;
        let request = SecurityExecutionRequest {
            conversation_id: format!("task-node:{}", execution.id),
            tool_call_id: execution.id.to_string(),
            tool_name: binding.tool_registry_name,
            arguments: binding.arguments,
            subject: SecuritySubject::local_user(),
        };
        let outcome = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(AdapterError::Execution("cancelled".into())),
            result=gateway.execute(&request,RiskLevel::High)=>result.map_err(|_|AdapterError::Execution("security_gateway_error".into()))?,
        };
        match outcome {
            SecurityExecutionOutcome::RequiresApproval { risk_level, reason } => {
                let approval = self.server.approval_store.create_task_node(
                    execution.graph_id.to_string(),
                    execution.node_id.to_string(),
                    execution.id.to_string(),
                    request.tool_call_id,
                    request.tool_name,
                    request.arguments,
                    risk_level,
                    reason,
                    request.subject.subject_id,
                );
                if self.cancel.is_cancelled() {
                    let _ = self
                        .server
                        .approval_store
                        .cancel(&approval.approval_id, &approval.conversation_id);
                    return Err(AdapterError::Execution("cancelled".into()));
                }
                Ok(ExecutorDispatch::WaitingApproval {
                    approval_ref: approval.approval_id,
                })
            }
            other => completed_result(capability_id, other),
        }
    }
}
