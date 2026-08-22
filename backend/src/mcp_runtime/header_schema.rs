// ============================================================
// x-mcp-header schema scanning.
//
// Extracts `x-mcp-header` annotations from a tool inputSchema. Traversal is
// strictly limited to the `properties` chain: it never descends through
// `items`, `oneOf`, `anyOf`, `allOf`, `not`, `if`/`then`/`else`, or `$ref`.
// ============================================================

use std::collections::{BTreeMap, HashSet};

use super::model::{McpHeaderBinding, McpHeaderValueType, MAX_MCP_SCHEMA_DEPTH};
use super::protocol::is_valid_header_token;

const X_MCP_HEADER: &str = "x-mcp-header";

/// Scan an input schema for `x-mcp-header` bindings. Invalid tools return an
/// error (the caller excludes just that tool).
pub fn scan_tool_header_bindings(
    input_schema: &serde_json::Value,
) -> Result<Vec<McpHeaderBinding>, String> {
    let mut bindings = Vec::new();
    let mut seen_headers: HashSet<String> = HashSet::new();

    let Some(properties) = input_schema.get("properties") else {
        return Ok(bindings);
    };
    let Some(properties) = properties.as_object() else {
        return Ok(bindings);
    };

    for (name, value) in properties {
        walk_property(
            &mut bindings,
            &mut seen_headers,
            &[name.to_string()],
            value,
            1,
        )?;
    }
    Ok(bindings)
}

fn walk_property(
    bindings: &mut Vec<McpHeaderBinding>,
    seen_headers: &mut HashSet<String>,
    path: &[String],
    value: &serde_json::Value,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_MCP_SCHEMA_DEPTH {
        return Err("inputSchema nesting exceeds the depth limit".to_string());
    }
    // A property node may carry an x-mcp-header annotation directly.
    if let Some(header) = value.get(X_MCP_HEADER) {
        let header_name = header.as_str().ok_or("x-mcp-header must be a string")?;
        register_binding(bindings, seen_headers, path, header_name, value)?;
        return Ok(());
    }
    // Otherwise, descend only through the properties chain.
    if let Some(props) = value.get("properties") {
        if let Some(props) = props.as_object() {
            for (name, child) in props {
                let mut next = path.to_vec();
                next.push(name.to_string());
                walk_property(bindings, seen_headers, &next, child, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn register_binding(
    bindings: &mut Vec<McpHeaderBinding>,
    seen_headers: &mut HashSet<String>,
    path: &[String],
    header_name: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    if header_name.is_empty() || !is_valid_header_token(header_name) {
        return Err("x-mcp-header value is not a valid HTTP field-name token".to_string());
    }
    // Case-insensitive dedup.
    let normalized = header_name.to_ascii_lowercase();
    if !seen_headers.insert(normalized) {
        return Err("duplicate x-mcp-header annotation".to_string());
    }
    let value_type = match value.get("type").and_then(|t| t.as_str()) {
        Some("string") => McpHeaderValueType::String,
        Some("integer") => McpHeaderValueType::Integer,
        Some("boolean") => McpHeaderValueType::Boolean,
        _ => return Err("x-mcp-header argument must be string, integer, or boolean".to_string()),
    };
    bindings.push(McpHeaderBinding {
        argument_path: path.to_vec(),
        header_name: header_name.to_string(),
        value_type,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(schema: serde_json::Value) -> Result<Vec<McpHeaderBinding>, String> {
        scan_tool_header_bindings(&schema)
    }

    #[test]
    fn simple_header_annotation() {
        let bindings = scan(serde_json::json!({
            "type": "object",
            "properties": {
                "token": { "type": "string", "x-mcp-header": "X-Token" }
            }
        }))
        .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].argument_path, vec!["token"]);
        assert_eq!(bindings[0].header_name, "X-Token");
    }

    #[test]
    fn nested_header_annotation() {
        let bindings = scan(serde_json::json!({
            "type": "object",
            "properties": {
                "auth": {
                    "type": "object",
                    "properties": {
                        "token": { "type": "string", "x-mcp-header": "X-Token" }
                    }
                }
            }
        }))
        .unwrap();
        assert_eq!(bindings[0].argument_path, vec!["auth", "token"]);
    }

    #[test]
    fn items_path_rejected() {
        // An annotation nested under `items` must NOT be discovered.
        let bindings = scan(serde_json::json!({
            "type": "object",
            "properties": {
                "tokens": {
                    "type": "array",
                    "items": { "type": "string", "x-mcp-header": "X-Token" }
                }
            }
        }))
        .unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn ref_path_rejected() {
        let bindings = scan(serde_json::json!({
            "type": "object",
            "properties": {
                "token": { "$ref": "#/definitions/tok" }
            },
            "definitions": {
                "tok": { "type": "string", "x-mcp-header": "X-Token" }
            }
        }))
        .unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn non_primitive_type_rejected() {
        assert!(scan(serde_json::json!({
            "type": "object",
            "properties": {
                "n": { "type": "number", "x-mcp-header": "X-N" }
            }
        }))
        .is_err());
        assert!(scan(serde_json::json!({
            "type": "object",
            "properties": {
                "o": { "type": "object", "x-mcp-header": "X-O" }
            }
        }))
        .is_err());
    }

    #[test]
    fn duplicate_case_insensitive_header_rejected() {
        assert!(scan(serde_json::json!({
            "type": "object",
            "properties": {
                "a": { "type": "string", "x-mcp-header": "X-Token" },
                "b": { "type": "string", "x-mcp-header": "x-token" }
            }
        }))
        .is_err());
    }

    #[test]
    fn extract_header_values_handles_primitives_and_nesting() {
        let bindings = scan(serde_json::json!({
            "type": "object",
            "properties": {
                "tenant": { "type": "string", "x-mcp-header": "X-Tenant" },
                "auth": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "x-mcp-header": "X-Count" }
                    }
                }
            }
        }))
        .unwrap();

        let values = extract_header_values(
            &serde_json::json!({ "tenant": "acme", "auth": { "count": 7 } }),
            &bindings,
        )
        .unwrap();
        assert_eq!(values.get("X-Tenant").unwrap(), "acme");
        assert_eq!(values.get("X-Count").unwrap(), "7");

        // Wrong type → error.
        assert!(extract_header_values(&serde_json::json!({ "tenant": 42 }), &bindings).is_err());
    }
}

/// Extract `x-mcp-header` argument values (string/integer/boolean) from call
/// arguments, keyed by header name. Missing values are skipped; an argument of
/// the wrong primitive type is an error.
pub fn extract_header_values(
    arguments: &serde_json::Value,
    bindings: &[McpHeaderBinding],
) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for binding in bindings {
        let mut current = arguments;
        let mut found = true;
        for key in &binding.argument_path {
            match current.get(key) {
                Some(v) => current = v,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if !found {
            continue;
        }
        let value_str = match binding.value_type {
            McpHeaderValueType::String => current.as_str().map(str::to_string),
            McpHeaderValueType::Integer => current.as_i64().map(|i| i.to_string()),
            McpHeaderValueType::Boolean => current.as_bool().map(|b| b.to_string()),
        };
        let Some(value_str) = value_str else {
            return Err(format!(
                "header argument {} has an invalid type",
                binding.argument_path.join(".")
            ));
        };
        out.insert(binding.header_name.clone(), value_str);
    }
    Ok(out)
}
