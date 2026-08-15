// ============================================================
// Workflow node executor — the execution boundary for a single node.
//
// Executors NEVER modify the [`super::run::WorkflowRun`] directly; they return
// an outcome and let the runner decide the resulting state transition. This
// keeps scheduling and security concerns in one place (the runner).
// ============================================================

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use super::definition::{WorkflowNodeConfig, WorkflowNodeDefinition};
use super::run::WorkflowRunError;
use crate::execution::ExecutionContext;
use crate::safety::execution_gateway::{SecurityExecutionOutcome, SecurityGatewayError};
use crate::safety::{SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject};
use crate::tools::RiskLevel;

/// The result of executing a single node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeExecutionOutcome {
    Completed,
    WaitingApproval { approval_id: String },
    Failed,
}

/// An execution-layer error (safe, non-leaking).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkflowExecutionError {
    #[error("workflow node execution failed: {0}")]
    Execution(String),
    #[error("workflow run persistence failed: {0}")]
    Persistence(String),
    #[error("workflow run state error: {0}")]
    State(#[from] WorkflowRunError),
}

/// Executes a single workflow node.
#[async_trait]
pub trait WorkflowNodeExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &ExecutionContext,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError>;
}

/// Production executor that routes every side-effecting node through the
/// [`SecurityExecutionGateway`].
///
/// Tool and Subagent nodes never call the tool registry directly: they build a
/// [`SecurityExecutionRequest`] and let the gateway decide Allow / Approval /
/// Deny. The security subject is always derived from the run's
/// `execution_context.subject_id` — never defaulted to `local-user`.
pub struct SecurityGatewayNodeExecutor {
    gateway: Arc<SecurityExecutionGateway>,
}

impl SecurityGatewayNodeExecutor {
    pub fn new(gateway: Arc<SecurityExecutionGateway>) -> Self {
        Self { gateway }
    }

    async fn execute_tool(
        &self,
        context: &ExecutionContext,
        node: &WorkflowNodeDefinition,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        let request = SecurityExecutionRequest {
            conversation_id: context.execution_id.to_string(),
            tool_call_id: format!("{}:{}", context.execution_id, node.id),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
            subject: SecuritySubject::from_subject_id(context.subject_id.clone()),
        };

        match self.gateway.execute(&request, RiskLevel::Low).await {
            Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) => {
                if tool_result.ok {
                    Ok(NodeExecutionOutcome::Completed)
                } else {
                    Ok(NodeExecutionOutcome::Failed)
                }
            }
            Ok(SecurityExecutionOutcome::RequiresApproval { .. }) => {
                // The real approval binding lands in Step 7F; for now the
                // tool-call id (unique per run + node) is the placeholder.
                Ok(NodeExecutionOutcome::WaitingApproval {
                    approval_id: request.tool_call_id.clone(),
                })
            }
            Ok(SecurityExecutionOutcome::Denied { reason }) => {
                Err(WorkflowExecutionError::Execution(reason))
            }
            Err(SecurityGatewayError::ToolNotFound(name)) => Err(
                WorkflowExecutionError::Execution(format!("tool not found: {name}")),
            ),
            Err(error) => Err(WorkflowExecutionError::Execution(error.to_string())),
        }
    }
}

#[async_trait]
impl WorkflowNodeExecutor for SecurityGatewayNodeExecutor {
    async fn execute(
        &self,
        context: &ExecutionContext,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        match &node.config {
            WorkflowNodeConfig::Tool {
                tool_name,
                arguments,
            } => self.execute_tool(context, node, tool_name, arguments).await,
            WorkflowNodeConfig::Subagent {
                subagent_name,
                task,
            } => {
                let tool_name = crate::tools::subagent::subagent_tool_name(subagent_name)
                    .ok_or_else(|| {
                        WorkflowExecutionError::Execution("invalid subagent name".to_string())
                    })?;
                let arguments = serde_json::json!({ "task": task });
                self.execute_tool(context, node, &tool_name, &arguments)
                    .await
            }
            // Conditions are deliberately trivial: the node only becomes Ready
            // once all dependencies completed, so both variants proceed.
            WorkflowNodeConfig::Condition { .. } => Ok(NodeExecutionOutcome::Completed),
            WorkflowNodeConfig::Output { .. } => Ok(NodeExecutionOutcome::Completed),
            // Agent node execution is not wired in v0.4.0 — fail closed.
            WorkflowNodeConfig::Agent { .. } => Err(WorkflowExecutionError::Execution(
                "agent node execution is not supported".to_string(),
            )),
        }
    }
}
