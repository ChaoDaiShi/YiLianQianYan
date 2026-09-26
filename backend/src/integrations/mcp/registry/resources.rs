// ============================================================
// MCP resources catalogue — parse `resources/list` and
// `resources/templates/list` pages.
// ============================================================

use super::super::protocol::model::{McpResourceDescriptor, McpResourceTemplate, McpRuntimeError};

pub fn parse_resource_list(
    result: &serde_json::Value,
) -> Result<(Vec<McpResourceDescriptor>, Option<String>), McpRuntimeError> {
    let items = result
        .get("resources")
        .and_then(|r| r.as_array())
        .ok_or(McpRuntimeError::InvalidResponse)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Ok(d) = serde_json::from_value::<McpResourceDescriptor>(item.clone()) {
            out.push(d);
        }
    }
    let next = result
        .get("nextCursor")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    Ok((out, next))
}

pub fn parse_resource_template_list(
    result: &serde_json::Value,
) -> Result<(Vec<McpResourceTemplate>, Option<String>), McpRuntimeError> {
    let items = result
        .get("resourceTemplates")
        .and_then(|r| r.as_array())
        .ok_or(McpRuntimeError::InvalidResponse)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Ok(t) = serde_json::from_value::<McpResourceTemplate>(item.clone()) {
            out.push(t);
        }
    }
    let next = result
        .get("nextCursor")
        .and_then(|c| c.as_str())
        .map(str::to_string);
    Ok((out, next))
}
