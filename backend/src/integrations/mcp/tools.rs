// ============================================================
// MCP tools wire helpers — parse tools/list + tools/call results.
// ============================================================

use super::model::{McpInputRequired, McpOperationOutcome, McpRuntimeError, McpTool};

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

/// Parse a `tools/call` result into an MRTR-aware outcome.
pub fn parse_call_result(result: &serde_json::Value) -> McpOperationOutcome<serde_json::Value> {
    match result.get("resultType").and_then(|t| t.as_str()) {
        Some("input_required") => McpOperationOutcome::InputRequired(McpInputRequired {
            prompt: result
                .get("prompt")
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string(),
            input_schema: result.get("inputSchema").cloned(),
        }),
        _ => McpOperationOutcome::Complete(result.clone()),
    }
}

/// Extract a bounded text summary from a `tools/call` result.
pub fn call_result_text(result: &serde_json::Value) -> String {
    let content = result.get("content").and_then(|c| c.as_array());
    let mut text = String::new();
    if let Some(items) = content {
        for item in items {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
                text.push('\n');
            }
        }
    }
    crate::workflow::safe_tool_result_summary(text.trim())
}
