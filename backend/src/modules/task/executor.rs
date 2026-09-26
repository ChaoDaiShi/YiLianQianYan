//! Resolver and adapter contracts for Task Harness execution.
//!
//! Resolution is deliberately a pure availability/binding check.  It never
//! invokes a provider, native command, workflow runner, or agent loop.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::executor_ref::{validate_command_binding, CommandBinding, ExecutorRef};
use super::TaskNode;

/// The narrow provider family selected by an [`ExecutorRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    Agent,
    Workflow,
    Capability,
    Command,
}

/// A validated provider selection. It contains no provider handle and has no
/// side effects; adapters consume this value only after the Task Harness has
/// persisted an execution attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedExecutionPlan {
    pub executor_ref: ExecutorRef,
    pub kind: ExecutorKind,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_binding: Option<CommandBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutorResolutionError {
    #[error("task node has no executor_ref")]
    MissingExecutorRef,
    #[error("executor provider is not registered: {scheme}://{target}")]
    ProviderUnavailable { scheme: String, target: String },
    #[error("executor reference is invalid: {0}")]
    InvalidExecutorRef(String),
    #[error("command binding is invalid: {0}")]
    InvalidCommandBinding(String),
}

/// Registry-backed resolver. The registry is intentionally explicit: a
/// capability descriptor, agent name, workflow id, or command string is not
/// executable until the corresponding existing provider registers it here.
#[derive(Debug, Clone, Default)]
pub struct ExecutorResolver {
    agents: BTreeSet<String>,
    workflows: BTreeSet<String>,
    capabilities: BTreeSet<String>,
    commands: BTreeSet<String>,
}

impl ExecutorResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_agent(&mut self, provider: impl Into<String>) -> &mut Self {
        self.agents.insert(provider.into());
        self
    }

    pub fn register_workflow(&mut self, provider: impl Into<String>) -> &mut Self {
        self.workflows.insert(provider.into());
        self
    }

    pub fn register_capability(&mut self, provider: impl Into<String>) -> &mut Self {
        self.capabilities.insert(provider.into());
        self
    }

    pub fn register_command(&mut self, provider: impl Into<String>) -> &mut Self {
        self.commands.insert(provider.into());
        self
    }

    pub fn with_agent(mut self, provider: impl Into<String>) -> Self {
        self.register_agent(provider);
        self
    }

    pub fn with_workflow(mut self, provider: impl Into<String>) -> Self {
        self.register_workflow(provider);
        self
    }

    pub fn with_capability(mut self, provider: impl Into<String>) -> Self {
        self.register_capability(provider);
        self
    }

    pub fn with_command(mut self, provider: impl Into<String>) -> Self {
        self.register_command(provider);
        self
    }

    /// Resolve a validated reference against registered providers.
    pub fn resolve(
        &self,
        executor_ref: &ExecutorRef,
    ) -> Result<ResolvedExecutionPlan, ExecutorResolutionError> {
        let (scheme, target) = executor_ref
            .as_str()
            .split_once("://")
            .ok_or_else(|| ExecutorResolutionError::InvalidExecutorRef(executor_ref.to_string()))?;
        let (kind, available) = match scheme {
            "agent" | "subagent" => (ExecutorKind::Agent, self.agents.contains(target)),
            "workflow" => (ExecutorKind::Workflow, self.workflows.contains(target)),
            "capability" => (ExecutorKind::Capability, self.capabilities.contains(target)),
            "command" => (ExecutorKind::Command, self.commands.contains(target)),
            _ => {
                return Err(ExecutorResolutionError::InvalidExecutorRef(
                    executor_ref.to_string(),
                ))
            }
        };
        if !available {
            return Err(ExecutorResolutionError::ProviderUnavailable {
                scheme: scheme.to_string(),
                target: target.to_string(),
            });
        }
        Ok(ResolvedExecutionPlan {
            executor_ref: executor_ref.clone(),
            kind,
            provider: target.to_string(),
            command_binding: None,
        })
    }

    /// Resolve the executor and strict command binding carried by one node.
    pub fn resolve_node(
        &self,
        node: &TaskNode,
    ) -> Result<ResolvedExecutionPlan, ExecutorResolutionError> {
        let raw = node
            .input
            .get("executor_ref")
            .and_then(serde_json::Value::as_str)
            .ok_or(ExecutorResolutionError::MissingExecutorRef)?;
        let executor_ref = ExecutorRef::new(raw)
            .map_err(|error| ExecutorResolutionError::InvalidExecutorRef(error.to_string()))?;
        let mut plan = self.resolve(&executor_ref)?;
        let command_binding = validate_command_binding(&node.input)
            .map_err(|error| ExecutorResolutionError::InvalidCommandBinding(error.to_string()))?;
        if plan.kind == ExecutorKind::Command {
            plan.command_binding = command_binding;
        } else if command_binding.is_some() {
            return Err(ExecutorResolutionError::InvalidCommandBinding(
                "non-command executor cannot carry command_binding".to_string(),
            ));
        }
        Ok(plan)
    }

    pub fn resolve_for_node(
        &self,
        node: &TaskNode,
    ) -> Result<ResolvedExecutionPlan, ExecutorResolutionError> {
        self.resolve_node(node)
    }

    pub fn resolve_str(&self, raw: &str) -> Result<ResolvedExecutionPlan, ExecutorResolutionError> {
        let executor_ref = ExecutorRef::new(raw)
            .map_err(|error| ExecutorResolutionError::InvalidExecutorRef(error.to_string()))?;
        self.resolve(&executor_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ExecutorRef, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind};
    use serde_json::json;

    fn node(reference: &str, input: serde_json::Value) -> TaskNode {
        TaskNode::new(
            TaskNodeId::new(reference.replace("://", "-")).unwrap(),
            TaskNodeKind::Work,
            "test node",
            input,
        )
        .unwrap_or_else(|error| panic!("{error:?}"))
    }

    #[test]
    fn resolver_maps_supported_schemes_and_requires_registered_providers() {
        let mut resolver = ExecutorResolver::new();
        resolver
            .register_agent("writer")
            .register_workflow("report")
            .register_capability("files.read")
            .register_command("desktop.app.focus");

        let cases = [
            ("agent://writer", ExecutorKind::Agent),
            ("subagent://writer", ExecutorKind::Agent),
            ("workflow://report", ExecutorKind::Workflow),
            ("capability://files.read", ExecutorKind::Capability),
            ("command://desktop.app.focus", ExecutorKind::Command),
        ];
        for (raw, kind) in cases {
            let reference = ExecutorRef::new(raw).unwrap();
            assert_eq!(resolver.resolve(&reference).unwrap().kind, kind);
        }

        let missing = ExecutorRef::new("capability://unknown").unwrap();
        assert!(matches!(
            resolver.resolve(&missing),
            Err(ExecutorResolutionError::ProviderUnavailable { .. })
        ));
    }

    #[test]
    fn node_resolution_enforces_strict_command_binding_and_keeps_open_reserved() {
        let mut resolver = ExecutorResolver::new();
        resolver
            .register_command("desktop.app.focus")
            .register_command("desktop.app.open");

        let focus = node(
            "command://desktop.app.focus",
            json!({
                "executor_ref": "command://desktop.app.focus",
                "command_binding": {
                    "command": "desktop.app.focus",
                    "args": {"app_id": "app:editor"}
                }
            }),
        );
        let plan = resolver.resolve_node(&focus).unwrap();
        assert_eq!(plan.command_binding.unwrap().args.app_id, "app:editor");

        let mut missing_binding = focus.clone();
        missing_binding.input = json!({"executor_ref": "command://desktop.app.focus"});
        assert!(matches!(
            resolver.resolve_node(&missing_binding),
            Err(ExecutorResolutionError::InvalidCommandBinding(_))
        ));

        let open = node(
            "command://desktop.app.open",
            json!({
                "executor_ref": "command://desktop.app.open",
                "command_binding": {
                    "command": "desktop.app.open",
                    "args": {"app_id": "app:editor"}
                }
            }),
        );
        assert_eq!(
            resolver
                .resolve_node(&open)
                .unwrap()
                .command_binding
                .unwrap()
                .command,
            "desktop.app.open"
        );
    }

    #[test]
    fn resolver_does_not_treat_descriptor_only_capability_as_executable() {
        let resolver = ExecutorResolver::new();
        let reference = ExecutorRef::new("capability://descriptor.only").unwrap();
        assert!(resolver.resolve(&reference).is_err());
    }

    #[allow(dead_code)]
    fn _graph_id_is_used_to_keep_test_imports_stable() -> TaskGraphId {
        TaskGraphId::new("executor-tests").unwrap()
    }
}
