// ============================================================
// MCP tools catalogue — parse `tools/list` pages.
// ============================================================

use super::super::protocol::model::{McpRuntimeError, McpTool};

/// Parse a `tools/list` result into (tools, nextCursor).
pub fn parse_tool_list(
    result: &serde_json::Value,
) -> Result<(Vec<McpTool>, Option<String>), McpRuntimeError> {
    let tools = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or(McpRuntimeError::InvalidResponse)?;
    let mut parsed = Vec::with_capacity(tools.len());
    for tool in tools {
        match serde_json::from_value::<McpTool>(tool.clone()) {
            Ok(t) => parsed.push(t),
            Err(_) => {
                // A single malformed tool is skipped; the server stays usable.
                tracing::warn!("skipping malformed MCP tool in tools/list");
            }
        }
    }
    let next_cursor = result
        .get("nextCursor")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    Ok((parsed, next_cursor))
}
