// ============================================================
// SubagentToolAdapter — safe Tool-trait adapter for a discovered
// subagent definition.
//
// Converts a strictly-parsed [`DiscoveredSubagent`] into a [`Tool`] so it can
// be exposed as an OpenAI-compatible tool definition and registered into a
// runtime registry.
//
// Security posture:
//   - risk_level is always High (delegation would run another agent chain)
//   - the disabled constructor (`new`) keeps the adapter fail-closed
//     ("Subagent execution is not enabled yet")
//   - the executable constructors bind a [`SubagentExecutor`]; production
//     registration must always use `new_unique_executable`
//   - `security_descriptor()` emits the formal agent.delegate policy
//     (AgentDelegate @ Subagent / High / AgentDelegation); execution is
//     authorized upstream by the SecurityExecutionGateway
//
// The full instructions body is kept private: it is not part of the Tool
// description, parameters, Debug output, or any serialization.
// ============================================================

use async_trait::async_trait;
use std::sync::Arc;

use crate::safety::{
    DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
    ToolSecurityDescriptor,
};
use crate::server::DiscoveredSubagent;
use crate::tools::trait_def::{RiskLevel, Tool, ToolResult};

/// Maximum length of the final exposed tool name.
const MAX_EXPOSED_SUBAGENT_NAME_LEN: usize = 64;
/// Maximum description characters (UTF-8 safe truncation).
const MAX_SUBAGENT_DESCRIPTION_CHARS: usize = 1000;
/// Maximum delegated task length. Oversized tasks are rejected, never silently
/// truncated (the approved task must be the executed task).
const MAX_SUBAGENT_TASK_CHARS: usize = 8000;

/// Executes a subagent delegation. Crate-private; production wiring is a later
/// step (6D). The execution result is always a bounded [`ToolResult`].
#[async_trait]
pub(crate) trait SubagentExecutor: Send + Sync {
    async fn execute_subagent(&self, spec: &SubagentExecutionSpec, task: &str) -> ToolResult;
}

/// Trusted, cloned subagent execution parameters. Everything here comes from
/// the discovered definition — never from LLM arguments.
#[derive(Clone)]
pub(crate) struct SubagentExecutionSpec {
    pub(crate) name: String,
    /// Read by the child runtime once wired (Step 6D); kept private from Debug.
    #[allow(dead_code)]
    pub(crate) instructions: String,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) model: Option<String>,
    pub(crate) workdir: Option<String>,
}

// Manual Debug: never print the instructions body.
impl std::fmt::Debug for SubagentExecutionSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentExecutionSpec")
            .field("name", &self.name)
            .field("allowed_tools", &self.allowed_tools)
            .field("model", &self.model)
            .field("workdir", &self.workdir)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubagentToolAdapterError {
    #[error("subagent name is empty")]
    EmptyName,
    #[error("subagent name cannot be safely exposed")]
    InvalidName,
    #[error("subagent instructions are empty")]
    EmptyInstructions,
    #[error("subagent exposed tool name collides with an existing name: {0}")]
    NameCollision(String),
}

/// A Tool-trait adapter around a single discovered subagent definition.
///
/// `exposed_name` is what the LLM / Agent would see; the subagent identity
/// and instructions are kept for a future delegation runtime.
pub struct SubagentToolAdapter {
    exposed_name: String,
    subagent_name: String,
    description: String,
    instructions: String,
    allowed_tools: Vec<String>,
    model: Option<String>,
    workdir: Option<String>,
    /// Reserved for the future delegation runtime; kept private.
    #[allow(dead_code)]
    definition_path: String,
    /// Optional runtime executor. `None` keeps the adapter fail-closed.
    executor: Option<Arc<dyn SubagentExecutor>>,
}

// Manual Debug: never print the instructions body or the executor.
impl std::fmt::Debug for SubagentToolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentToolAdapter")
            .field("exposed_name", &self.exposed_name)
            .field("subagent_name", &self.subagent_name)
            .field("allowed_tools", &self.allowed_tools)
            .field("model", &self.model)
            .field("workdir", &self.workdir)
            .finish()
    }
}

impl SubagentToolAdapter {
    /// Construct a fail-closed adapter (executor = `None`).
    pub fn new(definition: &DiscoveredSubagent) -> Result<Self, SubagentToolAdapterError> {
        Self::new_internal(definition, None)
    }

    /// Construct an executable adapter backed by a subagent runtime executor.
    /// Production wiring lands in Step 6D.
    #[allow(dead_code)]
    pub(crate) fn new_executable(
        definition: &DiscoveredSubagent,
        executor: Arc<dyn SubagentExecutor>,
    ) -> Result<Self, SubagentToolAdapterError> {
        Self::new_internal(definition, Some(executor))
    }

    /// Collision-safe executable constructor for the future production
    /// registry (Step 6D).
    #[allow(dead_code)]
    pub(crate) fn new_unique_executable(
        definition: &DiscoveredSubagent,
        occupied_names: &mut std::collections::HashSet<String>,
        executor: Arc<dyn SubagentExecutor>,
    ) -> Result<Self, SubagentToolAdapterError> {
        let adapter = Self::new_executable(definition, executor)?;
        if !occupied_names.insert(adapter.name().to_string()) {
            return Err(SubagentToolAdapterError::NameCollision(
                adapter.name().to_string(),
            ));
        }
        Ok(adapter)
    }

    fn new_internal(
        definition: &DiscoveredSubagent,
        executor: Option<Arc<dyn SubagentExecutor>>,
    ) -> Result<Self, SubagentToolAdapterError> {
        if definition.name.trim().is_empty() {
            return Err(SubagentToolAdapterError::EmptyName);
        }
        if definition.instructions.trim().is_empty() {
            return Err(SubagentToolAdapterError::EmptyInstructions);
        }
        let sanitized = sanitize_subagent_name(&definition.name)
            .ok_or(SubagentToolAdapterError::InvalidName)?;
        let exposed_name = build_exposed_name(&sanitized)?;

        let fallback = format!("Subagent {}", definition.name);
        let raw_desc = if definition.description.trim().is_empty() {
            &fallback
        } else {
            &definition.description
        };
        // Bound the FINAL LLM-visible description, keeping the prefix intact.
        // `truncate_chars` appends a "..." ellipsis when truncating, so reserve
        // 3 chars so the assembled string never exceeds the budget.
        let prefix = format!("[Subagent:{}] ", definition.name);
        let prefix_len = prefix.chars().count();
        if prefix_len + 3 >= MAX_SUBAGENT_DESCRIPTION_CHARS {
            // The prefix (plus minimum truncation overhead) cannot fit within
            // the budget while remaining intact — fail closed rather than
            // truncating the prefix.
            return Err(SubagentToolAdapterError::InvalidName);
        }
        let content_budget = MAX_SUBAGENT_DESCRIPTION_CHARS - prefix_len - 3;
        let truncated = crate::utils::text::truncate_chars(raw_desc, content_budget);
        let description = format!("{prefix}{truncated}");

        Ok(Self {
            exposed_name,
            subagent_name: definition.name.clone(),
            description,
            instructions: definition.instructions.clone(),
            allowed_tools: definition.allowed_tools.clone(),
            model: definition.model.clone(),
            workdir: definition.workdir.clone(),
            definition_path: definition.path.clone(),
            executor,
        })
    }

    /// Construct an adapter while enforcing exposed-name uniqueness against an
    /// accumulator of already-occupied names. Returns
    /// [`SubagentToolAdapterError::NameCollision`] if the name is already taken.
    pub fn new_unique(
        definition: &DiscoveredSubagent,
        occupied_names: &mut std::collections::HashSet<String>,
    ) -> Result<Self, SubagentToolAdapterError> {
        let adapter = Self::new(definition)?;
        if !occupied_names.insert(adapter.name().to_string()) {
            return Err(SubagentToolAdapterError::NameCollision(
                adapter.name().to_string(),
            ));
        }
        Ok(adapter)
    }
}

/// Register discovered subagent definitions as executable adapters into a
/// runtime registry snapshot.
///
/// Pure and side-effect free: no AGENT.md re-read, no LLM, no Child Agent.
/// Every adapter is built via `new_unique_executable` so a name collision or
/// invalid definition is skipped without failing the rest of the batch.
///
/// Returns the number of tools successfully registered.
pub(crate) fn register_discovered_subagent_tools(
    registry: &mut crate::tools::ToolRegistry,
    occupied_names: &mut std::collections::HashSet<String>,
    definitions: &[DiscoveredSubagent],
    executor: Arc<dyn SubagentExecutor>,
) -> usize {
    let mut registered = 0usize;
    for definition in definitions {
        match SubagentToolAdapter::new_unique_executable(
            definition,
            occupied_names,
            Arc::clone(&executor),
        ) {
            Ok(adapter) => {
                registry.register(Arc::new(adapter));
                registered += 1;
            }
            Err(error) => {
                tracing::debug!(
                    subagent_name = %definition.name,
                    error = %error,
                    "skipping subagent definition (invalid or name collision)"
                );
            }
        }
    }
    registered
}

/// Sanitize a subagent name into a stable ASCII identifier.
/// Returns `None` if nothing usable remains.
fn sanitize_subagent_name(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Build `subagent_<name>`, truncating only the name part.
fn build_exposed_name(name_ns: &str) -> Result<String, SubagentToolAdapterError> {
    let prefix = "subagent_";
    let budget = MAX_EXPOSED_SUBAGENT_NAME_LEN.saturating_sub(prefix.len());
    if budget == 0 {
        return Err(SubagentToolAdapterError::InvalidName);
    }
    let name_part: String = name_ns.chars().take(budget).collect();
    let name_part = name_part.trim_matches('_');
    if name_part.is_empty() {
        return Err(SubagentToolAdapterError::InvalidName);
    }
    Ok(format!("{prefix}{name_part}"))
}

#[async_trait]
impl Tool for SubagentToolAdapter {
    fn name(&self) -> &str {
        &self.exposed_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task to delegate to this subagent"
                }
            },
            "required": ["task"],
            "additionalProperties": false
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    /// Produce a trusted Subagent delegation security descriptor.
    ///
    /// The delegated subagent identity always comes from the adapter's trusted
    /// `subagent_name` binding — never from the LLM-provided arguments. The
    /// `task` argument must be present and non-empty. The descriptor is
    /// validated through the standard profile chain (no bypass).
    fn security_descriptor(
        &self,
        args: &serde_json::Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let task = args.get("task").and_then(|v| v.as_str());
        match task {
            Some(task) if !task.trim().is_empty() => {}
            _ => {
                return Err(DescriptorError::MissingArgument {
                    tool: self.name().to_string(),
                    argument: "task",
                })
            }
        }

        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name().to_string(),
            requested_permissions: vec![
                PermissionId::AgentDelegate.in_scope(ResourceScope::Subagent)
            ],
            resources: vec![ResourceDescriptor::Subagent {
                name: self.subagent_name.clone(),
            }],
            default_risk: RiskLevel::High,
            side_effects: vec![SideEffectKind::AgentDelegation],
        };
        descriptor.validate_for_tool(self.name())?;
        Ok(descriptor)
    }

    /// Execute the delegated task through the bound executor.
    ///
    /// Defensively validates `task` even though the Security Gateway already
    /// checked it: it must be a non-empty string within the length cap. The
    /// execution spec is always built from the adapter's trusted binding, never
    /// from extra LLM arguments.
    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let task = match args.get("task").and_then(|v| v.as_str()) {
            Some(task) => task.trim(),
            None => return ToolResult::error("Subagent task is missing or empty"),
        };
        if task.is_empty() {
            return ToolResult::error("Subagent task is missing or empty");
        }
        if task.chars().count() > MAX_SUBAGENT_TASK_CHARS {
            return ToolResult::error("Subagent task exceeds maximum length");
        }

        let Some(executor) = &self.executor else {
            return ToolResult::error("Subagent execution is not enabled yet");
        };

        let spec = SubagentExecutionSpec {
            name: self.subagent_name.clone(),
            instructions: self.instructions.clone(),
            allowed_tools: self.allowed_tools.clone(),
            model: self.model.clone(),
            workdir: self.workdir.clone(),
        };
        executor.execute_subagent(&spec, task).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
    };

    fn definition(name: &str, description: &str, instructions: &str) -> DiscoveredSubagent {
        DiscoveredSubagent {
            name: name.to_string(),
            description: description.to_string(),
            path: format!(".agents/agents/{name}/AGENT.md"),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            model: Some("deepseek-v4-flash".to_string()),
            workdir: None,
            instructions: instructions.to_string(),
        }
    }

    #[test]
    fn adapter_builds_namespaced_name() {
        let adapter =
            SubagentToolAdapter::new(&definition("researcher", "研究助手", "body")).unwrap();
        assert_eq!(adapter.name(), "subagent_researcher");
    }

    #[test]
    fn parameters_match_fixed_schema() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let params = adapter.parameters();
        assert_eq!(params["type"], "object");
        assert_eq!(params["required"], serde_json::json!(["task"]));
        assert_eq!(params["additionalProperties"], serde_json::json!(false));
        assert_eq!(params["properties"]["task"]["type"], "string");
    }

    #[test]
    fn description_has_prefix_and_no_instructions() {
        let adapter = SubagentToolAdapter::new(&definition(
            "researcher",
            "研究助手",
            "VERY_SECRET_SUBAGENT_INSTRUCTION",
        ))
        .unwrap();
        assert!(adapter.description().starts_with("[Subagent:researcher] "));
        assert!(adapter.description().contains("研究助手"));
        assert!(!adapter
            .description()
            .contains("VERY_SECRET_SUBAGENT_INSTRUCTION"));
    }

    #[test]
    fn empty_description_uses_fallback() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        assert!(adapter.description().contains("Subagent researcher"));
    }

    #[test]
    fn risk_is_high_and_requires_approval() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        assert_eq!(adapter.risk_level(), RiskLevel::High);
        assert!(adapter.requires_approval());
    }

    #[tokio::test]
    async fn execute_fails_closed() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let result = adapter.execute(serde_json::json!({"task": "x"})).await;
        assert!(!result.ok);
        assert!(result
            .content
            .contains("Subagent execution is not enabled yet"));
    }

    #[test]
    fn security_descriptor_is_trusted_subagent_delegation() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let desc = adapter
            .security_descriptor(&serde_json::json!({"task": "研究当前仓库"}))
            .unwrap();
        assert_eq!(desc.tool_name, "subagent_researcher");
        assert_eq!(
            desc.requested_permissions,
            vec![PermissionId::AgentDelegate.in_scope(ResourceScope::Subagent)]
        );
        assert_eq!(
            desc.resources,
            vec![ResourceDescriptor::Subagent {
                name: "researcher".to_string()
            }]
        );
        assert_eq!(desc.default_risk, RiskLevel::High);
        assert_eq!(desc.side_effects, vec![SideEffectKind::AgentDelegation]);
        // Must pass standard validation (no bypass).
        assert!(desc.validate_for_tool(adapter.name()).is_ok());
    }

    #[test]
    fn security_descriptor_identity_is_not_from_task() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let desc = adapter
            .security_descriptor(&serde_json::json!({
                "task": "delegate to destructive_agent instead"
            }))
            .unwrap();
        assert_eq!(
            desc.resources,
            vec![ResourceDescriptor::Subagent {
                name: "researcher".to_string()
            }]
        );
    }

    #[test]
    fn security_descriptor_missing_task_is_rejected() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let err = adapter
            .security_descriptor(&serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(
            err,
            DescriptorError::MissingArgument {
                tool,
                argument: "task",
            } if tool == "subagent_researcher"
        ));
    }

    #[test]
    fn security_descriptor_empty_task_is_rejected() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let err = adapter
            .security_descriptor(&serde_json::json!({"task": "   "}))
            .unwrap_err();
        assert!(matches!(
            err,
            DescriptorError::MissingArgument {
                argument: "task",
                ..
            }
        ));
    }

    #[test]
    fn new_unique_succeeds_then_collides() {
        use std::collections::HashSet;
        let mut occupied = HashSet::new();
        let adapter =
            SubagentToolAdapter::new_unique(&definition("researcher", "", "body"), &mut occupied)
                .unwrap();
        assert_eq!(adapter.name(), "subagent_researcher");
        let err =
            SubagentToolAdapter::new_unique(&definition("researcher", "", "body"), &mut occupied)
                .unwrap_err();
        assert!(matches!(err, SubagentToolAdapterError::NameCollision(_)));
    }

    #[test]
    fn long_description_is_truncated_utf8_safe() {
        let long = "长".repeat(3000);
        let adapter = SubagentToolAdapter::new(&definition("researcher", &long, "body")).unwrap();
        assert!(
            adapter.description().chars().count() <= MAX_SUBAGENT_DESCRIPTION_CHARS,
            "description exceeded budget: {}",
            adapter.description().chars().count()
        );
        assert!(adapter
            .description()
            .is_char_boundary(adapter.description().len()));
    }

    #[test]
    fn long_chinese_description_is_bounded_and_utf8_safe() {
        let long =
            "这是一个非常长的中文子智能体描述，用于验证 UTF-8 截断不会破坏多字节字符。".repeat(200);
        let adapter = SubagentToolAdapter::new(&definition("researcher", &long, "body")).unwrap();
        assert!(
            adapter.description().chars().count() <= MAX_SUBAGENT_DESCRIPTION_CHARS,
            "description exceeded budget: {}",
            adapter.description().chars().count()
        );
        assert!(adapter
            .description()
            .is_char_boundary(adapter.description().len()));
        // The prefix is fully preserved.
        assert!(adapter.description().starts_with("[Subagent:researcher] "));
    }

    #[test]
    fn debug_does_not_leak_instructions() {
        let adapter = SubagentToolAdapter::new(&definition(
            "researcher",
            "研究助手",
            "VERY_SECRET_SUBAGENT_INSTRUCTION",
        ))
        .unwrap();
        let debug = format!("{adapter:?}");
        assert!(!debug.contains("VERY_SECRET_SUBAGENT_INSTRUCTION"));
        assert!(debug.contains("subagent_researcher"));
        assert!(debug.contains("allowed_tools"));
    }

    #[test]
    fn empty_name_is_rejected() {
        let err = SubagentToolAdapter::new(&definition("", "desc", "body")).unwrap_err();
        assert!(matches!(err, SubagentToolAdapterError::EmptyName));
    }

    #[test]
    fn empty_instructions_are_rejected() {
        let err = SubagentToolAdapter::new(&definition("researcher", "desc", "  ")).unwrap_err();
        assert!(matches!(err, SubagentToolAdapterError::EmptyInstructions));
    }

    // ── Executable adapter (runtime foundation) ──

    struct FakeSubagentExecutor {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        seen_name: std::sync::Mutex<Option<String>>,
        seen_allowed_tools: std::sync::Mutex<Option<Vec<String>>>,
        seen_model: std::sync::Mutex<Option<Option<String>>>,
        seen_task: std::sync::Mutex<Option<String>>,
        seen_instructions: std::sync::Mutex<Option<String>>,
    }

    #[async_trait]
    impl SubagentExecutor for FakeSubagentExecutor {
        async fn execute_subagent(&self, spec: &SubagentExecutionSpec, task: &str) -> ToolResult {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            *self.seen_name.lock().unwrap() = Some(spec.name.clone());
            *self.seen_allowed_tools.lock().unwrap() = Some(spec.allowed_tools.clone());
            *self.seen_model.lock().unwrap() = Some(spec.model.clone());
            *self.seen_task.lock().unwrap() = Some(task.to_string());
            *self.seen_instructions.lock().unwrap() = Some(spec.instructions.clone());
            ToolResult::success("child result")
        }
    }

    fn fake_executor() -> (
        Arc<FakeSubagentExecutor>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let executor = Arc::new(FakeSubagentExecutor {
            calls: Arc::clone(&calls),
            seen_name: std::sync::Mutex::new(None),
            seen_allowed_tools: std::sync::Mutex::new(None),
            seen_model: std::sync::Mutex::new(None),
            seen_task: std::sync::Mutex::new(None),
            seen_instructions: std::sync::Mutex::new(None),
        });
        (executor, calls)
    }

    #[tokio::test]
    async fn executable_adapter_runs_fake_executor_with_trusted_spec() {
        let (executor, calls) = fake_executor();
        let adapter = SubagentToolAdapter::new_executable(
            &definition("researcher", "", "research body"),
            executor.clone(),
        )
        .unwrap();

        let result = adapter
            .execute(serde_json::json!({"task": "research this"}))
            .await;
        assert!(result.ok);
        assert_eq!(result.content, "child result");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            *executor.seen_name.lock().unwrap(),
            Some("researcher".to_string())
        );
        assert_eq!(
            *executor.seen_task.lock().unwrap(),
            Some("research this".to_string())
        );
        assert_eq!(
            *executor.seen_allowed_tools.lock().unwrap(),
            Some(vec!["read_file".to_string(), "grep".to_string()])
        );
        assert_eq!(
            *executor.seen_model.lock().unwrap(),
            Some(Some("deepseek-v4-flash".to_string()))
        );
        assert_eq!(
            *executor.seen_instructions.lock().unwrap(),
            Some("research body".to_string())
        );
    }

    #[tokio::test]
    async fn executable_adapter_spec_ignores_extra_arguments() {
        let (executor, _calls) = fake_executor();
        let adapter = SubagentToolAdapter::new_executable(
            &definition("researcher", "", "trusted instructions"),
            executor.clone(),
        )
        .unwrap();

        let result = adapter
            .execute(serde_json::json!({
                "task": "x",
                "model": "evil-model",
                "allowed_tools": ["bash"],
                "subagent": "evil"
            }))
            .await;
        assert!(result.ok);
        // Everything the executor saw comes from the definition, not args.
        assert_eq!(
            *executor.seen_name.lock().unwrap(),
            Some("researcher".to_string())
        );
        assert_eq!(
            *executor.seen_allowed_tools.lock().unwrap(),
            Some(vec!["read_file".to_string(), "grep".to_string()])
        );
        assert_eq!(
            *executor.seen_model.lock().unwrap(),
            Some(Some("deepseek-v4-flash".to_string()))
        );
        assert_eq!(
            *executor.seen_instructions.lock().unwrap(),
            Some("trusted instructions".to_string())
        );
    }

    #[tokio::test]
    async fn executable_adapter_missing_task_does_not_call_executor() {
        let (executor, calls) = fake_executor();
        let adapter =
            SubagentToolAdapter::new_executable(&definition("researcher", "", "body"), executor)
                .unwrap();

        for args in [serde_json::json!({}), serde_json::json!({"task": "   "})] {
            let result = adapter.execute(args).await;
            assert!(!result.ok);
            assert!(result.content.contains("missing or empty"));
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn executable_adapter_oversized_task_is_rejected() {
        let (executor, calls) = fake_executor();
        let adapter =
            SubagentToolAdapter::new_executable(&definition("researcher", "", "body"), executor)
                .unwrap();

        let long_task = "任".repeat(MAX_SUBAGENT_TASK_CHARS + 100);
        let result = adapter
            .execute(serde_json::json!({"task": long_task}))
            .await;
        assert!(!result.ok);
        assert!(result.content.contains("exceeds maximum length"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn disabled_adapter_still_fails_closed() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let result = adapter.execute(serde_json::json!({"task": "x"})).await;
        assert!(!result.ok);
        assert!(result
            .content
            .contains("Subagent execution is not enabled yet"));
    }

    #[test]
    fn new_unique_executable_is_collision_safe() {
        use std::collections::HashSet;
        let (executor, _calls) = fake_executor();
        let mut occupied = HashSet::new();
        SubagentToolAdapter::new_unique_executable(
            &definition("researcher", "", "body"),
            &mut occupied,
            executor.clone(),
        )
        .unwrap();
        let err = SubagentToolAdapter::new_unique_executable(
            &definition("researcher", "", "body"),
            &mut occupied,
            executor,
        )
        .unwrap_err();
        assert!(matches!(err, SubagentToolAdapterError::NameCollision(_)));
    }

    // ── production registration helper ──

    #[tokio::test]
    async fn register_helper_registers_executable_subagents() {
        use std::collections::HashSet;
        let (executor, calls) = fake_executor();
        let mut registry = crate::tools::ToolRegistry::new();
        let mut occupied = HashSet::new();
        let defs = vec![
            definition("researcher", "", "body"),
            definition("reviewer", "", "body"),
        ];

        let n = register_discovered_subagent_tools(&mut registry, &mut occupied, &defs, executor);
        assert_eq!(n, 2);
        assert!(registry.get("subagent_researcher").is_some());
        assert!(registry.get("subagent_reviewer").is_some());

        // Prove the registered adapters are executable (never the disabled new()).
        let researcher = registry.get("subagent_researcher").unwrap();
        let result = researcher.execute(serde_json::json!({"task": "x"})).await;
        assert!(result.ok);
        assert_eq!(result.content, "child result");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn register_helper_collision_skips_without_overwriting() {
        use std::collections::HashSet;
        let (executor, _calls) = fake_executor();
        let mut registry = crate::tools::ToolRegistry::new();
        let mut occupied = HashSet::new();
        occupied.insert("subagent_researcher".to_string());
        let defs = vec![
            definition("researcher", "", "body"),
            definition("reviewer", "", "body"),
        ];

        let n = register_discovered_subagent_tools(&mut registry, &mut occupied, &defs, executor);
        assert_eq!(n, 1);
        assert!(registry.get("subagent_researcher").is_none());
        assert!(registry.get("subagent_reviewer").is_some());
    }

    #[test]
    fn register_helper_skips_invalid_definition() {
        use std::collections::HashSet;
        let (executor, _calls) = fake_executor();
        let mut registry = crate::tools::ToolRegistry::new();
        let mut occupied = HashSet::new();
        let bad = DiscoveredSubagent {
            name: "bad".to_string(),
            description: String::new(),
            path: String::new(),
            allowed_tools: vec![],
            model: None,
            workdir: None,
            instructions: "   ".to_string(), // empty instructions -> invalid
        };
        let defs = vec![bad, definition("reviewer", "", "body")];

        let n = register_discovered_subagent_tools(&mut registry, &mut occupied, &defs, executor);
        assert_eq!(n, 1);
        assert!(registry.get("subagent_bad").is_none());
        assert!(registry.get("subagent_reviewer").is_some());
    }
}
