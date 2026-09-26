// ============================================================
// Legacy MCP stdio client — wire result types.
//
// `McpCallResult::into_tool_result` is the bridge into the project's
// `ToolResult`: text blocks are joined in order and every non-text block is
// replaced by a placeholder, so base64 image/audio payloads are never leaked
// into an agent's context.
// ============================================================

use serde::{Deserialize, Serialize};

use crate::tools::trait_def::ToolResult;

use super::error::MAX_MCP_TOOL_RESULT_CHARS;

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
