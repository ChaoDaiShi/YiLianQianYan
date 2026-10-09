// ============================================================
// MCP `tools/call` results — MRTR-aware outcome plus a bounded text bridge.
// ============================================================

use super::super::protocol::model::{McpInputRequired, McpOperationOutcome};

pub const MAX_MCP_STRUCTURED_BYTES: usize = 64 * 1024;
pub const MAX_MCP_STRUCTURED_DEPTH: usize = 16;

/// Preserve only the explicit protocol field. Text is never parsed as JSON.
pub fn call_result_structured(
    result: &serde_json::Value,
) -> Result<Option<serde_json::Value>, super::super::protocol::model::McpRuntimeError> {
    let Some(value) = result.get("structuredContent") else {
        return Ok(None);
    };
    if !value.is_object() {
        return Err(super::super::protocol::model::McpRuntimeError::Protocol(
            "structuredContent must be an object".into(),
        ));
    }
    fn bounded(value: &serde_json::Value, depth: usize) -> bool {
        depth <= MAX_MCP_STRUCTURED_DEPTH
            && match value {
                serde_json::Value::Object(map) => map.values().all(|v| bounded(v, depth + 1)),
                serde_json::Value::Array(values) => values.iter().all(|v| bounded(v, depth + 1)),
                _ => true,
            }
    }
    if !bounded(value, 0) || value.to_string().len() > MAX_MCP_STRUCTURED_BYTES {
        return Err(super::super::protocol::model::McpRuntimeError::ResponseTooLarge);
    }
    Ok(Some(value.clone()))
}

#[cfg(test)]
mod structured_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn u31_structured_protocol_field_only_and_bounds() {
        assert_eq!(
            call_result_structured(&json!({"content":[{"type":"text","text":"{\"answer\":42}"}]}))
                .unwrap(),
            None
        );
        assert_eq!(
            call_result_structured(&json!({"structuredContent":{"answer":42}})).unwrap(),
            Some(json!({"answer":42}))
        );
        assert!(call_result_structured(&json!({"structuredContent":null})).is_err());
        assert!(call_result_structured(
            &json!({"structuredContent":{"value":"x".repeat(MAX_MCP_STRUCTURED_BYTES)}})
        )
        .is_err());
        let mut value = json!({});
        for _ in 0..=MAX_MCP_STRUCTURED_DEPTH {
            value = json!({"nested":value});
        }
        assert!(call_result_structured(&json!({"structuredContent":value})).is_err());
    }
    #[test]
    fn u31_tool_result_legacy_serde_and_constructors() {
        use crate::tools::ToolResult;
        let old: ToolResult = serde_json::from_value(json!({"content":"old","ok":true})).unwrap();
        assert!(old.structured.is_none());
        assert!(serde_json::to_value(ToolResult::success("old"))
            .unwrap()
            .get("structured")
            .is_none());
        assert!(ToolResult::error("error").structured.is_none());
    }
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
    crate::modules::workflow::safe_tool_result_summary(text.trim())
}
