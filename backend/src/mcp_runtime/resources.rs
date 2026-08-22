// ============================================================
// MCP resources wire helpers — parse list/templates/read results.
// ============================================================

use super::model::{
    McpResourceContent, McpResourceDescriptor, McpResourceTemplate, McpRuntimeError,
    MAX_RESOURCE_BLOB_BASE64_CHARS, MAX_RESOURCE_CONTENT_ITEMS, MAX_RESOURCE_TEXT_CHARS,
};

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

/// Parse `resources/read` contents into bounded typed content.
pub fn parse_resource_contents(
    result: &serde_json::Value,
) -> Result<Vec<McpResourceContent>, McpRuntimeError> {
    let items = result
        .get("contents")
        .and_then(|c| c.as_array())
        .ok_or(McpRuntimeError::InvalidResponse)?;
    if items.len() > MAX_RESOURCE_CONTENT_ITEMS {
        return Err(McpRuntimeError::ResponseTooLarge);
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .unwrap_or_default()
            .to_string();
        let mime_type = item
            .get("mimeType")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
            if text.chars().count() > MAX_RESOURCE_TEXT_CHARS {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            out.push(McpResourceContent::Text {
                uri,
                mime_type,
                text: text.to_string(),
            });
        } else if let Some(blob) = item.get("blob").and_then(|b| b.as_str()) {
            if blob.len() > MAX_RESOURCE_BLOB_BASE64_CHARS {
                return Err(McpRuntimeError::ResponseTooLarge);
            }
            out.push(McpResourceContent::Blob {
                uri,
                mime_type,
                blob_base64: blob.to_string(),
            });
        }
    }
    Ok(out)
}
