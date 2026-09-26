// ============================================================
// MCP `prompts/get` results — MRTR-aware outcome.
// ============================================================

use super::super::protocol::model::{McpInputRequired, McpOperationOutcome, McpPromptResult};

/// Parse a `prompts/get` result into an MRTR-aware outcome.
pub fn parse_prompt_get(result: &serde_json::Value) -> McpOperationOutcome<McpPromptResult> {
    match result.get("resultType").and_then(|t| t.as_str()) {
        Some("input_required") => McpOperationOutcome::InputRequired(McpInputRequired {
            prompt: result
                .get("prompt")
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string(),
            input_schema: result.get("inputSchema").cloned(),
        }),
        _ => {
            let prompt_result: McpPromptResult =
                serde_json::from_value(result.clone()).unwrap_or(McpPromptResult {
                    description: None,
                    messages: Vec::new(),
                });
            McpOperationOutcome::Complete(prompt_result)
        }
    }
}
