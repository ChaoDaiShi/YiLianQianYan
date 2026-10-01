//! Deterministic resolver. Reads only the supplied execution dependency snapshot.
use super::execution::NodeContext;
use super::input_binding::{validate_set, InputBinding, InputBindingError};
use serde_json::Value;

pub struct TypedInputBindingResolver;
impl TypedInputBindingResolver {
    pub fn resolve(
        literal: &Value,
        bindings: &[InputBinding],
        context: &NodeContext,
        schema: Option<&Value>,
    ) -> Result<Value, InputBindingError> {
        validate_set(bindings)?;
        if !literal.is_object() {
            return Err(InputBindingError::new(
                "input_binding_type_mismatch",
                "literal arguments must be an object",
            ));
        }
        let mut result = literal.clone();
        for binding in bindings {
            let mut value = context
                .dependency_outputs
                .get(binding.source.node_id().as_str())
                .ok_or_else(|| {
                    InputBindingError::new(
                        "input_binding_source_missing",
                        "dependency snapshot has no source output",
                    )
                })?;
            for key in binding.source.path() {
                value = value
                    .as_object()
                    .and_then(|object| object.get(key))
                    .ok_or_else(|| {
                        InputBindingError::new(
                            "input_binding_path_missing",
                            "object property does not exist in dependency snapshot",
                        )
                    })?;
            }
            write_path(&mut result, &binding.target_path, value.clone())?;
        }
        fn depth(value: &Value, n: usize) -> bool {
            n <= 16
                && match value {
                    Value::Object(map) => map.values().all(|v| depth(v, n + 1)),
                    Value::Array(values) => values.iter().all(|v| depth(v, n + 1)),
                    _ => true,
                }
        }
        if result.to_string().len() > super::application::capability_binding::MAX_ARGUMENT_BYTES
            || !depth(&result, 0)
        {
            return Err(InputBindingError::new(
                "input_binding_exceeds_limit",
                "resolved arguments exceed bounded limits",
            ));
        }
        if !bindings.is_empty() {
            let schema = schema.ok_or_else(|| {
                InputBindingError::new(
                    "input_binding_schema_mismatch",
                    "downstream schema is missing",
                )
            })?;
            if super::application::capability_binding::has_header_binding(schema) {
                return Err(InputBindingError::new(
                    "header_unsupported",
                    "Canvas header bindings remain unsupported",
                ));
            }
            super::binding_schema::validate_arguments(&result, schema)?;
        }
        Ok(result)
    }
}

fn write_path(target: &mut Value, path: &[String], value: Value) -> Result<(), InputBindingError> {
    let object = target.as_object_mut().ok_or_else(|| {
        InputBindingError::new(
            "input_binding_type_mismatch",
            "target parent must be an object",
        )
    })?;
    if path.len() == 1 {
        object.insert(path[0].clone(), value);
        return Ok(());
    }
    let child = object
        .entry(path[0].clone())
        .or_insert_with(|| serde_json::json!({}));
    write_path(child, &path[1..], value)
}
