// ============================================================
// MCP Client Protocol Foundation — stdio JSON-RPC handshake +
// one-shot tool execution.
//
// Implements the minimal MCP client lifecycle over a stdio child
// process:
//
//   initialize
//   → notifications/initialized
//   → tools/list (with pagination)
//   → tools/call (single remote tool execution)
//
// The tools/call protocol and a bounded ToolResult bridge are provided
// as an execution foundation. MCP tools are NOT registered with the
// Agent, and McpToolAdapter::execute() remains fail-closed.
// ============================================================

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

use crate::db::McpServer;
use crate::tools::trait_def::ToolResult;

/// MCP protocol version supported by this client.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Whole-probe timeout (spawn + handshake + tools/list).
pub const MCP_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// Timeout for a one-shot stdio tools/call (initialize + initialized + call).
pub const MCP_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum characters of the final `ToolResult.content` (UTF-8 safe truncation).
pub const MAX_MCP_TOOL_RESULT_CHARS: usize = 16_000;

/// Safety cap on pages fetched from a single tools/list pagination loop.
pub const MAX_TOOL_LIST_PAGES: usize = 20;

/// Safety cap on messages consumed while waiting for one response.
const MAX_MESSAGES_PER_RESPONSE: usize = 100;

// ── Errors ──

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("invalid MCP config: {0}")]
    InvalidConfig(String),
    #[error("failed to start MCP server: {0}")]
    Spawn(String),
    #[error("MCP I/O error: {0}")]
    Io(String),
    #[error("invalid MCP JSON-RPC message: {0}")]
    InvalidMessage(String),
    #[error("MCP server returned error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("unsupported MCP protocol version: {0}")]
    ProtocolVersionMismatch(String),
    #[error("MCP operation timed out")]
    Timeout,
    #[error("MCP tools/list exceeded pagination limit")]
    PaginationLimit,
    #[error("invalid MCP tool call: {0}")]
    InvalidToolCall(String),
}

// ── Result types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpProbeResult {
    pub protocol_version: String,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub tools: Vec<McpTool>,
}

#[derive(Debug, Clone)]
pub struct McpInitializeInfo {
    pub protocol_version: String,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
}

/// The `CallToolResult` returned by a `tools/call` request.
///
/// `is_error` is a tool-level error (e.g. invalid input), distinct from a
/// JSON-RPC error which is a protocol/request failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallResult {
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(rename = "structuredContent", default)]
    pub structured_content: Option<serde_json::Value>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

// ── Public entry point ──

/// Spawn a stdio MCP server, complete the initialize + tools/list
/// handshake, and return discovered metadata.
///
/// The child process is always cleaned up (killed + reaped) whether the
/// probe succeeds or fails.
pub async fn probe_stdio_server(server: &McpServer) -> Result<McpProbeResult, McpError> {
    probe_stdio_server_with_timeout(server, MCP_PROBE_TIMEOUT).await
}

async fn probe_stdio_server_with_timeout(
    server: &McpServer,
    timeout: Duration,
) -> Result<McpProbeResult, McpError> {
    tokio::time::timeout(timeout, probe_stdio_server_inner(server))
        .await
        .map_err(|_| McpError::Timeout)?
}

async fn probe_stdio_server_inner(server: &McpServer) -> Result<McpProbeResult, McpError> {
    let command = server
        .command
        .as_deref()
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .ok_or_else(|| McpError::InvalidConfig("command is required".to_string()))?;
    let args = server.args.clone().unwrap_or_default();
    let envs = resolve_env(&server.env)?;

    let mut cmd = tokio::process::Command::new(command);
    cmd.args(&args);
    for (key, value) in &envs {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| McpError::Spawn(e.to_string()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpError::Io("child stdout unavailable".to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpError::Io("child stdin unavailable".to_string()))?;
    let io = tokio::io::join(stdout, stdin);

    let mut session = McpSession::new(io);
    let result = run_probe(&mut session).await;

    // Best-effort cleanup: drop session (closes stdin), kill + reap child.
    drop(session);
    let _ = child.kill().await;
    let _ = child.wait().await;

    result
}

async fn run_probe<IO>(session: &mut McpSession<IO>) -> Result<McpProbeResult, McpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    let init = session.initialize().await?;
    session.send_initialized().await?;
    let tools = session.list_tools().await?;
    Ok(McpProbeResult {
        protocol_version: init.protocol_version,
        server_name: init.server_name,
        server_version: init.server_version,
        tools,
    })
}

// ── One-shot tools/call execution ──

/// Spawn a stdio MCP server, handshake, execute one remote tool, and
/// return the bounded [`ToolResult`].
///
/// The child process is owned OUTSIDE the timeout so that on timeout (or any
/// error) we can explicitly kill + reap it. `kill_on_drop(true)` remains as a
/// final safety net.
pub async fn call_stdio_tool(
    server: &McpServer,
    remote_tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolResult, McpError> {
    if remote_tool_name.trim().is_empty() {
        return Err(McpError::InvalidToolCall(
            "remote tool name is empty".to_string(),
        ));
    }
    if !arguments.is_object() {
        return Err(McpError::InvalidToolCall(
            "arguments must be a JSON object".to_string(),
        ));
    }

    let command = server
        .command
        .as_deref()
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .ok_or_else(|| McpError::InvalidConfig("command is required".to_string()))?;
    let args = server.args.clone().unwrap_or_default();
    let envs = resolve_env(&server.env)?;

    let mut cmd = tokio::process::Command::new(command);
    cmd.args(&args);
    for (key, value) in &envs {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| McpError::Spawn(e.to_string()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpError::Io("child stdout unavailable".to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpError::Io("child stdin unavailable".to_string()))?;
    let io = tokio::io::join(stdout, stdin);

    let mut session = McpSession::new(io);

    let protocol_result = tokio::time::timeout(
        MCP_TOOL_CALL_TIMEOUT,
        run_tool_call(&mut session, remote_tool_name, arguments),
    )
    .await;

    // Child ownership is outside the timeout: always clean up explicitly.
    drop(session);
    let _ = child.kill().await;
    let _ = child.wait().await;

    match protocol_result {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(McpError::Timeout),
    }
}

async fn run_tool_call<IO>(
    session: &mut McpSession<IO>,
    remote_tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolResult, McpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    session.initialize().await?;
    session.send_initialized().await?;
    let result = session.call_tool(remote_tool_name, arguments).await?;
    Ok(result.into_tool_result())
}

impl McpCallResult {
    /// Convert an MCP `CallToolResult` into the project's [`ToolResult`].
    ///
    /// Text content blocks are joined in order; non-text blocks are replaced
    /// with short placeholders so binary/base64 payloads are never leaked.
    pub fn into_tool_result(self) -> ToolResult {
        let mut parts: Vec<String> = Vec::new();

        for block in &self.content {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        parts.push(text.to_string());
                    }
                }
                "image" => parts.push("[MCP image content omitted]".to_string()),
                "audio" => parts.push("[MCP audio content omitted]".to_string()),
                "resource" => parts.push("[MCP resource content omitted]".to_string()),
                "resource_link" => parts.push("[MCP resource_link content omitted]".to_string()),
                "" => parts.push("[MCP content block without type]".to_string()),
                other => parts.push(format!("[MCP unsupported content type: {other}]")),
            }
        }

        if let Some(structured) = &self.structured_content {
            parts.push("[MCP structured content]".to_string());
            parts.push(structured.to_string());
        }

        let content = if parts.is_empty() {
            if self.is_error {
                "MCP tool failed without error content".to_string()
            } else {
                "MCP tool returned no content".to_string()
            }
        } else {
            let joined = parts.join("\n");
            crate::utils::text::truncate_chars(&joined, MAX_MCP_TOOL_RESULT_CHARS)
        };

        if self.is_error {
            ToolResult::error(content)
        } else {
            ToolResult::success(content)
        }
    }
}

// ── Config resolution ──

/// Validate that `env` is a JSON object of string values.
/// Never returns or logs the env values themselves.
fn resolve_env(env: &Option<serde_json::Value>) -> Result<Vec<(String, String)>, McpError> {
    let Some(value) = env else {
        return Ok(Vec::new());
    };
    let obj = value.as_object().ok_or_else(|| {
        McpError::InvalidConfig("env must be a JSON object of string values".to_string())
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (key, val) in obj {
        let s = val.as_str().ok_or_else(|| {
            McpError::InvalidConfig(format!("env value for key '{key}' must be a string"))
        })?;
        out.push((key.clone(), s.to_string()));
    }
    Ok(out)
}

// ── Protocol session ──

/// A stateful JSON-RPC client session over any `AsyncRead + AsyncWrite`
/// pair (a stdio child, or an in-memory duplex for tests).
pub struct McpSession<IO> {
    reader: BufReader<tokio::io::ReadHalf<IO>>,
    writer: BufWriter<tokio::io::WriteHalf<IO>>,
    next_id: i64,
}

impl<IO> McpSession<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(io: IO) -> Self {
        let (reader, writer) = tokio::io::split(io);
        Self {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            next_id: 1,
        }
    }

    fn next_request_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Send `initialize` and validate the response protocol version.
    pub async fn initialize(&mut self) -> Result<McpInitializeInfo, McpError> {
        let id = self.next_request_id();
        let params = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "yilianqianyan",
                "version": env!("CARGO_PKG_VERSION"),
            }
        });
        self.write_request(id, "initialize", params).await?;
        let resp = self.read_response_for_id(id).await?;

        if let Some(error) = resp.get("error") {
            return Err(McpError::Rpc {
                code: error["code"].as_i64().unwrap_or(0),
                message: error["message"]
                    .as_str()
                    .unwrap_or("unknown MCP error")
                    .to_string(),
            });
        }

        let result = resp.get("result").ok_or_else(|| {
            McpError::InvalidMessage("initialize response missing 'result'".to_string())
        })?;
        let protocol_version = result
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpError::InvalidMessage(
                    "initialize response missing 'protocolVersion'".to_string(),
                )
            })?
            .to_string();

        if protocol_version != MCP_PROTOCOL_VERSION {
            return Err(McpError::ProtocolVersionMismatch(protocol_version));
        }

        let server_info = result.get("serverInfo");
        let server_name = server_info
            .and_then(|si| si.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string);
        let server_version = server_info
            .and_then(|si| si.get("version"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        Ok(McpInitializeInfo {
            protocol_version,
            server_name,
            server_version,
        })
    }

    /// Send the `notifications/initialized` notification (no id, no wait).
    pub async fn send_initialized(&mut self) -> Result<(), McpError> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.write_line(&msg).await
    }

    /// Call `tools/list`, following `nextCursor` pagination.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;

        for _page in 1..=MAX_TOOL_LIST_PAGES {
            let id = self.next_request_id();
            let params = match &cursor {
                Some(c) => serde_json::json!({ "cursor": c }),
                None => serde_json::json!({}),
            };
            self.write_request(id, "tools/list", params).await?;
            let resp = self.read_response_for_id(id).await?;

            if let Some(error) = resp.get("error") {
                return Err(McpError::Rpc {
                    code: error["code"].as_i64().unwrap_or(0),
                    message: error["message"]
                        .as_str()
                        .unwrap_or("unknown MCP error")
                        .to_string(),
                });
            }

            let result = resp.get("result").ok_or_else(|| {
                McpError::InvalidMessage("tools/list response missing 'result'".to_string())
            })?;
            let page_tools: Vec<McpTool> = serde_json::from_value(
                result
                    .get("tools")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .map_err(|e| McpError::InvalidMessage(format!("failed to parse tools: {e}")))?;
            tools.extend(page_tools);

            cursor = result
                .get("nextCursor")
                .and_then(|c| c.as_str())
                .map(str::to_string);
            if cursor.is_none() {
                return Ok(tools);
            }
        }

        Err(McpError::PaginationLimit)
    }

    /// Execute a single remote tool via `tools/call`.
    ///
    /// `remote_tool_name` must be the MCP server's original tool name (not the
    /// namespaced `mcp_*` adapter name). `arguments` must be a JSON object.
    pub async fn call_tool(
        &mut self,
        remote_tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpCallResult, McpError> {
        if remote_tool_name.trim().is_empty() {
            return Err(McpError::InvalidToolCall(
                "remote tool name is empty".to_string(),
            ));
        }
        if !arguments.is_object() {
            return Err(McpError::InvalidToolCall(
                "arguments must be a JSON object".to_string(),
            ));
        }

        let id = self.next_request_id();
        let params = serde_json::json!({
            "name": remote_tool_name,
            "arguments": arguments,
        });
        self.write_request(id, "tools/call", params).await?;
        let resp = self.read_response_for_id(id).await?;

        if let Some(error) = resp.get("error") {
            return Err(McpError::Rpc {
                code: error["code"].as_i64().unwrap_or(0),
                message: error["message"]
                    .as_str()
                    .unwrap_or("unknown MCP error")
                    .to_string(),
            });
        }

        let result = resp.get("result").ok_or_else(|| {
            McpError::InvalidMessage("tools/call response missing 'result'".to_string())
        })?;
        let content = result.get("content").ok_or_else(|| {
            McpError::InvalidMessage("tools/call result missing 'content'".to_string())
        })?;
        if !content.is_array() {
            return Err(McpError::InvalidMessage(
                "tools/call result 'content' must be an array".to_string(),
            ));
        }

        let structured_content = result.get("structuredContent");
        if let Some(sc) = structured_content {
            if !sc.is_object() {
                return Err(McpError::InvalidMessage(
                    "tools/call 'structuredContent' must be a JSON object".to_string(),
                ));
            }
        }

        Ok(McpCallResult {
            content: content.as_array().cloned().unwrap_or_default(),
            structured_content: structured_content.cloned(),
            is_error: result
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    async fn write_request(
        &mut self,
        id: i64,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), McpError> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_line(&msg).await
    }

    async fn write_line(&mut self, msg: &serde_json::Value) -> Result<(), McpError> {
        let mut line =
            serde_json::to_string(msg).map_err(|e| McpError::InvalidMessage(e.to_string()))?;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| McpError::Io(e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| McpError::Io(e.to_string()))?;
        Ok(())
    }

    /// Read stdout lines until the message with `expected_id` arrives.
    /// Notifications and messages for other ids are skipped.
    async fn read_response_for_id(
        &mut self,
        expected_id: i64,
    ) -> Result<serde_json::Value, McpError> {
        let mut line = String::new();
        for _ in 0..MAX_MESSAGES_PER_RESPONSE {
            line.clear();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| McpError::Io(e.to_string()))?;
            if n == 0 {
                return Err(McpError::Io(
                    "MCP server closed stdout while waiting for a response".to_string(),
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let msg: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
                McpError::InvalidMessage(format!("failed to parse JSON-RPC message: {e}"))
            })?;

            // Notification: has a method but no id → ignore.
            if msg.get("method").is_some() && msg.get("id").is_none() {
                continue;
            }
            // Not the id we're waiting for → skip.
            if msg.get("id").and_then(|v| v.as_i64()) != Some(expected_id) {
                continue;
            }
            return Ok(msg);
        }
        Err(McpError::InvalidMessage(
            "exceeded message limit while waiting for a response".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncWriteExt, BufReader, DuplexStream};

    fn duplex_pair() -> (DuplexStream, DuplexStream) {
        tokio::io::duplex(8192)
    }

    /// A persistent buffered reader so the mock server never loses
    /// buffered lines between reads.
    type MockReader = BufReader<DuplexStream>;

    async fn read_server_line(reader: &mut MockReader) -> serde_json::Value {
        let mut buf = String::new();
        AsyncBufReadExt::read_line(reader, &mut buf).await.unwrap();
        serde_json::from_str(buf.trim()).unwrap()
    }

    async fn write_server_line(reader: &mut MockReader, msg: &serde_json::Value) {
        let mut line = serde_json::to_string(msg).unwrap();
        line.push('\n');
        let io = reader.get_mut();
        io.write_all(line.as_bytes()).await.unwrap();
        io.flush().await.unwrap();
    }

    fn initialize_response(protocol_version: &str) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": protocol_version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "filesystem", "version": "1.0.0" }
            }
        })
    }

    // ── 1. initialize request is correct ──

    #[tokio::test]
    async fn initialize_request_is_correct() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let req = read_server_line(&mut reader).await;
            assert_eq!(req["jsonrpc"], "2.0");
            assert_eq!(req["method"], "initialize");
            assert_eq!(req["params"]["protocolVersion"], MCP_PROTOCOL_VERSION);
            assert_eq!(req["params"]["clientInfo"]["name"], "yilianqianyan");
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        });

        let mut session = McpSession::new(client);
        let info = session.initialize().await.unwrap();
        assert_eq!(info.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(info.server_name.as_deref(), Some("filesystem"));
        assert_eq!(info.server_version.as_deref(), Some("1.0.0"));
        mock.await.unwrap();
    }

    // ── 2. initialized notification is sent (no id) ──

    #[tokio::test]
    async fn initialized_notification_is_sent_without_id() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let notif = read_server_line(&mut reader).await;
            assert_eq!(notif["method"], "notifications/initialized");
            assert!(notif.get("id").is_none());
        });

        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        mock.await.unwrap();
    }

    // ── 3. tools/list is parsed ──

    #[tokio::test]
    async fn tools_list_parsed() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let notif = read_server_line(&mut reader).await;
            assert_eq!(notif["method"], "notifications/initialized");
            let list = read_server_line(&mut reader).await;
            assert_eq!(list["method"], "tools/list");
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "tools": [
                            { "name": "echo", "description": "Echo input", "inputSchema": { "type": "object" } }
                        ]
                    }
                }),
            )
            .await;
        });

        let mut session = McpSession::new(client);
        let _info = session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let tools = session.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].description.as_deref(), Some("Echo input"));
        assert_eq!(tools[0].input_schema["type"], "object");
        mock.await.unwrap();
    }

    // ── 4. notification before response does not misalign ──

    #[tokio::test]
    async fn notification_before_response_does_not_misalign() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
                                                         // Send a notification first, then the response.
            write_server_line(
                &mut reader,
                &json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
            )
            .await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        });

        let mut session = McpSession::new(client);
        let info = session.initialize().await.unwrap();
        assert_eq!(info.protocol_version, MCP_PROTOCOL_VERSION);
        mock.await.unwrap();
    }

    // ── 5. RPC error is surfaced ──

    #[tokio::test]
    async fn rpc_error_is_reported() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": { "code": -32601, "message": "Method not found" }
                }),
            )
            .await;
        });

        let mut session = McpSession::new(client);
        let err = session.initialize().await.unwrap_err();
        assert!(matches!(err, McpError::Rpc { code: -32601, .. }));
        mock.await.unwrap();
    }

    // ── 6. protocol version mismatch ──

    #[tokio::test]
    async fn protocol_version_mismatch_is_reported() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response("2024-11-05")).await;
        });

        let mut session = McpSession::new(client);
        let err = session.initialize().await.unwrap_err();
        assert!(matches!(
            err,
            McpError::ProtocolVersionMismatch(v) if v == "2024-11-05"
        ));
        mock.await.unwrap();
    }

    // ── 7. pagination merges pages ──

    #[tokio::test]
    async fn tools_list_pagination_merges_pages() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await; // initialized
                                                         // Page 1
            let l1 = read_server_line(&mut reader).await;
            let id1 = l1["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id1,
                    "result": { "tools": [{ "name": "A", "inputSchema": {} }], "nextCursor": "page2" }
                }),
            )
            .await;
            // Page 2
            let l2 = read_server_line(&mut reader).await;
            assert_eq!(l2["params"]["cursor"], "page2");
            let id2 = l2["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id2,
                    "result": { "tools": [{ "name": "B", "inputSchema": {} }] }
                }),
            )
            .await;
        });

        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let tools = session.list_tools().await.unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B"]);
        mock.await.unwrap();
    }

    // ── 8. pagination limit ──

    #[tokio::test]
    async fn tools_list_pagination_limit() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await; // initialized
                                                         // Keep replying with nextCursor until the client drops.
            loop {
                let line = match read_server_line_opt(&mut reader).await {
                    Some(line) => line,
                    None => break, // client closed
                };
                let id = line["id"].as_i64().unwrap();
                write_server_line(
                    &mut reader,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "tools": [], "nextCursor": "again" }
                    }),
                )
                .await;
            }
        });

        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let err = session.list_tools().await.unwrap_err();
        assert!(matches!(err, McpError::PaginationLimit));
        // Drop the client session to close the duplex, letting the mock
        // observe EOF and finish its read-until-close loop.
        drop(session);
        mock.await.unwrap();
    }

    /// Read a server line, returning `None` on EOF.
    async fn read_server_line_opt(reader: &mut MockReader) -> Option<serde_json::Value> {
        let mut buf = String::new();
        let n = AsyncBufReadExt::read_line(reader, &mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        serde_json::from_str(buf.trim()).ok()
    }

    // ── 9. invalid env is rejected ──

    #[test]
    fn invalid_env_is_rejected() {
        let env = Some(json!({ "TOKEN": 123 }));
        assert!(matches!(resolve_env(&env), Err(McpError::InvalidConfig(_))));
    }

    #[test]
    fn valid_env_is_accepted() {
        let env = Some(json!({ "TOKEN": "abc", "EMPTY": "" }));
        let resolved = resolve_env(&env).unwrap();
        assert!(resolved.contains(&("TOKEN".to_string(), "abc".to_string())));
    }

    // ── 10. missing command is rejected ──

    #[test]
    fn missing_command_is_rejected() {
        let server = McpServer {
            id: "id".to_string(),
            name: "n".to_string(),
            transport: "stdio".to_string(),
            command: None,
            args: None,
            url: None,
            env: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(probe_stdio_server(&server)).unwrap_err();
        assert!(matches!(err, McpError::InvalidConfig(_)));
    }

    // ── 11. timeout does not hang ──

    #[tokio::test]
    async fn initialize_times_out_when_server_is_silent() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
                                                         // Never respond.
            tokio::time::sleep(Duration::from_secs(30)).await;
        });

        let mut session = McpSession::new(client);
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(Duration::from_millis(200), session.initialize()).await;
        assert!(result.is_err()); // timed out
        assert!(start.elapsed() < Duration::from_secs(5));
        mock.abort();
    }

    // ── tools/call protocol tests ──

    async fn run_call_session(
        mock: tokio::task::JoinHandle<()>,
        client: DuplexStream,
        remote_name: &str,
        args: serde_json::Value,
    ) -> Result<McpCallResult, McpError> {
        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let result = session.call_tool(remote_name, args).await;
        drop(session);
        mock.await.unwrap();
        result
    }

    #[tokio::test]
    async fn tools_call_request_uses_remote_name_and_arguments() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await; // initialize
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await; // initialized
            let call = read_server_line(&mut reader).await;
            assert_eq!(call["method"], "tools/call");
            assert_eq!(call["params"]["name"], "read-file");
            assert_eq!(call["params"]["arguments"], json!({"path": "/tmp/a"}));
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [{ "type": "text", "text": "hello" }], "isError": false }
                }),
            )
            .await;
        });

        let result = run_call_session(mock, client, "read-file", json!({"path": "/tmp/a"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
    }

    #[tokio::test]
    async fn tools_call_success_returns_tool_result_ok() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [{ "type": "text", "text": "hello" }], "isError": false }
                }),
            )
            .await;
        });

        let result = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap();
        let tr = result.into_tool_result();
        assert!(tr.ok);
        assert_eq!(tr.content, "hello");
    }

    #[tokio::test]
    async fn tools_call_tool_error_is_not_rpc_error() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": "invalid input" }],
                        "isError": true
                    }
                }),
            )
            .await;
        });

        let result = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap();
        assert!(result.is_error);
        let tr = result.into_tool_result();
        assert!(!tr.ok);
    }

    #[tokio::test]
    async fn tools_call_jsonrpc_error_is_rpc_error() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32602, "message": "Unknown tool" }
                }),
            )
            .await;
        });

        let err = run_call_session(mock, client, "nope", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::Rpc { code: -32602, .. }));
    }

    #[tokio::test]
    async fn tools_call_handles_notification_before_response() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            // Notification first, then the actual response.
            write_server_line(
                &mut reader,
                &json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
            )
            .await;
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [{ "type": "text", "text": "ok" }], "isError": false }
                }),
            )
            .await;
        });

        let result = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap();
        assert_eq!(result.content[0]["text"], "ok");
    }

    #[tokio::test]
    async fn tools_call_multiple_text_blocks_join_in_order() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            { "type": "text", "text": "A" },
                            { "type": "text", "text": "B" }
                        ],
                        "isError": false
                    }
                }),
            )
            .await;
        });

        let result = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap();
        let tr = result.into_tool_result();
        assert_eq!(tr.content, "A\nB");
    }

    // ── ToolResult bridge tests (no protocol needed) ──

    fn raw_call_result(value: serde_json::Value) -> McpCallResult {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn image_payload_is_not_leaked() {
        let result = raw_call_result(json!({
            "content": [
                { "type": "text", "text": "visible" },
                { "type": "image", "data": "VERY_SECRET_BASE64_PAYLOAD", "mimeType": "image/png" }
            ],
            "isError": false
        }));
        let tr = result.into_tool_result();
        assert!(tr.content.contains("visible"));
        assert!(tr.content.contains("[MCP image content omitted]"));
        assert!(!tr.content.contains("VERY_SECRET_BASE64_PAYLOAD"));
    }

    #[test]
    fn audio_payload_is_not_leaked() {
        let result = raw_call_result(json!({
            "content": [
                { "type": "audio", "data": "VERY_SECRET_BASE64_PAYLOAD", "mimeType": "audio/wav" }
            ],
            "isError": false
        }));
        let tr = result.into_tool_result();
        assert!(tr.content.contains("[MCP audio content omitted]"));
        assert!(!tr.content.contains("VERY_SECRET_BASE64_PAYLOAD"));
    }

    #[test]
    fn structured_content_is_included() {
        let result = raw_call_result(json!({
            "content": [],
            "structuredContent": { "answer": 42 }
        }));
        let tr = result.into_tool_result();
        assert!(tr.content.contains("[MCP structured content]"));
        assert!(tr.content.contains("\"answer\":42"));
    }

    #[test]
    fn empty_success_result_gets_default_text() {
        let result = raw_call_result(json!({ "content": [], "isError": false }));
        let tr = result.into_tool_result();
        assert_eq!(tr.content, "MCP tool returned no content");
        assert!(tr.ok);
    }

    #[test]
    fn empty_error_result_gets_default_text() {
        let result = raw_call_result(json!({ "content": [], "isError": true }));
        let tr = result.into_tool_result();
        assert_eq!(tr.content, "MCP tool failed without error content");
        assert!(!tr.ok);
    }

    #[test]
    fn result_is_truncated_utf8_safe() {
        let long = "长".repeat(MAX_MCP_TOOL_RESULT_CHARS + 5000);
        let result = raw_call_result(json!({
            "content": [{ "type": "text", "text": long }],
            "isError": false
        }));
        let tr = result.into_tool_result();
        assert!(tr.content.chars().count() <= MAX_MCP_TOOL_RESULT_CHARS + 3);
        assert!(tr.content.is_char_boundary(tr.content.len()));
    }

    // ── argument / name validation ──

    #[tokio::test]
    async fn call_tool_rejects_non_object_arguments() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            // Server must never receive a tools/call for bad arguments.
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let notif = read_server_line(&mut reader).await;
            assert_eq!(notif["method"], "notifications/initialized");
            // Then the client returns InvalidToolCall and drops; EOF expected.
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let err = session
            .call_tool("search", json!(["bad"]))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::InvalidToolCall(_)));
        drop(session);
        mock.abort();
    }

    #[tokio::test]
    async fn call_tool_rejects_empty_remote_name() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let mut session = McpSession::new(client);
        session.initialize().await.unwrap();
        session.send_initialized().await.unwrap();
        let err = session.call_tool("", json!({})).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidToolCall(_)));
        drop(session);
        mock.abort();
    }

    #[tokio::test]
    async fn call_tool_missing_content_is_invalid_message() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "isError": false }
                }),
            )
            .await;
        });

        let err = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::InvalidMessage(_)));
    }

    #[tokio::test]
    async fn call_tool_non_object_structured_content_is_invalid() {
        let (client, server) = duplex_pair();
        let mock = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let _ = read_server_line(&mut reader).await;
            write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
            let _ = read_server_line(&mut reader).await;
            let call = read_server_line(&mut reader).await;
            let id = call["id"].as_i64().unwrap();
            write_server_line(
                &mut reader,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [], "structuredContent": [] }
                }),
            )
            .await;
        });

        let err = run_call_session(mock, client, "echo", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::InvalidMessage(_)));
    }

    #[test]
    fn call_stdio_tool_rejects_non_object_arguments() {
        let server = McpServer {
            id: "id".to_string(),
            name: "n".to_string(),
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: None,
            url: None,
            env: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(call_stdio_tool(&server, "search", json!(["bad"])))
            .unwrap_err();
        assert!(matches!(err, McpError::InvalidToolCall(_)));
    }
}
