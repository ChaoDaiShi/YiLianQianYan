use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::client::{AcpClient, AcpError, AcpPermissionHandler};
use super::model::{
    agent_message_text, exposed_tool_name, external_task_result, AcpLaunchConfig, AcpPromptOutcome,
    ExternalSessionState, ExternalTaskResult,
};
use super::session::AcpSession;
use crate::safety::{
    DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
    ToolSecurityDescriptor,
};
use crate::tools::trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::tools::ToolRegistry;

#[derive(Debug, thiserror::Error)]
pub enum AcpToolError {
    #[error("ACP provider id is empty or cannot form a tool name")]
    InvalidProviderId,
    #[error("ACP executable must be an existing absolute file path")]
    InvalidExecutable,
    #[error("ACP working directory must be an existing absolute directory")]
    InvalidWorkingDirectory,
    #[error("ACP timeout must be greater than zero")]
    InvalidTimeout,
    #[error("ACP environment allowlist contains an invalid variable name")]
    InvalidEnvironmentName,
    #[error("ACP exposed tool name collides with a registered tool: {0}")]
    NameCollision(String),
}

pub struct AcpAgentTool {
    name: String,
    config: AcpLaunchConfig,
    permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
    active: Mutex<std::collections::HashMap<String, CancellationToken>>,
    session_slot: Arc<Semaphore>,
}

impl std::fmt::Debug for AcpAgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpAgentTool")
            .field("name", &self.name)
            .field("provider_id", &self.config.id)
            .field("executable", &self.config.executable)
            .field("cwd", &self.config.cwd)
            .finish_non_exhaustive()
    }
}

impl AcpAgentTool {
    pub fn new(
        config: AcpLaunchConfig,
        permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
    ) -> Result<Self, AcpToolError> {
        Self::new_with_session_slot(config, permission_handler, Arc::new(Semaphore::new(1)))
    }

    pub fn new_with_session_slot(
        mut config: AcpLaunchConfig,
        permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
        session_slot: Arc<Semaphore>,
    ) -> Result<Self, AcpToolError> {
        let name = exposed_tool_name(&config.id).ok_or(AcpToolError::InvalidProviderId)?;
        if config.executable.is_absolute() {
            config.executable = std::fs::canonicalize(&config.executable)
                .map_err(|_| AcpToolError::InvalidExecutable)?;
        } else {
            return Err(AcpToolError::InvalidExecutable);
        }
        if !config.executable.is_file() {
            return Err(AcpToolError::InvalidExecutable);
        }
        if config.cwd.is_absolute() {
            config.cwd = std::fs::canonicalize(&config.cwd)
                .map_err(|_| AcpToolError::InvalidWorkingDirectory)?;
        } else {
            return Err(AcpToolError::InvalidWorkingDirectory);
        }
        if !config.cwd.is_dir() {
            return Err(AcpToolError::InvalidWorkingDirectory);
        }
        if config.timeout_seconds == 0 {
            return Err(AcpToolError::InvalidTimeout);
        }
        if config
            .env_allowlist
            .iter()
            .any(|name| !valid_environment_name(name))
        {
            return Err(AcpToolError::InvalidEnvironmentName);
        }
        Ok(Self {
            name,
            config,
            permission_handler,
            active: Mutex::new(std::collections::HashMap::new()),
            session_slot,
        })
    }

    pub async fn cancel_execution(&self, tool_call_id: &str) -> bool {
        let cancel = self.active.lock().await.remove(tool_call_id);
        if let Some(cancel) = cancel {
            cancel.cancel();
            true
        } else {
            false
        }
    }

    pub fn provider_id(&self) -> &str {
        &self.config.id
    }

    async fn execute_task(
        &self,
        args: Value,
        tool_call_id: String,
        parent_cancel: Option<CancellationToken>,
    ) -> ToolResult {
        let cancel = CancellationToken::new();
        self.active
            .lock()
            .await
            .insert(tool_call_id.clone(), cancel.clone());
        let parent_watcher = parent_cancel.map(|parent| {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                parent.cancelled().await;
                cancel.cancel();
            })
        });

        let result = self.run_agent(args, &cancel).await;

        if let Some(watcher) = parent_watcher {
            watcher.abort();
        }
        self.active.lock().await.remove(&tool_call_id);
        match result {
            Ok(result) if result.state == ExternalSessionState::Completed => {
                let content = if result.response_text.is_empty() {
                    "External ACP agent completed the prompt turn.".to_string()
                } else {
                    result.response_text.clone()
                };
                ToolResult::success_with_structured(content, serde_json::to_value(result).ok())
            }
            Ok(result) => {
                let content = format!("External ACP session ended in {:?}.", result.state);
                ToolResult::error(content).with_structured(serde_json::to_value(result).ok())
            }
            Err(error) => ToolResult::error(format!(
                "External ACP adapter failed: {}",
                error_code(&error)
            ))
            .with_structured(Some(json!({
                "provider_id": self.config.id,
                "state": match &error {
                    AcpError::ResultUnknown(_) => "result_unknown",
                    AcpError::Cancelled => "cancelling",
                    _ => "failed",
                },
                "error_code": error_code(&error),
                "model_calls": Value::Null,
                "total_tokens": Value::Null,
            }))),
        }
    }

    async fn run_agent(
        &self,
        args: Value,
        cancel: &CancellationToken,
    ) -> Result<ExternalTaskResult, AcpError> {
        let permit = Arc::clone(&self.session_slot)
            .try_acquire_owned()
            .map_err(|_| AcpError::Server {
                code: -32010,
                message: "external ACP session slot is occupied".to_string(),
            })?;
        let task = args
            .get("task")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AcpError::InvalidConfig("task must be a non-empty string".to_string())
            })?;
        let expected_outputs =
            parse_expected_outputs(args.get("expected_outputs"), &self.config.cwd)?;
        let prompt = build_task_prompt(task, &expected_outputs, &self.config.cwd);
        let client = AcpClient::spawn(&self.config, self.permission_handler.clone()).await?;
        let mut updates = client.subscribe_updates();
        let outcome = self.run_session(&client, &prompt, cancel).await;
        let result = match outcome {
            Ok((session, prompt_outcome)) => {
                let mut response_text = String::new();
                while let Ok(update) = updates.try_recv() {
                    if update.session_id == session.id() {
                        if let Some(text) = agent_message_text(&update.update) {
                            response_text.push_str(text);
                        }
                    }
                }
                let (state, stop_reason, context_usage, error) = match prompt_outcome {
                    AcpPromptOutcome::Stopped {
                        stop_reason,
                        context_usage,
                    } => {
                        let state = match stop_reason.as_str() {
                            "end_turn" => ExternalSessionState::Completed,
                            "cancelled" => ExternalSessionState::Cancelling,
                            "max_tokens" | "max_turn_requests" | "refusal" => {
                                ExternalSessionState::Failed
                            }
                            _ => ExternalSessionState::ResultUnknown,
                        };
                        let error = (state != ExternalSessionState::Completed)
                            .then(|| format!("acp_stop_reason_{stop_reason}"));
                        (state, Some(stop_reason), context_usage, error)
                    }
                    AcpPromptOutcome::ResultUnknown { error_code } => (
                        ExternalSessionState::ResultUnknown,
                        None,
                        client.latest_usage(session.id()).await,
                        Some(error_code),
                    ),
                };
                let artifact_paths =
                    collect_expected_outputs(&self.config.cwd, &expected_outputs).await;
                let missing_output =
                    !expected_outputs.is_empty() && artifact_paths.len() != expected_outputs.len();
                let mut external = external_task_result(self.config.id.clone());
                external.session_id = Some(session.id().to_string());
                external.state = if missing_output && state == ExternalSessionState::Completed {
                    ExternalSessionState::Failed
                } else {
                    state
                };
                external.stop_reason = stop_reason;
                external.response_text = response_text;
                external.artifact_paths = artifact_paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                external.context_usage = context_usage;
                external.error_code =
                    error.or_else(|| missing_output.then(|| "expected_output_missing".to_string()));
                Ok(external)
            }
            Err(error) => Err(error),
        };
        client.shutdown().await;
        drop(permit);
        result
    }

    async fn run_session(
        &self,
        client: &AcpClient,
        prompt: &str,
        cancel: &CancellationToken,
    ) -> Result<(AcpSession, AcpPromptOutcome), AcpError> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AcpError::Cancelled),
            result = client.initialize() => { result?; }
        }
        let session = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AcpError::Cancelled),
            result = AcpSession::create(client.clone(), &self.config.cwd) => result?,
        };
        let outcome = session.prompt(prompt, cancel).await?;
        Ok((session, outcome))
    }
}

#[async_trait]
impl Tool for AcpAgentTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Delegate a task to the configured external ACP v1 agent in its task working directory."
    }

    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties": {
                "task": {"type":"string", "minLength":1},
                "expected_outputs": {"type":"array", "items":{"type":"string"}},
            },
            "required":["task"],
            "additionalProperties":false,
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn security_descriptor(
        &self,
        _args: &Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name.clone(),
            requested_permissions: vec![
                PermissionId::AgentDelegate.in_scope(ResourceScope::Subagent)
            ],
            resources: vec![ResourceDescriptor::Subagent {
                name: self.config.id.clone(),
            }],
            default_risk: RiskLevel::High,
            side_effects: vec![SideEffectKind::AgentDelegation],
        };
        descriptor.validate_for_tool(self.name())?;
        Ok(descriptor)
    }

    async fn execute(&self, args: Value) -> ToolResult {
        let scope = crate::agent::delegation::current_scope();
        let cancel = scope.as_ref().map(|scope| scope.cancel.clone());
        self.execute_task(args, Uuid::new_v4().to_string(), cancel)
            .await
    }

    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let scope = crate::agent::delegation::current_scope();
        let cancel = scope.as_ref().map(|scope| scope.cancel.clone());
        self.execute_task(args, context.tool_call_id.clone(), cancel)
            .await
    }
}

/// Register one configured external provider for a runtime registry snapshot.
/// The caller owns provider configuration, Gateway wiring, and error reporting.
pub fn register_configured_acp_agent(
    registry: &mut ToolRegistry,
    config: AcpLaunchConfig,
    permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
) -> Result<Arc<AcpAgentTool>, AcpToolError> {
    let tool = Arc::new(AcpAgentTool::new(config, permission_handler)?);
    if registry.get(tool.name()).is_some() {
        return Err(AcpToolError::NameCollision(tool.name().to_string()));
    }
    registry.register(tool.clone());
    Ok(tool)
}

pub fn register_configured_acp_agent_with_slot(
    registry: &mut ToolRegistry,
    config: AcpLaunchConfig,
    permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
    session_slot: Arc<Semaphore>,
) -> Result<Arc<AcpAgentTool>, AcpToolError> {
    let tool = Arc::new(AcpAgentTool::new_with_session_slot(
        config,
        permission_handler,
        session_slot,
    )?);
    if registry.get(tool.name()).is_some() {
        return Err(AcpToolError::NameCollision(tool.name().to_string()));
    }
    registry.register(tool.clone());
    Ok(tool)
}

fn valid_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn parse_expected_outputs(value: Option<&Value>, cwd: &Path) -> Result<Vec<PathBuf>, AcpError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let outputs = value.as_array().ok_or_else(|| {
        AcpError::InvalidConfig("expected_outputs must be an array of relative paths".to_string())
    })?;
    outputs
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AcpError::InvalidConfig(
                        "expected_outputs must contain non-empty relative paths".to_string(),
                    )
                })?;
            let path = Path::new(value);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(AcpError::InvalidConfig(
                    "expected_outputs must stay beneath cwd".to_string(),
                ));
            }
            Ok(cwd.join(path))
        })
        .collect()
}

fn build_task_prompt(task: &str, expected_outputs: &[PathBuf], cwd: &Path) -> String {
    if expected_outputs.is_empty() {
        return task.to_string();
    }
    let outputs = expected_outputs
        .iter()
        .filter_map(|path| {
            path.strip_prefix(cwd)
                .ok()
                .map(|relative| relative.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{task}\n\nExpected output files relative to the configured task working directory: {outputs}")
}

async fn collect_expected_outputs(cwd: &Path, expected: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(root) = tokio::fs::canonicalize(cwd).await else {
        return found;
    };
    for path in expected {
        let Ok(actual) = tokio::fs::canonicalize(path).await else {
            continue;
        };
        if actual.starts_with(&root)
            && tokio::fs::metadata(&actual)
                .await
                .is_ok_and(|metadata| metadata.is_file())
        {
            found.push(actual);
        }
    }
    found
}

fn error_code(error: &AcpError) -> String {
    match error {
        AcpError::InvalidConfig(_) => "acp_invalid_config".to_string(),
        AcpError::SpawnFailed => "acp_spawn_failed".to_string(),
        AcpError::Transport(_) => "acp_transport_failed".to_string(),
        AcpError::Protocol(_) => "acp_protocol_error".to_string(),
        AcpError::Server { code, .. } => format!("acp_server_error_{code}"),
        AcpError::UnsupportedVersion(_) => "acp_unsupported_version".to_string(),
        AcpError::Timeout(_) => "acp_timeout".to_string(),
        AcpError::ResultUnknown(_) => "acp_result_unknown".to_string(),
        AcpError::Cancelled => "acp_cancelled".to_string(),
    }
}
