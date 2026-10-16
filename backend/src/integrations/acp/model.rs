use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const ACP_PROTOCOL_VERSION: u64 = 1;
pub const ACP_CLIENT_NAME: &str = "YiLianQianYan";
pub const ACP_CLIENT_VERSION: &str = "v1.2-adapter-pilot";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcpLaunchConfig {
    pub id: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    /// Provider working directory for this invocation. The caller should use a
    /// task-specific directory; the adapter does not create an OS sandbox.
    pub cwd: PathBuf,
    /// Names of process environment variables copied into the child. Their
    /// values remain in the process environment and never enter AppConfig.
    #[serde(default)]
    pub env_allowlist: Vec<String>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl std::fmt::Debug for AcpLaunchConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpLaunchConfig")
            .field("id", &self.id)
            .field("executable", &self.executable)
            .field(
                "args",
                &format_args!("[{} configured arguments]", self.args.len()),
            )
            .field("cwd", &self.cwd)
            .field("env_allowlist", &self.env_allowlist)
            .field("timeout_seconds", &self.timeout_seconds)
            .finish()
    }
}

fn default_timeout_seconds() -> u64 {
    600
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSessionState {
    Starting,
    Ready,
    Running,
    WaitingApproval,
    Cancelling,
    Completed,
    Failed,
    ResultUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExternalTaskResult {
    pub provider_id: String,
    pub session_id: Option<String>,
    pub state: ExternalSessionState,
    pub stop_reason: Option<String>,
    pub response_text: String,
    pub artifact_paths: Vec<String>,
    /// ACP v1 does not define model-call accounting in `usage_update`.
    pub model_calls: Option<u32>,
    /// Context window usage is not the same as cumulative input/output tokens.
    pub total_tokens: Option<u64>,
    pub context_usage: Option<AcpContextUsage>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AcpContextUsage {
    pub context_used: u64,
    pub context_size: u64,
    pub cost_amount: Option<f64>,
    pub currency: Option<String>,
    /// Deliberately remains unknown: ACP reports context use, not total tokens.
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcpSessionUpdate {
    pub session_id: String,
    pub update: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcpPermissionRequest {
    pub session_id: String,
    pub tool_call: Value,
    pub options: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpPermissionDecision {
    Selected(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AcpPromptOutcome {
    Stopped {
        stop_reason: String,
        context_usage: Option<AcpContextUsage>,
    },
    ResultUnknown {
        error_code: String,
    },
}

pub fn initialize_params(client_name: &str, client_version: &str) -> Value {
    json!({
        "protocolVersion": ACP_PROTOCOL_VERSION,
        "clientCapabilities": {},
        "clientInfo": { "name": client_name, "version": client_version },
    })
}

pub fn new_session_params(cwd: &Path) -> Value {
    json!({
        "cwd": cwd.to_string_lossy(),
        "mcpServers": [],
    })
}

pub fn prompt_params(session_id: &str, prompt: &str) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": [{ "type": "text", "text": prompt }],
    })
}

pub fn cancel_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn parse_context_usage(update: &Value) -> Option<AcpContextUsage> {
    if update.get("sessionUpdate")?.as_str()? != "usage_update" {
        return None;
    }
    let cost = update.get("cost");
    Some(AcpContextUsage {
        context_used: update.get("used")?.as_u64()?,
        context_size: update.get("size")?.as_u64()?,
        cost_amount: cost
            .and_then(|value| value.get("amount"))
            .and_then(Value::as_f64),
        currency: cost
            .and_then(|value| value.get("currency"))
            .and_then(Value::as_str)
            .map(str::to_string),
        total_tokens: None,
    })
}

pub fn agent_message_text(update: &Value) -> Option<&str> {
    (update.get("sessionUpdate")?.as_str()? == "agent_message_chunk")
        .then(|| {
            update
                .get("content")
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
        })
        .flatten()
}

pub fn exposed_tool_name(provider_id: &str) -> Option<String> {
    let normalized = provider_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    (!normalized.is_empty()).then(|| format!("subagent_acp_{normalized}_run"))
}

pub fn external_task_result(provider_id: String) -> ExternalTaskResult {
    ExternalTaskResult {
        provider_id,
        session_id: None,
        state: ExternalSessionState::Starting,
        stop_reason: None,
        response_text: String::new(),
        artifact_paths: Vec::new(),
        model_calls: None,
        total_tokens: None,
        context_usage: None,
        error_code: None,
    }
}
