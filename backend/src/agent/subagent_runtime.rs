// ============================================================
// Subagent Runtime Execution Foundation — isolated Child Agent.
//
// A subagent delegation runs as a brand-new Child AgentState with its own
// restricted tool registry, security gateway, approval store, and ReAct loop.
//
// Hard runtime constraints this round:
//   - Child registry is a strict whitelist of `allowed_tools` copied from the
//     parent source registry; no implicit builtins.
//   - `subagent_*` tools are always filtered out (single-layer delegation).
//   - Nested approvals fail closed (child uses an independent ApprovalStore).
//   - workdir override is not supported yet (fail closed).
//   - Only the model *name* may be overridden; provider config is inherited.
//   - The child never inherits parent conversation history / memory / workflow.
//
// Production wiring (Step 6D) will construct this executor from AppServer.
// Until then the executor and its helpers are exercised only by tests, so
// dead-code warnings are suppressed here on purpose.
// ============================================================

#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::engine;
use crate::agent::state::AgentState;
use crate::agent::verifier::DefaultVerifier;
use crate::config::types::AppConfig;
use crate::db::Database;
use crate::llm::client::LlmClient;
use crate::safety::approval::ApprovalStore;
use crate::safety::AuditRecorder;
use crate::safety::SecurityExecutionGateway;
use crate::secret::SecretResolver;
use crate::server::LogBuffer;
use crate::tools::registry::ToolRegistry;
use crate::tools::subagent::{SubagentExecutionSpec, SubagentExecutor};
use crate::tools::trait_def::ToolResult;
use tokio_util::sync::CancellationToken;

/// Maximum characters of the final child result returned to the parent.
pub(crate) const MAX_SUBAGENT_RESULT_CHARS: usize = 16_000;

/// Runtime executor that runs a local Child Agent using the shared ReAct engine.
pub(crate) struct LocalSubagentExecutor {
    config: AppConfig,
    workspace_root: String,
    source_tool_registry: Arc<ToolRegistry>,
    db: Database,
    audit_recorder: AuditRecorder,
    log_buffer: LogBuffer,
    resolver: Arc<SecretResolver>,
}

impl LocalSubagentExecutor {
    pub(crate) fn new(
        config: AppConfig,
        workspace_root: String,
        source_tool_registry: Arc<ToolRegistry>,
        db: Database,
        audit_recorder: AuditRecorder,
        log_buffer: LogBuffer,
        resolver: Arc<SecretResolver>,
    ) -> Self {
        Self {
            config,
            workspace_root,
            source_tool_registry,
            db,
            audit_recorder,
            log_buffer,
            resolver,
        }
    }
}

#[async_trait]
impl SubagentExecutor for LocalSubagentExecutor {
    async fn execute_subagent(&self, spec: &SubagentExecutionSpec, task: &str) -> ToolResult {
        if let Err(reason) = validate_runtime_spec(spec) {
            return ToolResult::error(reason);
        }

        let child_config = build_child_config(&self.config, spec.model.as_deref());
        let child_registry =
            build_child_tool_registry(&self.source_tool_registry, &spec.allowed_tools);

        let system_prompt = build_child_system_prompt(&spec.name, &spec.instructions);
        let mut state = AgentState::new(system_prompt);
        state.add_user_message(task.to_string());

        let conversation_id = format!("subagent:{}:{}", spec.name, uuid::Uuid::new_v4());

        let client = LlmClient::new(&child_config.model, Arc::clone(&self.resolver));
        let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            child_config.sandbox.clone(),
            self.workspace_root.clone(),
            Arc::clone(&child_registry),
            Arc::new(DefaultVerifier::new(&self.workspace_root)),
            Arc::new(self.audit_recorder.clone()),
        )
        .with_db(Arc::new(self.db.clone_connection()))
        .with_grant_enforcement();

        // Independent approval store: a child pause must never leak into the
        // parent approval chain.
        let child_approval_store = ApprovalStore::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(256);
        let cancel_token = CancellationToken::new();

        let result = engine::run_react_loop_with_channel(
            &mut state,
            &client,
            &child_registry,
            &child_approval_store,
            &security_gateway,
            &child_config,
            &conversation_id,
            &cancel_token,
            &tx,
            &self.log_buffer,
        )
        .await;

        match result {
            Ok(engine::RunOutcome::Done { output }) => bounded_subagent_success(&output),
            Ok(engine::RunOutcome::Paused { .. }) => ToolResult::error(
                "Subagent requested a nested approval; nested approvals are not supported",
            ),
            Err(error) => ToolResult::error(format!(
                "Subagent execution failed: {}",
                crate::utils::text::truncate_chars(&error, 1000)
            )),
        }
    }
}

/// Fail-closed validation before any LLM call. Currently only the workdir
/// override is rejected; model-name override is allowed.
fn validate_runtime_spec(spec: &SubagentExecutionSpec) -> Result<(), String> {
    if let Some(workdir) = &spec.workdir {
        if !workdir.trim().is_empty() {
            return Err("Subagent workdir override is not supported yet".to_string());
        }
    }
    Ok(())
}

/// Copy the parent config but allow only the model *name* to be overridden by
/// the subagent definition. Provider / base_url / keys / sandbox are inherited.
fn build_child_config(parent: &AppConfig, model: Option<&str>) -> AppConfig {
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
fn build_child_tool_registry(source: &ToolRegistry, allowed_tools: &[String]) -> Arc<ToolRegistry> {
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
fn build_child_system_prompt(name: &str, instructions: &str) -> String {
    format!(
        "You are the subagent `{name}`.\n\n\
        Runtime constraints:\n\
        - Complete only the delegated task.\n\
        - You may use only tools exposed by the child runtime registry.\n\
        - You must not delegate to another subagent.\n\
        - Nested approvals are not supported.\n\
        - Do not claim access to tools that are not present.\n\n\
        ## Subagent Instructions\n\n{instructions}"
    )
}

/// Bound a successful child output into a `ToolResult` (UTF-8 safe).
fn bounded_subagent_success(output: &str) -> ToolResult {
    let bounded = crate::utils::text::truncate_chars(output, MAX_SUBAGENT_RESULT_CHARS);
    ToolResult::success(bounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::trait_def::{RiskLevel, Tool};
    use async_trait::async_trait;

    struct NoopTool {
        name: &'static str,
    }

    #[async_trait]
    impl Tool for NoopTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }

        async fn execute(&self, _args: serde_json::Value) -> ToolResult {
            ToolResult::success("ok")
        }
    }

    fn source_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(NoopTool { name: "read_file" }));
        reg.register(Arc::new(NoopTool { name: "grep" }));
        reg.register(Arc::new(NoopTool { name: "bash" }));
        reg.register(Arc::new(NoopTool {
            name: "subagent_other",
        }));
        reg
    }

    fn spec(name: &str) -> SubagentExecutionSpec {
        SubagentExecutionSpec {
            name: name.to_string(),
            instructions: "instructions".to_string(),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            model: Some("deepseek-v4-flash".to_string()),
            workdir: None,
        }
    }

    #[test]
    fn child_registry_is_strict_whitelist() {
        let source = source_registry();
        let child = build_child_tool_registry(&source, &spec("researcher").allowed_tools);
        assert!(child.get("read_file").is_some());
        assert!(child.get("grep").is_some());
        assert!(child.get("bash").is_none());
        assert_eq!(child.list_tools().len(), 2);
    }

    #[test]
    fn child_registry_has_no_implicit_builtins() {
        let source = source_registry();
        let child = build_child_tool_registry(&source, &["read_file".to_string()]);
        assert!(child.get("read_file").is_some());
        assert!(child.get("write_file").is_none());
        assert!(child.get("bash").is_none());
        assert!(child.get("http_request").is_none());
    }

    #[test]
    fn child_registry_skips_missing_allowed_tools() {
        let source = source_registry();
        let child = build_child_tool_registry(
            &source,
            &["read_file".to_string(), "not_exists".to_string()],
        );
        assert!(child.get("read_file").is_some());
        assert!(child.get("not_exists").is_none());
    }

    #[test]
    fn child_registry_filters_subagent_tools() {
        let source = source_registry();
        let mut s = spec("researcher");
        s.allowed_tools = vec!["subagent_other".to_string(), "read_file".to_string()];
        let child = build_child_tool_registry(&source, &s.allowed_tools);
        assert!(child.get("subagent_other").is_none());
        assert!(child.get("read_file").is_some());
    }

    #[test]
    fn child_config_overrides_only_model_name() {
        let mut parent = AppConfig::default();
        parent.model.name = "parent-model".to_string();
        parent.model.base_url = "https://example.com/v1".to_string();
        parent.model.api_key = "secret".to_string();
        parent.sandbox.profile = crate::config::types::SandboxProfile::ReadOnly;

        let child = build_child_config(&parent, Some("child-model"));
        assert_eq!(child.model.name, "child-model");
        assert_eq!(child.model.base_url, "https://example.com/v1");
        assert_eq!(child.model.api_key, "secret");
        assert_eq!(
            child.sandbox.profile,
            crate::config::types::SandboxProfile::ReadOnly
        );
    }

    #[test]
    fn child_config_ignores_empty_model_override() {
        let mut parent = AppConfig::default();
        parent.model.name = "parent-model".to_string();
        let child = build_child_config(&parent, Some("   "));
        assert_eq!(child.model.name, "parent-model");
        let child_none = build_child_config(&parent, None);
        assert_eq!(child_none.model.name, "parent-model");
    }

    #[test]
    fn workdir_override_fails_closed() {
        let mut s = spec("researcher");
        s.workdir = Some("./other".to_string());
        let err = validate_runtime_spec(&s).unwrap_err();
        assert!(err.contains("workdir"));
    }

    #[test]
    fn workdir_none_passes_validation() {
        let s = spec("researcher");
        assert!(validate_runtime_spec(&s).is_ok());
    }

    #[test]
    fn bounded_success_truncates_utf8_safe() {
        let long = "长".repeat(MAX_SUBAGENT_RESULT_CHARS + 5000);
        let result = bounded_subagent_success(&long);
        assert!(result.ok);
        assert!(result.content.chars().count() <= MAX_SUBAGENT_RESULT_CHARS + 3);
        assert!(result.content.is_char_boundary(result.content.len()));
    }

    #[test]
    fn bounded_success_keeps_short_output() {
        let result = bounded_subagent_success("short");
        assert!(result.ok);
        assert_eq!(result.content, "short");
    }
}
