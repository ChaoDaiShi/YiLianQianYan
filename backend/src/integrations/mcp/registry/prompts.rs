// ============================================================
// MCP prompts catalogue — parse `prompts/list` pages.
// ============================================================

use super::super::protocol::model::{McpPromptDescriptor, McpRuntimeError};

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
