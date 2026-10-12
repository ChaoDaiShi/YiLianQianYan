//! Schema path resolution and type compatibility for ProofLoop.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §3

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contracts::{FieldPath, ProofError};
use crate::modules::task::input_binding::{MAX_PATH_SEGMENTS, MAX_PATH_SEGMENT_BYTES};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedProperty {
    pub path: FieldPath,
    pub property_type: String,
    pub is_required: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredProperty {
    pub path: FieldPath,
    pub property_type: String,
    pub is_required: bool,
}

/// Resolves a field path against a JSON Schema, returning property type and whether it is required.
pub fn resolve_property(schema: &Value, path: &[String]) -> Result<ResolvedProperty, ProofError> {
    if path.is_empty() {
        return Err(ProofError::new("empty_path", "属性路径不能为空"));
    }
    if path.len() > MAX_PATH_SEGMENTS {
        return Err(ProofError::new(
            "path_too_deep",
            format!("属性路径深度超过最大上限 {}", MAX_PATH_SEGMENTS),
        ));
    }
    for seg in path {
        if seg.len() > MAX_PATH_SEGMENT_BYTES {
            return Err(ProofError::new(
                "segment_too_long",
                format!("路径片段超过字节上限 {}", MAX_PATH_SEGMENT_BYTES),
            ));
        }
    }

    let mut current = schema;
    let mut is_required = true;

    for (i, segment) in path.iter().enumerate() {
        let properties = match current.get("properties").and_then(Value::as_object) {
            Some(props) => props,
            None => {
                return Err(ProofError::new(
                    "proofloop_unsupported_schema",
                    format!("路径 {:?} 在第 {} 层缺少 properties 对象", path, i),
                ));
            }
        };

        let required_list = current
            .get("required")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .any(|req| req == segment)
            })
            .unwrap_or(false);

        if !required_list {
            is_required = false;
        }

        current = match properties.get(segment) {
            Some(val) => val,
            None => {
                return Err(ProofError::new(
                    "proofloop_unknown_output",
                    format!(
                        "未在 schema 中声明字段 '{}' (完整路径: {:?})",
                        segment, path
                    ),
                ));
            }
        };
    }

    let property_type = current
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let description = current
        .get("description")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    Ok(ResolvedProperty {
        path: path.to_vec(),
        property_type,
        is_required,
        description,
    })
}

/// Checks type compatibility according to §3:
/// - Exact matches match
/// - integer can flow into number
/// - No implicit conversions (string -> number, etc.)
pub fn are_types_compatible(source_type: &str, target_type: &str) -> bool {
    if source_type == target_type {
        return true;
    }
    if source_type == "integer" && target_type == "number" {
        return true;
    }
    false
}

/// Recursively extracts all declared properties from a schema up to MAX_PATH_SEGMENTS.
pub fn extract_declared_properties(schema: &Value, prefix: &[String]) -> Vec<DeclaredProperty> {
    let mut out = Vec::new();
    extract_inner(schema, prefix, &mut out, 0);
    out
}

fn extract_inner(
    schema: &Value,
    current_path: &[String],
    out: &mut Vec<DeclaredProperty>,
    depth: usize,
) {
    if depth >= MAX_PATH_SEGMENTS {
        return;
    }
    let properties = match schema.get("properties").and_then(Value::as_object) {
        Some(props) => props,
        None => return,
    };
    let required_set: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    for (name, prop_val) in properties {
        let mut path = current_path.to_vec();
        path.push(name.clone());

        let prop_type = prop_val
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("object");

        out.push(DeclaredProperty {
            path: path.clone(),
            property_type: prop_type.to_string(),
            is_required: required_set.contains(name.as_str()),
        });

        if prop_type == "object" && prop_val.get("properties").is_some() {
            extract_inner(prop_val, &path, out, depth + 1);
        }
    }
}
