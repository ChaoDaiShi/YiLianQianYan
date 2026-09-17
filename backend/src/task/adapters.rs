//! Narrow adapters from Task Harness attempts to existing domain runtimes.
//!
//! The adapters translate a resolved reference and return bounded evidence.
//! They do not own TaskGraph scheduling, duplicate WorkflowRunner, or invoke
//! native providers directly.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;

use crate::shared::command::{CommandRequest, CommandRouter, CommandStatus};
use crate::shared::event::{EventHub, YiEvent};

use super::execution::{NodeExecution, NodeExecutionStatus};
use super::executor::{ExecutorKind, ResolvedExecutionPlan};

const MAX_ADAPTER_ERROR_CHARS: usize = 2_000;
const MAX_ADAPTER_OUTPUT_CHARS: usize = 32_000;
const EVENT_SOURCE: &str = "v1-task-harness";

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutorDispatch {
    Completed { output: Option<Value> },
    WaitingApproval { approval_ref: String },
    Failed { code: String, error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdapterError {
    #[error("adapter kind mismatch: expected {expected:?}, got {actual:?}")]
    KindMismatch {
        expected: ExecutorKind,
        actual: ExecutorKind,
    },
    #[error("adapter provider is unavailable")]
    ProviderUnavailable,
    #[error("command dispatch failed: {0}")]
    Command(String),
    #[error("adapter output exceeds bounded limits")]
    OutputTooLarge,
    #[error("adapter execution failed: {0}")]
    Execution(String),
}

#[async_trait]
pub trait TaskExecutorAdapter: Send + Sync {
    fn kind(&self) -> ExecutorKind;

    async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError>;

    async fn cancel(&self, _execution: &NodeExecution) -> Result<(), AdapterError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn TaskExecutorAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<A>(&mut self, adapter: A) -> &mut Self
    where
        A: TaskExecutorAdapter + 'static,
    {
        self.adapters.push(Arc::new(adapter));
        self
    }

    pub fn with_adapter<A>(mut self, adapter: A) -> Self
    where
        A: TaskExecutorAdapter + 'static,
    {
        self.register(adapter);
        self
    }

    pub fn has_kind(&self, kind: ExecutorKind) -> bool {
        self.adapters.iter().any(|adapter| adapter.kind() == kind)
    }

    pub async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        let Some(adapter) = self
            .adapters
            .iter()
            .find(|adapter| adapter.kind() == plan.kind)
        else {
            return Err(AdapterError::ProviderUnavailable);
        };
        adapter.dispatch(plan, execution).await
    }
}

/// Command adapter boundary. It can only submit a registered command through
/// CommandRouter; SecurityExecutionGateway remains inside handlers/domains.
#[derive(Clone)]
pub struct CommandExecutor {
    router: CommandRouter,
}

impl CommandExecutor {
    pub fn new(router: CommandRouter) -> Self {
        Self { router }
    }
}

#[async_trait]
impl TaskExecutorAdapter for CommandExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::Command
    }

    async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        if plan.kind != ExecutorKind::Command {
            return Err(AdapterError::KindMismatch {
                expected: ExecutorKind::Command,
                actual: plan.kind,
            });
        }
        let payload = plan
            .command_binding
            .as_ref()
            .map(|binding| json!({"app_id": binding.args.app_id}))
            .unwrap_or_else(|| json!({}));
        let result = self.router.execute(CommandRequest::new(
            plan.provider.clone(),
            execution.id.to_string(),
            "task-harness",
            payload,
        ));
        match result.status {
            CommandStatus::Succeeded => {
                let output = result.result;
                if let Some(Value::Object(object)) = output.as_ref() {
                    if object.get("status").and_then(Value::as_str) == Some("waiting_approval") {
                        let approval_ref = object
                            .get("approval_id")
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                            .ok_or_else(|| {
                                AdapterError::Command("approval id is missing".into())
                            })?;
                        return Ok(ExecutorDispatch::WaitingApproval {
                            approval_ref: bound_text(approval_ref),
                        });
                    }
                }
                ensure_output_bound(output.as_ref())?;
                Ok(ExecutorDispatch::Completed { output })
            }
            CommandStatus::NotFound => Err(AdapterError::ProviderUnavailable),
            CommandStatus::Failed => {
                let error = result
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "registered command failed".to_string());
                Ok(ExecutorDispatch::Failed {
                    code: "provider_error".to_string(),
                    error: bound_text(&error),
                })
            }
        }
    }
}

#[async_trait]
pub trait WorkflowExecutionProvider: Send + Sync {
    async fn execute(
        &self,
        workflow_id: &str,
        context: &super::NodeContext,
    ) -> Result<Option<Value>, AdapterError>;
}

pub struct WorkflowExecutor {
    provider: Arc<dyn WorkflowExecutionProvider>,
}

impl WorkflowExecutor {
    pub fn new(provider: Arc<dyn WorkflowExecutionProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl TaskExecutorAdapter for WorkflowExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::Workflow
    }

    async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        if plan.kind != ExecutorKind::Workflow {
            return Err(AdapterError::KindMismatch {
                expected: ExecutorKind::Workflow,
                actual: plan.kind,
            });
        }
        let output = self
            .provider
            .execute(&plan.provider, &execution.context)
            .await?;
        ensure_output_bound(output.as_ref())?;
        Ok(ExecutorDispatch::Completed { output })
    }
}

#[async_trait]
pub trait CapabilityExecutionProvider: Send + Sync {
    async fn execute(
        &self,
        capability_id: &str,
        context: &super::NodeContext,
    ) -> Result<Option<Value>, AdapterError>;
}

pub struct CapabilityExecutor {
    provider: Option<Arc<dyn CapabilityExecutionProvider>>,
}

impl CapabilityExecutor {
    pub fn new(provider: Arc<dyn CapabilityExecutionProvider>) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    pub fn unavailable() -> Self {
        Self { provider: None }
    }
}

#[async_trait]
impl TaskExecutorAdapter for CapabilityExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::Capability
    }

    async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        if plan.kind != ExecutorKind::Capability {
            return Err(AdapterError::KindMismatch {
                expected: ExecutorKind::Capability,
                actual: plan.kind,
            });
        }
        let Some(provider) = &self.provider else {
            return Err(AdapterError::ProviderUnavailable);
        };
        let output = provider.execute(&plan.provider, &execution.context).await?;
        ensure_output_bound(output.as_ref())?;
        Ok(ExecutorDispatch::Completed { output })
    }
}

#[async_trait]
pub trait AgentExecutionProvider: Send + Sync {
    async fn execute(
        &self,
        agent_id: &str,
        context: &super::NodeContext,
    ) -> Result<Option<Value>, AdapterError>;
}

pub struct AgentExecutor {
    provider: Option<Arc<dyn AgentExecutionProvider>>,
}

impl AgentExecutor {
    pub fn new(provider: Arc<dyn AgentExecutionProvider>) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    pub fn unavailable() -> Self {
        Self { provider: None }
    }
}

#[async_trait]
impl TaskExecutorAdapter for AgentExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::Agent
    }

    async fn dispatch(
        &self,
        plan: &ResolvedExecutionPlan,
        execution: &NodeExecution,
    ) -> Result<ExecutorDispatch, AdapterError> {
        if plan.kind != ExecutorKind::Agent {
            return Err(AdapterError::KindMismatch {
                expected: ExecutorKind::Agent,
                actual: plan.kind,
            });
        }
        let Some(provider) = &self.provider else {
            return Err(AdapterError::ProviderUnavailable);
        };
        let output = provider.execute(&plan.provider, &execution.context).await?;
        ensure_output_bound(output.as_ref())?;
        Ok(ExecutorDispatch::Completed { output })
    }
}

/// Product event facade. Payloads contain only stable bounded facts and never
/// raw provider logs, arbitrary output, secrets, PIDs or native handles.
#[derive(Clone)]
pub struct TaskEventPublisher {
    events: EventHub,
}

impl TaskEventPublisher {
    pub fn new(events: EventHub) -> Self {
        Self { events }
    }

    pub fn publish(
        &self,
        event_type: &str,
        graph_id: &str,
        node_id: &str,
        execution: Option<&NodeExecution>,
    ) {
        let payload = json!({
            "graph_id": bound_text(graph_id),
            "node_id": bound_text(node_id),
            "execution_id": execution.map(|value| value.id.to_string()),
            "attempt": execution.map(|value| value.attempt),
            "status": execution.map(|value| value.status),
        });
        let _ = self
            .events
            .publish(YiEvent::new(event_type, EVENT_SOURCE, payload));
    }

    pub fn execution_created(&self, execution: &NodeExecution) {
        self.publish(
            "task.execution.created",
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            Some(execution),
        );
    }

    pub fn execution_waiting_approval(&self, execution: &NodeExecution) {
        self.publish(
            "task.execution.waiting_approval",
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            Some(execution),
        );
    }

    pub fn execution_validating(&self, execution: &NodeExecution) {
        self.publish(
            "task.execution.validating",
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            Some(execution),
        );
    }

    pub fn execution_finished(&self, execution: &NodeExecution) {
        let event_type = match execution.status {
            NodeExecutionStatus::Succeeded => "task.execution.succeeded",
            NodeExecutionStatus::Cancelled => "task.execution.cancelled",
            _ => "task.execution.failed",
        };
        self.publish(
            event_type,
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            Some(execution),
        );
    }
}

fn ensure_output_bound(output: Option<&Value>) -> Result<(), AdapterError> {
    if output.is_some_and(|value| value.to_string().chars().count() > MAX_ADAPTER_OUTPUT_CHARS) {
        return Err(AdapterError::OutputTooLarge);
    }
    Ok(())
}

fn bound_text(value: &str) -> String {
    value.chars().take(MAX_ADAPTER_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{
        ExecutionRetryPolicy, ExecutorRef, GraphRevision, NodeContext, NodeExecutionId,
        TaskGraphId, TaskNodeId,
    };
    use serde_json::json;

    fn execution(reference: &str) -> NodeExecution {
        NodeExecution::new(
            NodeExecutionId::new("adapter-execution").unwrap(),
            TaskGraphId::new("adapter-graph").unwrap(),
            TaskNodeId::new("node").unwrap(),
            1,
            Some(ExecutorRef::new(reference).unwrap()),
            NodeContext::new(
                "goal",
                "instruction",
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .unwrap(),
            1,
        )
        .unwrap()
        .with_retry_policy(ExecutionRetryPolicy::default())
        .unwrap()
    }

    #[tokio::test]
    async fn command_adapter_dispatches_only_registered_commands() {
        let router = CommandRouter::new();
        router
            .register("core.echo", |request| Ok(request.payload.clone()))
            .unwrap();
        let adapter = CommandExecutor::new(router);
        let plan = ResolvedExecutionPlan {
            executor_ref: ExecutorRef::new("command://core.echo").unwrap(),
            kind: ExecutorKind::Command,
            provider: "core.echo".to_string(),
            command_binding: None,
        };
        let result = adapter
            .dispatch(&plan, &execution("command://core.echo"))
            .await;
        assert!(matches!(result, Ok(ExecutorDispatch::Completed { .. })));

        let missing = ResolvedExecutionPlan {
            executor_ref: ExecutorRef::new("command://missing.command").unwrap(),
            kind: ExecutorKind::Command,
            provider: "missing.command".to_string(),
            command_binding: None,
        };
        assert!(matches!(
            adapter
                .dispatch(&missing, &execution("command://missing.command"))
                .await,
            Err(AdapterError::ProviderUnavailable)
        ));
    }

    struct WorkflowProvider;

    #[async_trait]
    impl WorkflowExecutionProvider for WorkflowProvider {
        async fn execute(
            &self,
            _workflow_id: &str,
            _context: &super::super::NodeContext,
        ) -> Result<Option<Value>, AdapterError> {
            Ok(Some(json!({"status": "completed"})))
        }
    }

    #[tokio::test]
    async fn workflow_adapter_observes_existing_provider_result() {
        let adapter = WorkflowExecutor::new(Arc::new(WorkflowProvider));
        let plan = ResolvedExecutionPlan {
            executor_ref: ExecutorRef::new("workflow://report").unwrap(),
            kind: ExecutorKind::Workflow,
            provider: "report".to_string(),
            command_binding: None,
        };
        assert!(matches!(
            adapter
                .dispatch(&plan, &execution("workflow://report"))
                .await,
            Ok(ExecutorDispatch::Completed { .. })
        ));
    }

    #[tokio::test]
    async fn descriptor_only_capability_is_unavailable_and_agent_is_minimal() {
        let plan = ResolvedExecutionPlan {
            executor_ref: ExecutorRef::new("capability://descriptor").unwrap(),
            kind: ExecutorKind::Capability,
            provider: "descriptor".to_string(),
            command_binding: None,
        };
        assert!(matches!(
            CapabilityExecutor::unavailable()
                .dispatch(&plan, &execution("capability://descriptor"))
                .await,
            Err(AdapterError::ProviderUnavailable)
        ));

        struct AgentProvider;
        #[async_trait]
        impl AgentExecutionProvider for AgentProvider {
            async fn execute(
                &self,
                _agent_id: &str,
                _context: &super::super::NodeContext,
            ) -> Result<Option<Value>, AdapterError> {
                Ok(Some(json!({"summary": "agent result"})))
            }
        }
        let plan = ResolvedExecutionPlan {
            executor_ref: ExecutorRef::new("agent://writer").unwrap(),
            kind: ExecutorKind::Agent,
            provider: "writer".to_string(),
            command_binding: None,
        };
        assert!(matches!(
            AgentExecutor::new(Arc::new(AgentProvider))
                .dispatch(&plan, &execution("agent://writer"))
                .await,
            Ok(ExecutorDispatch::Completed { .. })
        ));
    }

    #[test]
    fn publisher_payload_is_bounded_and_has_product_event_names() {
        let publisher = TaskEventPublisher::new(EventHub::new(4));
        let execution = execution("agent://writer");
        publisher.execution_created(&execution);
        assert_eq!(
            execution.graph_id,
            TaskGraphId::new("adapter-graph").unwrap()
        );
        let _ = GraphRevision::initial();
    }
}
