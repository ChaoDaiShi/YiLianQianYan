//! Fixed-input and binding schema checks.
use super::*;

pub(super) fn literal_contains_path(
    literal: &serde_json::Map<String, Value>,
    path: &[String],
) -> bool {
    let Some(first) = path.first() else {
        return false;
    };
    let mut value = literal.get(first);
    for segment in path.iter().skip(1) {
        value = value
            .and_then(|parent| parent.as_object())
            .and_then(|object| object.get(segment));
    }
    value.is_some()
}

pub(super) fn resolve_output_property_type(
    schema: Option<&Value>,
    path: &[String],
) -> Option<String> {
    let mut current = schema?;
    for segment in path {
        let properties = current.get("properties")?.as_object()?;
        current = properties.get(segment)?;
    }
    extract_schema_type(current)
}

pub(super) fn resolve_input_property_type(
    schema: Option<&Value>,
    path: &[String],
) -> Option<String> {
    let mut current = schema?;
    for segment in path {
        let properties = current.get("properties")?.as_object()?;
        current = properties.get(segment)?;
    }
    extract_schema_type(current)
}

fn extract_schema_type(schema: &Value) -> Option<String> {
    if let Some(t) = schema.get("type").and_then(Value::as_str) {
        return Some(t.to_string());
    }
    None
}

pub(super) fn types_compatible(source: &str, target: &str) -> bool {
    if source == target {
        return true;
    }
    // integer can flow into number (integer is a subtype of number in JSON)
    if source == "integer" && target == "number" {
        return true;
    }
    false
}

pub(super) fn remove_required_checks(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        object.remove("required");
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            for property in properties.values_mut() {
                remove_required_checks(property);
            }
        }
    }
}

pub(super) fn collect_missing_inputs(
    schema: &Value,
    literal: &Value,
    bindings: &[crate::modules::task::input_binding::InputBinding],
    prefix: &[String],
    node_id: &str,
    pending: &mut Vec<PendingInput>,
) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    let required = schema.get("required").and_then(Value::as_array);
    for (key, property) in properties {
        let mut path = prefix.to_vec();
        path.push(key.clone());
        if bindings
            .iter()
            .any(|binding| path.starts_with(&binding.target_path))
        {
            continue;
        }
        let value = literal.get(key);
        let bound_descendant = bindings
            .iter()
            .any(|binding| binding.target_path.starts_with(&path));
        if value.is_none()
            && !bound_descendant
            && required.is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some(key)))
        {
            if !pending
                .iter()
                .any(|item| item.node_id == node_id && item.target_path == path)
            {
                pending.push(PendingInput {
                    node_id: node_id.into(),
                    target_path: path,
                    reason: PendingInputReason::MissingLiteral,
                });
            }
        } else if value.is_some() || bound_descendant {
            collect_missing_inputs(
                property,
                value.unwrap_or(&Value::Null),
                bindings,
                &path,
                node_id,
                pending,
            );
        }
    }
}
