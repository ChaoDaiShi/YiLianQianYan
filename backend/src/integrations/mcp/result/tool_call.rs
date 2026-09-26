// ============================================================
// MCP `tools/call` results — MRTR-aware outcome plus a bounded text bridge.
// ============================================================

use super::super::protocol::model::{McpInputRequired, McpOperationOutcome};

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
    crate::modules::workflow::safe_tool_result_summary(text.trim())
}
