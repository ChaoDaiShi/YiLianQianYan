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
        let truncated =
            crate::utils::text::truncate_chars(raw_desc, MAX_SUBAGENT_DESCRIPTION_CHARS);
        let description = format!("[Subagent:{}] {}", definition.name, truncated);

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

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("Subagent execution is not enabled yet")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::DescriptorError;

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
    fn security_descriptor_still_fails_closed() {
        let adapter = SubagentToolAdapter::new(&definition("researcher", "", "body")).unwrap();
        let result = adapter.security_descriptor(&serde_json::json!({"task": "x"}));
        assert!(matches!(result, Err(DescriptorError::UnknownTool(_))));
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
        let prefix_len = "[Subagent:researcher] ".chars().count();
        assert!(adapter.description().chars().count() <= 1000 + 3 + prefix_len);
        assert!(adapter
            .description()
            .is_char_boundary(adapter.description().len()));
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
