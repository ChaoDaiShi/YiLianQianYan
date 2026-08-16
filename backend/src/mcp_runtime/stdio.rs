// ============================================================
// Persistent stdio transport — a single child process is spawned once,
// negotiated (modern server/discover with legacy initialize fallback), and
// reused for all subsequent requests. Requests are serialized per server.
// ============================================================

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio_util::sync::CancellationToken;

use super::jsonrpc::{JsonRpcMessage, JsonRpcRequest};
use super::model::{
    McpNegotiationResult, McpProtocolVersion, McpRuntimeError, McpServerCapabilities,
    MAX_MCP_STDIO_LINE_BYTES,
};
use super::protocol::attach_request_metadata;
use super::transport::McpTransport;

const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);

pub struct StdioTransport {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    inner: tokio::sync::Mutex<StdioInner>,
    version: parking_lot::RwLock<McpProtocolVersion>,
    capabilities: parking_lot::RwLock<McpServerCapabilities>,
}

struct StdioInner {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    reader: Option<BufReader<tokio::process::ChildStdout>>,
    next_id: i64,
    reconnect_attempted: bool,
}

impl StdioTransport {
    pub fn new(command: String, args: Vec<String>, env: BTreeMap<String, String>) -> Self {
        Self {
            command,
            args,
            env,
            inner: tokio::sync::Mutex::new(StdioInner {
                child: None,
                stdin: None,
                reader: None,
                next_id: 1,
                reconnect_attempted: false,
            }),
            version: parking_lot::RwLock::new(McpProtocolVersion::V2025_11_25),
            capabilities: parking_lot::RwLock::new(McpServerCapabilities::default()),
        }
    }

    async fn spawn(
        &self,
    ) -> Result<(Child, ChildStdin, BufReader<tokio::process::ChildStdout>), McpRuntimeError> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| McpRuntimeError::SpawnFailed(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or(McpRuntimeError::SpawnFailed("no stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(McpRuntimeError::SpawnFailed("no stdout".to_string()))?;

        // Drain stderr in the background so the child never blocks on a full pipe.
        // stderr is untrusted external data: never log secrets, and bound it.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    if crate::safety::contains_sensitive_content(&line) {
                        tracing::warn!(
                            "mcp server emitted sensitive-looking stderr; content suppressed"
                        );
                    } else {
                        let bounded = crate::utils::text::truncate_chars(line.trim_end(), 500);
                        tracing::debug!(stderr = %bounded, "mcp stdio stderr");
                    }
                    line.clear();
                }
            });
        }

        let reader = BufReader::new(stdout);
        Ok((child, stdin, reader))
    }

    async fn write_request(
        &self,
        stdin: &mut ChildStdin,
        request: &JsonRpcRequest,
    ) -> Result<(), McpRuntimeError> {
        let line = request.to_bounded_string(MAX_MCP_STDIO_LINE_BYTES)?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn read_response(
        &self,
        reader: &mut BufReader<tokio::process::ChildStdout>,
        expected_id: i64,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        loop {
            let line = tokio::select! {
                _ = cancel.cancelled() => return Err(McpRuntimeError::Cancelled),
                l = Self::read_bounded_line(reader) => l?,
            };
            // Skip non-JSON lines (defensive: a server must write only JSON-RPC
            // on stdout, but stray output must not corrupt the request stream).
            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let message = match super::jsonrpc::parse_message(&value) {
                Ok(m) => m,
                Err(_) => continue,
            };
            match &message {
                JsonRpcMessage::Success(s) if s.id == expected_id => return Ok(message),
                JsonRpcMessage::Error(e) if e.id == expected_id => return Ok(message),
                _ => { /* notification or wrong-id: keep reading */ }
            }
        }
    }

    async fn read_bounded_line(
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Result<String, McpRuntimeError> {
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let chunk = reader
                .fill_buf()
                .await
                .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
            if chunk.is_empty() {
                // EOF
                if buf.is_empty() {
                    return Err(McpRuntimeError::Transport("stdio EOF".to_string()));
                }
                return Ok(String::from_utf8_lossy(&buf).to_string());
            }
            if let Some(pos) = chunk.iter().position(|&b| b == b'\n') {
                buf.extend_from_slice(&chunk[..pos]);
                let consumed = pos + 1;
                reader.consume(consumed);
                return Ok(String::from_utf8_lossy(&buf).to_string());
            }
            buf.extend_from_slice(chunk);
            if buf.len() > MAX_MCP_STDIO_LINE_BYTES {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            let n = chunk.len();
            reader.consume(n);
        }
    }

    async fn negotiate(
        &self,
        stdin: &mut ChildStdin,
        reader: &mut BufReader<tokio::process::ChildStdout>,
        next_id: &mut i64,
    ) -> Result<(McpProtocolVersion, McpServerCapabilities), McpRuntimeError> {
        // Try modern server/discover first.
        let mut params = serde_json::json!({});
        attach_request_metadata(&mut params, "0.8.0");
        let discover = JsonRpcRequest::new(*next_id, "server/discover", Some(params));
        *next_id += 1;
        self.write_request(stdin, &discover).await?;
        let cancel = CancellationToken::new();
        match self.read_response(reader, discover.id, &cancel).await {
            Ok(JsonRpcMessage::Success(s)) => Ok((
                McpProtocolVersion::V2026_07_28,
                super::model::parse_capabilities(&s.result),
            )),
            Ok(JsonRpcMessage::Error(e)) if e.error.code == -32601 => {
                // "Method not found" is the clear legacy-compatible signal.
                let init = JsonRpcRequest::new(
                    *next_id,
                    "initialize",
                    Some(serde_json::json!({
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": { "name": "YiLianQianYan", "version": "0.8.0" }
                    })),
                );
                *next_id += 1;
                self.write_request(stdin, &init).await?;
                let caps = match self.read_response(reader, init.id, &cancel).await? {
                    JsonRpcMessage::Success(s) => super::model::parse_capabilities(&s.result),
                    _ => McpServerCapabilities::default(),
                };
                // Send initialized notification (no id).
                let initialized = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized",
                    "params": {}
                });
                let line = serde_json::to_string(&initialized)
                    .map_err(|e| McpRuntimeError::Protocol(e.to_string()))?;
                stdin
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| McpRuntimeError::Transport(e.to_string()))?;
                Ok((McpProtocolVersion::V2025_11_25, caps))
            }
            // A recognized modern error that is NOT "method not found" (e.g. an
            // explicit UnsupportedProtocolVersion) must NOT be blindly treated
            // as a legacy server.
            Ok(JsonRpcMessage::Error(_)) => Err(McpRuntimeError::UnsupportedProtocol),
            Ok(JsonRpcMessage::Notification(_)) => Err(McpRuntimeError::InvalidResponse),
            Err(e) => Err(e),
        }
    }

    async fn ensure_connected(&self, inner: &mut StdioInner) -> Result<(), McpRuntimeError> {
        if inner.child.is_some() {
            return Ok(());
        }
        let (mut child, mut stdin, mut reader) = self.spawn().await?;
        let negotiated = self
            .negotiate(&mut stdin, &mut reader, &mut inner.next_id)
            .await;
        match negotiated {
            Ok((v, caps)) => {
                *self.version.write() = v;
                *self.capabilities.write() = caps;
                inner.child = Some(child);
                inner.stdin = Some(stdin);
                inner.reader = Some(reader);
                inner.reconnect_attempted = false;
                Ok(())
            }
            Err(e) => {
                let _ = child.kill().await;
                Err(e)
            }
        }
    }

    async fn send_locked(
        &self,
        request: &JsonRpcRequest,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        let mut inner = self.inner.lock().await;
        self.ensure_connected(&mut inner).await?;
        match self.do_send(&mut inner, request, cancel).await {
            Ok(msg) => Ok(msg),
            Err(e) => {
                // Child likely died — mark disconnected and allow one reconnect.
                inner.child = None;
                inner.stdin = None;
                inner.reader = None;
                if inner.reconnect_attempted {
                    return Err(e);
                }
                inner.reconnect_attempted = true;
                self.ensure_connected(&mut inner).await?;
                self.do_send(&mut inner, request, cancel).await
            }
        }
    }

    async fn do_send(
        &self,
        inner: &mut StdioInner,
        request: &JsonRpcRequest,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        let StdioInner { stdin, reader, .. } = inner;
        let stdin = stdin
            .as_mut()
            .ok_or(McpRuntimeError::Transport("no stdin".to_string()))?;
        let reader = reader
            .as_mut()
            .ok_or(McpRuntimeError::Transport("no stdout".to_string()))?;
        self.write_request(stdin, request).await?;
        self.read_response(reader, request.id, cancel).await
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn connect(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<McpNegotiationResult, McpRuntimeError> {
        let mut inner = self.inner.lock().await;
        self.ensure_connected(&mut inner).await?;
        Ok(McpNegotiationResult {
            protocol_version: *self.version.read(),
            capabilities: self.capabilities.read().clone(),
        })
    }

    async fn send_with_options(
        &self,
        request: &JsonRpcRequest,
        _options: &super::transport::McpRequestOptions,
        cancel: &CancellationToken,
    ) -> Result<JsonRpcMessage, McpRuntimeError> {
        // stdio has no HTTP headers; options are ignored.
        self.send_locked(request, cancel).await
    }

    async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(mut child) = inner.child.take() {
            inner.stdin = None;
            inner.reader = None;
            // Best-effort graceful: close stdin (dropped) then wait briefly.
            let _ = tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await;
            let _ = child.kill().await;
        }
    }

    fn protocol_version(&self) -> McpProtocolVersion {
        *self.version.read()
    }
}
