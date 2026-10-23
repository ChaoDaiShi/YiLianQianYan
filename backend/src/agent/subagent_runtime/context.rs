//! Construction of the isolated child configuration, tools, and system prompt.
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::tools::registry::ToolRegistry;

/// Copy the parent config but allow only the model *name* to be overridden by
/// the subagent definition. Provider / base_url / keys / sandbox are inherited.
pub(super) fn build_child_config(parent: &AppConfig, model: Option<&str>) -> AppConfig {
    let mut child = parent.clone();
    if let Some(model) = model {
        let model = model.trim();
        if !model.is_empty() {
            child.model.name = model.to_string();
        }
    }
    child
}

/// Build a strictly whitelisted child tool registry.
///
/// Starts empty and only exact-copies tools whose names appear in
/// `allowed_tools`. `subagent_*` names are always filtered out (single-layer
/// delegation). Missing tools are skipped.
pub(super) fn build_child_tool_registry(
    source: &ToolRegistry,
    allowed_tools: &[String],
) -> Arc<ToolRegistry> {
    let mut child = ToolRegistry::new();
    for allowed_name in allowed_tools {
        if allowed_name.starts_with("subagent_") {
            continue;
        }
        if let Some(tool) = source.get(allowed_name) {
            child.register(Arc::clone(tool));
        }
    }
    Arc::new(child)
}

/// Independent child system prompt. The instructions are a trusted local
/// definition, used as the child's system instructions.
pub(super) fn build_child_system_prompt(
    name: &str,
    instructions: &str,
    handoff: Option<&crate::agent::delegation::DelegationRequest>,
) -> String {
    let mut prompt = format!(
        "You are the subagent `{name}`.\n\n\
        Runtime constraints:\n\
        - Complete only the delegated task.\n\
        - You may use only tools exposed by the child runtime registry.\n\
        - You must not delegate to another subagent.\n\
        - Nested approvals are not supported.\n\
        - Role labels are descriptive and do not grant capabilities.\n\
        - Do not claim access to tools that are not present.\n\n\
        ## Subagent Instructions\n\n{instructions}"
    );
    if handoff.is_some() {
        prompt.push_str("\n\nFor handoff completion, return one JSON object with `summary`, `artifact_ids`, `artifacts`, and `evidence_ids`. Each `artifacts` item has a required `name` and actual workspace-relative `path`, plus optional `artifact_id` only when a tool returned a registered artifact id. Never invent an id or path. Evidence ids may only be selected from the explicit handoff context. Keep the completion unverified and do not claim the parent acceptance passed.");
    }
    if handoff.is_some_and(|request| !request.required_artifact_names.is_empty()) {
        prompt.push_str(" A required artifact is complete only after an authorized tool has produced it and you can report its registered artifact id.");
    }
    prompt
}
