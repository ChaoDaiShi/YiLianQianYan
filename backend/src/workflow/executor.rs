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

use super::definition::{WorkflowCondition, WorkflowNodeConfig, WorkflowNodeDefinition};
use super::run::{safe_tool_result_summary, NodeRunResult, WorkflowRunError, WorkflowRunId};
use crate::config::types::ModelConfig;
use crate::execution::ExecutionContext;
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::safety::execution_gateway::{SecurityExecutionOutcome, SecurityGatewayError};
use crate::safety::{
    ApprovalStore, SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject,
};
use crate::secret::SecretResolver;
use crate::tools::RiskLevel;

/// The result of executing a single node.
///
/// `Completed` carries a bounded, UI-safe result (if any); `Failed` carries an
/// optional safe error summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeExecutionOutcome {
    Completed { result: Option<NodeRunResult> },
    WaitingApproval { approval_id: String },
    Failed { error: Option<String> },
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
        run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError>;
}

/// Executes an LLM-only Agent node (no tools, no ReAct loop).
///
/// Injected so tests can substitute a mock LLM. An absent executor keeps Agent
/// nodes fail-closed.
#[async_trait]
pub trait WorkflowAgentExecutor: Send + Sync {
    /// Generate a single text completion from a prompt. Must never produce tool
    /// calls.
    async fn generate(&self, prompt: &str) -> Result<String, WorkflowExecutionError>;
}

/// System prompt used for workflow Agent nodes. Explicitly instructs a
/// text-only answer and forbids tool use.
const WORKFLOW_AGENT_SYSTEM_PROMPT: &str =
    "你是工作流中的一个 Agent 节点。请只输出文字结果，不要调用任何工具，不要输出工具调用。直接完成任务要求。";

/// Production LLM-only agent executor backed by the configured chat model.
///
/// Uses the shared `LlmClient` / `ModelConfig` (provider, base_url, api key
/// resolver, timeout). Node config can never supply secrets.
pub struct LlmWorkflowAgentExecutor {
    llm: LlmClient,
}

impl LlmWorkflowAgentExecutor {
    pub fn new(config: &ModelConfig, resolver: Arc<SecretResolver>) -> Self {
        Self {
            llm: LlmClient::new(config, resolver),
        }
    }
}

#[async_trait]
impl WorkflowAgentExecutor for LlmWorkflowAgentExecutor {
    async fn generate(&self, prompt: &str) -> Result<String, WorkflowExecutionError> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(WORKFLOW_AGENT_SYSTEM_PROMPT.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let response = self
            .llm
            .invoke(&messages, &[])
            .await
            .map_err(|error| WorkflowExecutionError::Execution(error.to_string()))?;
        let choice =
            response.choices.into_iter().next().ok_or_else(|| {
                WorkflowExecutionError::Execution("empty LLM response".to_string())
            })?;
        let message = choice.message;
        if message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
        {
            return Err(WorkflowExecutionError::Execution(
                "workflow agent node returned unsupported tool calls".to_string(),
            ));
        }
        Ok(message.content.unwrap_or_default())
    }
}

/// Production executor that routes every side-effecting node through the
/// [`SecurityExecutionGateway`].
///
/// Tool and Subagent nodes never call the tool registry directly: they build a
/// [`SecurityExecutionRequest`] and let the gateway decide Allow / Approval /
/// Deny. The security subject is always derived from the run's
/// `execution_context.subject_id` — never defaulted to `local-user`.
///
/// When the gateway requires approval, a workflow-bound [`PendingApproval`] is
/// created so a later resume can locate the exact run and node.
pub struct SecurityGatewayNodeExecutor {
    gateway: Arc<SecurityExecutionGateway>,
    approval_store: Arc<ApprovalStore>,
    agent_executor: Option<Arc<dyn WorkflowAgentExecutor>>,
}

impl SecurityGatewayNodeExecutor {
    pub fn new(gateway: Arc<SecurityExecutionGateway>, approval_store: Arc<ApprovalStore>) -> Self {
        Self {
            gateway,
            approval_store,
            agent_executor: None,
        }
    }

    /// Attach an LLM-only agent executor so Agent nodes can run. Without it,
    /// Agent nodes fail closed.
    pub fn with_agent_executor(mut self, agent_executor: Arc<dyn WorkflowAgentExecutor>) -> Self {
        self.agent_executor = Some(agent_executor);
        self
    }

    async fn execute_tool(
        &self,
        context: &ExecutionContext,
        run_id: &WorkflowRunId,
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
                    // Persist a bounded, safe textual summary — never the raw
                    // payload (base64 images are replaced with a label).
                    Ok(NodeExecutionOutcome::Completed {
                        result: Some(NodeRunResult::new(safe_tool_result_summary(
                            &tool_result.content,
                        ))),
                    })
                } else {
                    Ok(NodeExecutionOutcome::Failed {
                        error: Some(
                            NodeRunResult::new(
                                tool_result.error.as_deref().unwrap_or("tool failed"),
                            )
                            .summary,
                        ),
                    })
                }
            }
            Ok(SecurityExecutionOutcome::RequiresApproval { risk_level, reason }) => {
                let approval = self.approval_store.create_workflow(
                    context.execution_id.to_string(),
                    run_id.to_string(),
                    node.id.to_string(),
                    request.tool_call_id.clone(),
                    request.tool_name.clone(),
                    request.arguments.clone(),
                    risk_level,
                    reason,
                    context.subject_id.clone(),
                );
                Ok(NodeExecutionOutcome::WaitingApproval {
                    approval_id: approval.approval_id,
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
        run_id: &WorkflowRunId,
        node: &WorkflowNodeDefinition,
    ) -> Result<NodeExecutionOutcome, WorkflowExecutionError> {
        match &node.config {
            WorkflowNodeConfig::Tool {
                tool_name,
                arguments,
            } => {
                self.execute_tool(context, run_id, node, tool_name, arguments)
                    .await
            }
            WorkflowNodeConfig::Subagent {
                subagent_name,
                task,
            } => {
                let tool_name = crate::tools::subagent::subagent_tool_name(subagent_name)
                    .ok_or_else(|| {
                        WorkflowExecutionError::Execution("invalid subagent name".to_string())
                    })?;
                let arguments = serde_json::json!({ "task": task });
                self.execute_tool(context, run_id, node, &tool_name, &arguments)
                    .await
            }
            // Conditions are deliberately trivial: the node only becomes Ready
            // once all dependencies completed, so both variants proceed.
            WorkflowNodeConfig::Condition { when } => Ok(NodeExecutionOutcome::Completed {
                result: Some(NodeRunResult::new(match when {
                    WorkflowCondition::Always => "条件满足：always",
                    WorkflowCondition::PreviousSucceeded => "条件满足：previous_succeeded",
                })),
            }),
            WorkflowNodeConfig::Output { template } => {
                let summary = template
                    .as_deref()
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| "工作流执行完成".to_string());
                Ok(NodeExecutionOutcome::Completed {
                    result: Some(NodeRunResult::new(summary)),
                })
            }
            // Agent nodes are LLM-only: a single text generation, no tools, no
            // ReAct loop, no approval, and no Tool Registry access. If no agent
            // executor was injected, fail closed.
            WorkflowNodeConfig::Agent { prompt } => {
                let Some(agent) = &self.agent_executor else {
                    return Err(WorkflowExecutionError::Execution(
                        "agent node execution is not supported".to_string(),
                    ));
                };
                let text = agent.generate(prompt).await?;
                Ok(NodeExecutionOutcome::Completed {
                    result: Some(NodeRunResult::new(text)),
                })
            }
        }
    }
}
