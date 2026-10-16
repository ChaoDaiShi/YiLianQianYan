use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use super::model::{
    cancel_params, initialize_params, new_session_params, prompt_params, AcpContextUsage,
    AcpLaunchConfig, AcpPermissionDecision, AcpPermissionRequest, AcpPromptOutcome,
    AcpSessionUpdate, ACP_CLIENT_NAME, ACP_CLIENT_VERSION, ACP_PROTOCOL_VERSION,
};

mod reader;

const UPDATE_BUFFER: usize = 256;
const CANCEL_RESPONSE_GRACE: Duration = Duration::from_secs(5);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AcpError {
    #[error("ACP configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("ACP agent process could not be started")]
    SpawnFailed,
    #[error("ACP stdio transport failed: {0}")]
    Transport(String),
    #[error("ACP protocol error: {0}")]
    Protocol(String),
    #[error("ACP agent returned JSON-RPC error {code}: {message}")]
    Server { code: i64, message: String },
    #[error("ACP agent selected unsupported protocol version {0}")]
    UnsupportedVersion(u64),
    #[error("ACP request timed out: {0}")]
    Timeout(String),
    #[error("ACP result is unknown after {0} was sent")]
    ResultUnknown(String),
    #[error("ACP request was cancelled")]
    Cancelled,
}

#[async_trait]
pub trait AcpPermissionHandler: Send + Sync {
    /// Route the provider's permission request through the caller's existing
    /// approval/Gateway flow. Returning `Cancelled` never grants the action.
    async fn request_permission(
        &self,
        request: AcpPermissionRequest,
        cancel: CancellationToken,
    ) -> Result<AcpPermissionDecision, AcpError>;
}

struct PendingResponse {
    method: String,
    sender: oneshot::Sender<Result<Value, AcpError>>,
}

struct PendingPermission {
    session_id: String,
    cancel: CancellationToken,
}

struct AcpInner {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Mutex<HashMap<u64, PendingResponse>>,
    permissions: Mutex<HashMap<u64, PendingPermission>>,
    sessions: Mutex<HashSet<String>>,
    usage: Mutex<HashMap<String, AcpContextUsage>>,
    updates: broadcast::Sender<AcpSessionUpdate>,
    permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
    next_id: AtomicU64,
    timeout: Duration,
    initialized: AtomicBool,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct AcpClient {
    inner: Arc<AcpInner>,
}

impl std::fmt::Debug for AcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpClient")
            .field(
                "initialized",
                &self.inner.initialized.load(Ordering::Relaxed),
            )
            .field("closed", &self.inner.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AcpClient {
    pub async fn spawn(
        config: &AcpLaunchConfig,
        permission_handler: Option<Arc<dyn AcpPermissionHandler>>,
    ) -> Result<Self, AcpError> {
        let cwd = validate_working_directory(&config.cwd)?;
        if !config.executable.is_absolute() || !config.executable.is_file() {
            return Err(AcpError::InvalidConfig(
                "executable must be an existing absolute file path".to_string(),
            ));
        }
        if config.timeout_seconds == 0 {
            return Err(AcpError::InvalidConfig(
                "timeout_seconds must be greater than zero".to_string(),
            ));
        }

        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .kill_on_drop(true);
        for key in &config.env_allowlist {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        #[cfg(windows)]
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|_| AcpError::SpawnFailed)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::Transport("child stdin unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::Transport("child stdout unavailable".to_string()))?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }

        let (updates, _) = broadcast::channel(UPDATE_BUFFER);
        let inner = Arc::new(AcpInner {
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            pending: Mutex::new(HashMap::new()),
            permissions: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashSet::new()),
            usage: Mutex::new(HashMap::new()),
            updates,
            permission_handler,
            next_id: AtomicU64::new(0),
            timeout: Duration::from_secs(config.timeout_seconds),
            initialized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        });
        tokio::spawn(reader::read_messages(Arc::clone(&inner), stdout));
        Ok(Self { inner })
    }

    pub fn subscribe_updates(&self) -> broadcast::Receiver<AcpSessionUpdate> {
        self.inner.updates.subscribe()
    }

    pub async fn latest_usage(&self, session_id: &str) -> Option<AcpContextUsage> {
        self.inner.usage.lock().await.get(session_id).cloned()
    }

    pub async fn initialize(&self) -> Result<Value, AcpError> {
        let result = self
            .call(
                "initialize",
                initialize_params(ACP_CLIENT_NAME, ACP_CLIENT_VERSION),
            )
            .await?;
        let version = result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                AcpError::Protocol("initialize response omitted protocolVersion".to_string())
            })?;
        if version != ACP_PROTOCOL_VERSION {
            return Err(AcpError::UnsupportedVersion(version));
        }
        if !result
            .get("agentCapabilities")
            .is_some_and(Value::is_object)
        {
            return Err(AcpError::Protocol(
                "initialize response omitted agentCapabilities".to_string(),
            ));
        }
        self.inner.initialized.store(true, Ordering::Release);
        Ok(result)
    }

    pub async fn new_session(&self, cwd: &Path) -> Result<String, AcpError> {
        if !self.inner.initialized.load(Ordering::Acquire) {
            return Err(AcpError::Protocol(
                "initialize must complete before session/new".to_string(),
            ));
        }
        let cwd = validate_working_directory(cwd)?;
        let result = self.call("session/new", new_session_params(&cwd)).await?;
        let session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AcpError::Protocol("session/new response omitted sessionId".to_string())
            })?
            .to_string();
        self.inner.sessions.lock().await.insert(session_id.clone());
        Ok(session_id)
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        prompt: &str,
        cancel: &CancellationToken,
    ) -> Result<AcpPromptOutcome, AcpError> {
        if !self.inner.sessions.lock().await.contains(session_id) {
            return Err(AcpError::Protocol(
                "session/prompt references an unknown session".to_string(),
            ));
        }
        if cancel.is_cancelled() {
            return Ok(AcpPromptOutcome::Stopped {
                stop_reason: "cancelled".to_string(),
                context_usage: self.latest_usage(session_id).await,
            });
        }
        let (request_id, mut response) = self
            .start_request("session/prompt", prompt_params(session_id, prompt))
            .await?;
        tokio::select! {
            biased;
            response_result = &mut response => {
                self.read_prompt_response(session_id, response_result).await
            }
            _ = cancel.cancelled() => {
                self.cancel_prompt_and_wait(session_id, request_id, &mut response, "acp_cancelled_result_unknown").await
            }
            _ = tokio::time::sleep(self.inner.timeout) => {
                self.cancel_prompt_and_wait(session_id, request_id, &mut response, "acp_prompt_timeout_result_unknown").await
            }
        }
    }

    pub async fn cancel_session(&self, session_id: &str) -> Result<(), AcpError> {
        if !self.inner.sessions.lock().await.contains(session_id) {
            return Err(AcpError::Protocol(
                "session/cancel references an unknown session".to_string(),
            ));
        }
        self.cancel_permission_requests(session_id).await;
        self.call_with_timeout(
            "session/cancel",
            cancel_params(session_id),
            CANCEL_RESPONSE_GRACE,
        )
        .await
        .map(|_| ())
    }

    pub async fn shutdown(&self) {
        if self.inner.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancel_all_permission_requests().await;
        self.inner.stdin.lock().await.take();
        let mut child = self.inner.child.lock().await.take();
        if let Some(child) = child.as_mut() {
            if tokio::time::timeout(SHUTDOWN_GRACE, child.wait())
                .await
                .is_err()
            {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
    }

    async fn prompt_outcome(&self, session_id: &str, response: Value) -> AcpPromptOutcome {
        let stop_reason = response
            .get("stopReason")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let context_usage = self.latest_usage(session_id).await;
        if stop_reason == "unknown" {
            AcpPromptOutcome::ResultUnknown {
                error_code: "acp_prompt_stop_reason_missing".to_string(),
            }
        } else {
            AcpPromptOutcome::Stopped {
                stop_reason,
                context_usage,
            }
        }
    }

    async fn read_prompt_response(
        &self,
        session_id: &str,
        response: Result<Result<Value, AcpError>, oneshot::error::RecvError>,
    ) -> Result<AcpPromptOutcome, AcpError> {
        match response {
            Ok(Ok(value)) => Ok(self.prompt_outcome(session_id, value).await),
            Ok(Err(AcpError::ResultUnknown(_))) => Ok(AcpPromptOutcome::ResultUnknown {
                error_code: "acp_process_or_response_lost".to_string(),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Ok(AcpPromptOutcome::ResultUnknown {
                error_code: "acp_response_channel_closed".to_string(),
            }),
        }
    }

    async fn cancel_prompt_and_wait(
        &self,
        session_id: &str,
        request_id: u64,
        response: &mut oneshot::Receiver<Result<Value, AcpError>>,
        unknown_code: &str,
    ) -> Result<AcpPromptOutcome, AcpError> {
        let _ = self.cancel_session(session_id).await;
        match tokio::time::timeout(CANCEL_RESPONSE_GRACE, response).await {
            Ok(Ok(Ok(value))) => Ok(self.prompt_outcome(session_id, value).await),
            Ok(Ok(Err(_))) | Ok(Err(_)) | Err(_) => {
                self.inner.pending.lock().await.remove(&request_id);
                Ok(AcpPromptOutcome::ResultUnknown {
                    error_code: unknown_code.to_string(),
                })
            }
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        self.call_with_timeout(method, params, self.inner.timeout)
            .await
    }

    async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, AcpError> {
        let (request_id, response) = self.start_request(method, params).await?;
        match tokio::time::timeout(timeout, response).await {
            Ok(Ok(value)) => value,
            Ok(Err(_)) => Err(AcpError::ResultUnknown(method.to_string())),
            Err(_) => {
                self.inner.pending.lock().await.remove(&request_id);
                if method == "session/prompt" || method == "session/new" {
                    Err(AcpError::ResultUnknown(method.to_string()))
                } else {
                    Err(AcpError::Timeout(method.to_string()))
                }
            }
        }
    }

    async fn start_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, oneshot::Receiver<Result<Value, AcpError>>), AcpError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(AcpError::Transport("ACP process is closed".to_string()));
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = oneshot::channel();
        self.inner.pending.lock().await.insert(
            id,
            PendingResponse {
                method: method.to_string(),
                sender,
            },
        );
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(error) = self.write_message(&message).await {
            self.inner.pending.lock().await.remove(&id);
            return Err(error);
        }
        Ok((id, receiver))
    }

    async fn write_message(&self, message: &Value) -> Result<(), AcpError> {
        let mut stdin = self.inner.stdin.lock().await;
        let stdin = stdin
            .as_mut()
            .ok_or_else(|| AcpError::Transport("ACP stdin is closed".to_string()))?;
        let mut bytes =
            serde_json::to_vec(message).map_err(|error| AcpError::Protocol(error.to_string()))?;
        bytes.push(b'\n');
        stdin
            .write_all(&bytes)
            .await
            .map_err(|error| AcpError::Transport(error.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|error| AcpError::Transport(error.to_string()))
    }

    async fn cancel_permission_requests(&self, session_id: &str) {
        let cancelled = {
            let mut requests = self.inner.permissions.lock().await;
            let ids: Vec<u64> = requests
                .iter()
                .filter_map(|(id, request)| (request.session_id == session_id).then_some(*id))
                .collect();
            ids.into_iter()
                .filter_map(|id| requests.remove(&id).map(|request| (id, request)))
                .collect::<Vec<_>>()
        };
        for (id, request) in cancelled {
            request.cancel.cancel();
            let _ = self
                .write_message(&json!({
                    "jsonrpc":"2.0", "id":id,
                    "result":{"outcome":{"outcome":"cancelled"}}
                }))
                .await;
        }
    }

    async fn cancel_all_permission_requests(&self) {
        let cancelled = {
            let mut requests = self.inner.permissions.lock().await;
            requests.drain().collect::<Vec<_>>()
        };
        for (id, request) in cancelled {
            request.cancel.cancel();
            let _ = self
                .write_message(&json!({
                    "jsonrpc":"2.0", "id":id,
                    "result":{"outcome":{"outcome":"cancelled"}}
                }))
                .await;
        }
    }
}

fn validate_working_directory(path: &Path) -> Result<std::path::PathBuf, AcpError> {
    if !path.is_absolute() {
        return Err(AcpError::InvalidConfig(
            "cwd must be an absolute task working directory".to_string(),
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| AcpError::InvalidConfig("cwd must exist as a directory".to_string()))?;
    if !canonical.is_dir() {
        return Err(AcpError::InvalidConfig(
            "cwd must exist as a directory".to_string(),
        ));
    }
    Ok(canonical)
}
