// ============================================================
// MCP prompts wire helpers — parse list/get results.
// ============================================================

use super::model::{
    McpInputRequired, McpOperationOutcome, McpPromptDescriptor, McpPromptResult, McpRuntimeError,
};

pub fn parse_prompt_list(
    result: &serde_json::Value,
) -> Result<(Vec<McpPromptDescriptor>, Option<String>), McpRuntimeError> {
    let items = result
        .get("prompts")
        .and_then(|p| p.as_array())
        .ok_or(McpRuntimeError::InvalidResponse)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Ok(d) = serde_json::from_value::<McpPromptDescriptor>(item.clone()) {
            out.push(d);
        }
    }
    let next = result
        .get("nextCursor")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    Ok((out, next))
}

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
