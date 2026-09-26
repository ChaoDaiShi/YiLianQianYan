// ============================================================
// Legacy MCP stdio client — JSON-RPC session over an AsyncRead + AsyncWrite
// pair (a stdio child, or an in-memory duplex for tests).
// ============================================================

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

use super::error::{McpError, MAX_MESSAGES_PER_RESPONSE, MAX_TOOL_LIST_PAGES, MCP_PROTOCOL_VERSION};
use super::types::{McpCallResult, McpInitializeInfo, McpTool};

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
