//! Bounded schema checks for resolved typed arguments. Unknown assertions fail closed.
use super::input_binding::InputBindingError;
use serde_json::Value;

fn mismatch(reason: &str) -> InputBindingError {
    InputBindingError::new("input_binding_schema_mismatch", reason)
}

fn integer_value(value: &Value) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= i128::MIN as f64 && *n < i128::MAX as f64)
                .map(|n| n as i128)
        })
}

fn number_order(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (integer_value(a), integer_value(b)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        _ => a.as_f64()?.partial_cmp(&b.as_f64()?),
    }
}

// JSON numeric equality permits 42 and 42.0, without converting any argument.
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => {
            number_order(a, b) == Some(std::cmp::Ordering::Equal)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn type_matches(value: &Value, kind: &str) -> bool {
    match kind {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => {
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v.fract() == 0.0)
        }
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => false,
    }
}

pub fn validate_arguments(value: &Value, schema: &Value) -> Result<(), InputBindingError> {
    preflight(schema, 0)?;
    validate(value, schema, schema, 0)
}

fn preflight(schema: &Value, depth: usize) -> Result<(), InputBindingError> {
    if depth > 16 {
        return Err(mismatch("schema nesting exceeds limit"));
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let map = schema
        .as_object()
        .ok_or_else(|| mismatch("invalid schema"))?;
    if map.keys().any(|key| !SUPPORTED.contains(&key.as_str())) {
        return Err(mismatch("unsupported schema assertion for typed binding"));
    }
    for name in ["properties", "$defs", "definitions"] {
        if let Some(children) = map.get(name) {
            for child in children
                .as_object()
                .ok_or_else(|| mismatch("invalid schema map"))?
                .values()
            {
                preflight(child, depth + 1)?;
            }
        }
    }
    for name in ["items", "additionalProperties", "not"] {
        if let Some(child) = map.get(name) {
            preflight(child, depth + 1)?;
        }
    }
    for name in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = map.get(name) {
            for child in children
                .as_array()
                .filter(|items| !items.is_empty())
                .ok_or_else(|| mismatch("invalid schema alternatives"))?
            {
                preflight(child, depth + 1)?;
            }
        }
    }
    Ok(())
}

const SUPPORTED: &[&str] = &[
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "$ref",
    "$defs",
    "definitions",
    "$schema",
    "$id",
    "title",
    "description",
    "default",
    "examples",
    "readOnly",
    "writeOnly",
    "deprecated",
];

fn validate(
    value: &Value,
    schema: &Value,
    root: &Value,
    depth: usize,
) -> Result<(), InputBindingError> {
    if depth > 16 {
        return Err(mismatch("schema nesting/reference exceeds limit"));
    }
    if schema == &Value::Bool(true) {
        return Ok(());
    }
    if schema == &Value::Bool(false) {
        return Err(mismatch("schema rejects this value"));
    }
    let s = schema
        .as_object()
        .ok_or_else(|| mismatch("invalid input schema"))?;
    if s.keys().any(|key| !SUPPORTED.contains(&key.as_str())) {
        return Err(mismatch("unsupported schema assertion for typed binding"));
    }
    if let Some(reference) = s.get("$ref") {
        let pointer = reference
            .as_str()
            .and_then(|r| r.strip_prefix('#'))
            .filter(|p| p.is_empty() || p.starts_with('/'))
            .ok_or_else(|| mismatch("only bounded local schema references are supported"))?;
        validate(
            value,
            root.pointer(pointer)
                .ok_or_else(|| mismatch("schema reference is missing"))?,
            root,
            depth + 1,
        )?;
    }
    if let Some(kind) = s.get("type") {
        let kinds: Vec<&str> = if let Some(kind) = kind.as_str() {
            vec![kind]
        } else {
            kind.as_array()
                .ok_or_else(|| mismatch("invalid schema type"))?
                .iter()
                .map(|k| k.as_str().ok_or_else(|| mismatch("invalid schema type")))
                .collect::<Result<_, _>>()?
        };
        if kinds.is_empty()
            || kinds.iter().any(|k| {
                !matches!(
                    *k,
                    "string" | "number" | "integer" | "boolean" | "object" | "array" | "null"
                )
            })
        {
            return Err(mismatch("unsupported schema type"));
        }
        if !kinds.iter().any(|kind| type_matches(value, kind)) {
            return Err(InputBindingError::new(
                "input_binding_type_mismatch",
                "resolved value does not match schema type",
            ));
        }
    }
    if let Some(values) = s.get("enum") {
        if !values
            .as_array()
            .ok_or_else(|| mismatch("invalid enum"))?
            .iter()
            .any(|expected| equal(value, expected))
        {
            return Err(mismatch("resolved value is outside enum"));
        }
    }
    if let Some(expected) = s.get("const") {
        if !equal(value, expected) {
            return Err(mismatch("resolved value differs from const"));
        }
    }
    for (name, min, max) in [
        ("allOf", usize::MAX, usize::MAX),
        ("anyOf", 1, usize::MAX),
        ("oneOf", 1, 1),
    ] {
        if let Some(options) = s.get(name) {
            let options = options
                .as_array()
                .filter(|a| !a.is_empty())
                .ok_or_else(|| mismatch("invalid schema alternatives"))?;
            let passed = options
                .iter()
                .filter(|candidate| validate(value, candidate, root, depth + 1).is_ok())
                .count();
            let accepted = if name == "allOf" {
                passed == options.len()
            } else {
                passed >= min && passed <= max
            };
            if !accepted {
                return Err(mismatch("schema alternatives do not match"));
            }
        }
    }
    if let Some(negated) = s.get("not") {
        if validate(value, negated, root, depth + 1).is_ok() {
            return Err(mismatch("not schema matches"));
        }
    }
    if let Some(map) = value.as_object() {
        if let Some(required) = s.get("required") {
            for key in required
                .as_array()
                .ok_or_else(|| mismatch("invalid required list"))?
            {
                let key = key
                    .as_str()
                    .ok_or_else(|| mismatch("invalid required key"))?;
                if !map.contains_key(key) {
                    return Err(mismatch("required argument is missing"));
                }
            }
        }
        let properties = s
            .get("properties")
            .map(|p| p.as_object().ok_or_else(|| mismatch("invalid properties")))
            .transpose()?;
        for (key, item) in map {
            if let Some(schema) = properties.and_then(|p| p.get(key)) {
                validate(item, schema, root, depth + 1)?;
            } else if let Some(extra) = s.get("additionalProperties") {
                validate(item, extra, root, depth + 1)?;
            }
        }
        bounds(map.len(), s.get("minProperties"), s.get("maxProperties"))?;
    }
    if let Some(items) = value.as_array() {
        bounds(items.len(), s.get("minItems"), s.get("maxItems"))?;
        if let Some(schema) = s.get("items") {
            for item in items {
                validate(item, schema, root, depth + 1)?;
            }
        }
        if s.get("uniqueItems").and_then(Value::as_bool) == Some(true)
            && items
                .iter()
                .enumerate()
                .any(|(i, item)| items[..i].iter().any(|other| equal(item, other)))
        {
            return Err(mismatch("array items must be unique"));
        }
    }
    if let Some(text) = value.as_str() {
        bounds(text.chars().count(), s.get("minLength"), s.get("maxLength"))?;
    }
    if let Some(number) = value.as_f64() {
        for (key, valid) in [
            ("minimum", 0),
            ("maximum", 1),
            ("exclusiveMinimum", 2),
            ("exclusiveMaximum", 3),
        ] {
            if let Some(bound) = s.get(key) {
                let order =
                    number_order(value, bound).ok_or_else(|| mismatch("invalid number bound"))?;
                if !match valid {
                    0 => !order.is_lt(),
                    1 => !order.is_gt(),
                    2 => order.is_gt(),
                    _ => order.is_lt(),
                } {
                    return Err(mismatch("number exceeds schema bound"));
                }
            }
        }
        if let Some(multiple) = s.get("multipleOf") {
            let multiple = multiple
                .as_f64()
                .filter(|n| *n > 0.0)
                .ok_or_else(|| mismatch("invalid multipleOf"))?;
            if ((number / multiple) - (number / multiple).round()).abs() > f64::EPSILON * 16.0 {
                return Err(mismatch("number is not a schema multiple"));
            }
        }
    }
    Ok(())
}

fn bounds(size: usize, min: Option<&Value>, max: Option<&Value>) -> Result<(), InputBindingError> {
    if min
        .map(|v| v.as_u64().ok_or_else(|| mismatch("invalid length bound")))
        .transpose()?
        .is_some_and(|n| (size as u64) < n)
        || max
            .map(|v| v.as_u64().ok_or_else(|| mismatch("invalid length bound")))
            .transpose()?
            .is_some_and(|n| (size as u64) > n)
    {
        return Err(mismatch("value length exceeds schema bound"));
    }
    Ok(())
}
