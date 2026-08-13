// ============================================================
// SubagentToolAdapter — safe Tool-trait adapter for a discovered
// subagent definition.
//
// Converts a strictly-parsed [`DiscoveredSubagent`] into a [`Tool`] so it can
// be exposed as an OpenAI-compatible tool definition and, in a later step,
// registered into a runtime registry.
//
// Deliberately fail-closed this round:
//   - risk_level is always High (delegation would run another agent chain)
//   - execute() always errors ("Subagent execution is not enabled yet")
//   - security_descriptor() keeps the default: a namespaced `subagent_*` name
//     is not a builtin tool, so it resolves to `UnknownTool` -> deny.
//
// The full instructions body is kept private: it is not part of the Tool
// description, parameters, Debug output, or any serialization.
// ============================================================

use async_trait::async_trait;

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
    /// Reserved for the future delegation runtime; kept private.
    #[allow(dead_code)]
    instructions: String,
    allowed_tools: Vec<String>,
    model: Option<String>,
    workdir: Option<String>,
    /// Reserved for the future delegation runtime; kept private.
    #[allow(dead_code)]
    definition_path: String,
}

// Manual Debug: never print the instructions body.
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
    pub fn new(definition: &DiscoveredSubagent) -> Result<Self, SubagentToolAdapterError> {
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

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("Subagent execution is not enabled yet")
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
}
