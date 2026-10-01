//! Typed, bounded metadata for direct dependency output bindings. No execution.
use super::task_graph::{TaskGraph, TaskNodeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_BINDINGS: usize = 64;
pub const MAX_PATH_SEGMENTS: usize = 16;
pub const MAX_PATH_SEGMENT_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputBinding {
    pub target_path: Vec<String>,
    pub source: InputBindingSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputBindingSource {
    DependencyOutput {
        node_id: TaskNodeId,
        path: Vec<String>,
    },
}

impl InputBindingSource {
    pub fn node_id(&self) -> &TaskNodeId {
        match self {
            Self::DependencyOutput { node_id, .. } => node_id,
        }
    }
    pub fn path(&self) -> &[String] {
        match self {
            Self::DependencyOutput { path, .. } => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {reason}")]
pub struct InputBindingError {
    pub code: &'static str,
    pub reason: String,
}
impl InputBindingError {
    pub fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}

#[derive(Deserialize)]
struct BindingEnvelope {
    #[serde(default)]
    capability_bindings: Vec<InputBinding>,
    executor_ref: Option<Value>,
}

pub fn validate_path(path: &[String]) -> Result<(), InputBindingError> {
    if path.is_empty()
        || path.len() > MAX_PATH_SEGMENTS
        || path.iter().any(|s| {
            s.trim().is_empty()
                || s.len() > MAX_PATH_SEGMENT_BYTES
                || s.chars().any(char::is_control)
                || s.contains('*')
        })
    {
        return Err(InputBindingError::new(
            "invalid_binding_path",
            "object property path exceeds limits or contains an invalid segment",
        ));
    }
    Ok(())
}

pub fn validate_set(bindings: &[InputBinding]) -> Result<(), InputBindingError> {
    if bindings.len() > MAX_BINDINGS {
        return Err(InputBindingError::new(
            "input_binding_exceeds_limit",
            "too many bindings",
        ));
    }
    for (i, binding) in bindings.iter().enumerate() {
        validate_path(&binding.target_path)?;
        validate_path(binding.source.path())?;
        TaskNodeId::new(binding.source.node_id().as_str()).map_err(|_| {
            InputBindingError::new("invalid_binding_source", "invalid source node identity")
        })?;
        for other in &bindings[..i] {
            if binding.target_path == other.target_path {
                return Err(InputBindingError::new(
                    "duplicate_binding_target",
                    "two bindings address the same target",
                ));
            }
            if binding.target_path.starts_with(&other.target_path)
                || other.target_path.starts_with(&binding.target_path)
            {
                return Err(InputBindingError::new(
                    "binding_target_collision",
                    "parent and child binding targets overlap",
                ));
            }
        }
    }
    Ok(())
}

pub fn bindings_from_input(input: &Value) -> Result<Vec<InputBinding>, InputBindingError> {
    // Non-object legacy inputs cannot carry this field. Preserve their prior validation.
    let Some(fields) = input.as_object() else {
        return Ok(Vec::new());
    };
    if !fields.contains_key("capability_bindings") {
        return Ok(Vec::new());
    }
    let envelope: BindingEnvelope = serde_json::from_value(input.clone()).map_err(|_| {
        InputBindingError::new(
            "invalid_input_binding",
            "binding metadata must use the typed dependency-output shape",
        )
    })?;
    validate_set(&envelope.capability_bindings)?;
    if !envelope.capability_bindings.is_empty()
        && !envelope
            .executor_ref
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|r| r.starts_with("capability://"))
    {
        return Err(InputBindingError::new(
            "invalid_input_binding",
            "typed bindings require a capability executor",
        ));
    }
    Ok(envelope.capability_bindings)
}

pub fn validate_membership(
    node_id: &TaskNodeId,
    dependencies: &[TaskNodeId],
    bindings: &[InputBinding],
) -> Result<(), InputBindingError> {
    for binding in bindings {
        let source = binding.source.node_id();
        if source == node_id || !dependencies.contains(source) {
            return Err(InputBindingError::new(
                "input_binding_source_not_dependency",
                format!(
                    "节点 {} 的输入仍依赖 {}，请先移除绑定；来源必须是直接依赖。",
                    node_id, source
                ),
            ));
        }
    }
    Ok(())
}

pub fn validate_graph_bindings(graph: &TaskGraph) -> Result<(), InputBindingError> {
    for node in &graph.nodes {
        validate_membership(
            &node.id,
            &graph.dependencies(&node.id),
            &bindings_from_input(&node.input)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "input_binding_tests.rs"]
mod tests;
