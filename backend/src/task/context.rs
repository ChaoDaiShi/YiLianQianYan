//! Bounded TaskGraph-to-node execution context projection.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

use super::execution::{
    NodeContext, NodeExecutionError, MAX_NODE_CONTEXT_ITEMS, MAX_NODE_CONTEXT_ITEM_CHARS,
};
use super::{TaskGraph, TaskNode, TaskNodeId};

const FORBIDDEN_CONTEXT_FIELDS: &[(&str, &str)] = &[
    ("conversation", "conversation"),
    ("messages", "messages"),
    ("history", "history"),
    ("graph", "graph"),
    ("task_graph", "task_graph"),
    ("whole_graph", "whole_graph"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeContextLimits {
    pub max_items: usize,
    pub max_item_chars: usize,
}

impl Default for NodeContextLimits {
    fn default() -> Self {
        Self {
            max_items: MAX_NODE_CONTEXT_ITEMS,
            max_item_chars: MAX_NODE_CONTEXT_ITEM_CHARS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NodeContextBuildError {
    #[error("task graph is invalid: {0}")]
    InvalidGraph(String),
    #[error("task node does not exist: {0}")]
    NodeNotFound(String),
    #[error("context field is not allowed: {0}")]
    ForbiddenField(&'static str),
    #[error("context field is invalid: {0}")]
    InvalidField(String),
    #[error("dependency output is not a direct dependency: {node_id}")]
    DependencyNotDeclared { node_id: String },
    #[error("dependency output is duplicated: {node_id}")]
    DuplicateDependency { node_id: String },
    #[error("bounded node context is invalid: {0}")]
    Context(#[from] NodeExecutionError),
}

/// Builds the small, explicit context supplied to one execution attempt.
/// Callers must provide already-observed dependency outputs; this builder does
/// not fetch conversation history, serialize the graph, or query providers.
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeContextBuilder {
    limits: NodeContextLimits,
}

impl NodeContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limits(limits: NodeContextLimits) -> Self {
        Self { limits }
    }

    pub fn build<I>(
        &self,
        graph: &TaskGraph,
        node_id: &TaskNodeId,
        dependency_outputs: I,
    ) -> Result<NodeContext, NodeContextBuildError>
    where
        I: IntoIterator<Item = (String, Value)>,
    {
        graph
            .validate()
            .map_err(|error| NodeContextBuildError::InvalidGraph(error.to_string()))?;
        let node = graph
            .node(node_id)
            .ok_or_else(|| NodeContextBuildError::NodeNotFound(node_id.to_string()))?;
        self.build_for_node(graph, node, dependency_outputs)
    }

    pub fn build_for_node<I>(
        &self,
        graph: &TaskGraph,
        node: &TaskNode,
        dependency_outputs: I,
    ) -> Result<NodeContext, NodeContextBuildError>
    where
        I: IntoIterator<Item = (String, Value)>,
    {
        let input = node
            .input
            .as_object()
            .ok_or_else(|| NodeContextBuildError::InvalidField("input must be an object".into()))?;
        for (field, error_name) in FORBIDDEN_CONTEXT_FIELDS {
            if input.contains_key(*field) {
                return Err(NodeContextBuildError::ForbiddenField(error_name));
            }
        }

        let goal = optional_text(input, "goal")?.unwrap_or_else(|| node.title.clone());
        let instructions = optional_text(input, "instructions")?
            .or(optional_text(input, "instruction")?)
            .unwrap_or_else(|| node.title.clone());
        let resources = bounded_text_list(input, "resources", self.limits)?;
        let memory_references = bounded_text_list(input, "memory_references", self.limits)?;
        let capabilities = bounded_text_list(input, "capabilities", self.limits)?;
        let constraints = bounded_text_list(input, "constraints", self.limits)?;
        let acceptance_criteria = bounded_text_list(input, "acceptance_criteria", self.limits)?;

        let declared: BTreeSet<String> = graph
            .dependencies(&node.id)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        let mut projected = BTreeMap::new();
        for (dependency_id, output) in dependency_outputs {
            if !declared.contains(&dependency_id) {
                return Err(NodeContextBuildError::DependencyNotDeclared {
                    node_id: dependency_id,
                });
            }
            if projected.insert(dependency_id.clone(), output).is_some() {
                return Err(NodeContextBuildError::DuplicateDependency {
                    node_id: dependency_id,
                });
            }
        }
        let context = NodeContext::new(
            goal,
            instructions,
            resources,
            projected.into_iter().collect(),
            memory_references,
            capabilities,
            constraints,
            acceptance_criteria,
        )?;
        Ok(context)
    }
}

fn optional_text(
    input: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, NodeContextBuildError> {
    let Some(value) = input.get(field) else {
        return Ok(None);
    };
    let Some(text) = value.as_str() else {
        return Err(NodeContextBuildError::InvalidField(format!(
            "{field} must be a string"
        )));
    };
    Ok(Some(text.to_string()))
}

fn bounded_text_list(
    input: &serde_json::Map<String, Value>,
    field: &'static str,
    limits: NodeContextLimits,
) -> Result<Vec<String>, NodeContextBuildError> {
    let Some(value) = input.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(NodeContextBuildError::InvalidField(format!(
            "{field} must be an array of strings"
        )));
    };
    if values.len() > limits.max_items {
        return Err(NodeContextBuildError::InvalidField(format!(
            "{field} exceeds {} items",
            limits.max_items
        )));
    }
    values
        .iter()
        .map(|value| {
            let Some(text) = value.as_str() else {
                return Err(NodeContextBuildError::InvalidField(format!(
                    "{field} must contain only strings"
                )));
            };
            if text.chars().count() > limits.max_item_chars || text.chars().any(char::is_control) {
                return Err(NodeContextBuildError::InvalidField(format!(
                    "{field} contains an item outside bounded limits"
                )));
            }
            Ok(text.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS;
    use crate::task::{
        GraphRevision, TaskEdge, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
    };
    use serde_json::json;

    fn context_graph(input: serde_json::Value) -> (TaskGraph, TaskNodeId, TaskNodeId) {
        let source = TaskNodeId::new("source").unwrap();
        let target = TaskNodeId::new("target").unwrap();
        let graph = TaskGraph::new(
            TaskGraphId::new("context-graph").unwrap(),
            GraphRevision::initial(),
            vec![
                TaskNode::new(source.clone(), TaskNodeKind::Work, "Source", json!({})).unwrap(),
                TaskNode::new(target.clone(), TaskNodeKind::Work, "Target", input).unwrap(),
            ],
            vec![TaskEdge::new(source.clone(), target.clone())],
        )
        .unwrap();
        (graph, source, target)
    }

    #[test]
    fn builder_projects_only_direct_bounded_dependencies_and_resources() {
        let (graph, source, target) = context_graph(json!({
            "goal": "write a report",
            "instructions": "use only verified inputs",
            "resources": ["resource://brief"],
            "memory_references": ["memory:brief"],
            "capabilities": ["reports.read"],
            "constraints": ["offline"],
            "acceptance_criteria": ["result.status == success"]
        }));
        let context = NodeContextBuilder::new()
            .build(
                &graph,
                &target,
                [(source.to_string(), json!({"verified": true}))],
            )
            .unwrap();
        assert_eq!(context.goal, "write a report");
        assert_eq!(context.dependency_outputs.len(), 1);
        assert_eq!(context.resources, vec!["resource://brief"]);
        assert_eq!(context.memory_references, vec!["memory:brief"]);
    }

    #[test]
    fn builder_rejects_whole_graph_or_conversation_passthrough() {
        let (graph, _, target) = context_graph(json!({
            "conversation": [{"role": "user", "content": "secret"}]
        }));
        assert!(matches!(
            NodeContextBuilder::new().build(&graph, &target, []),
            Err(NodeContextBuildError::ForbiddenField("conversation"))
        ));

        let (graph, source, target) = context_graph(json!({}));
        assert!(matches!(
            NodeContextBuilder::new().build(
                &graph,
                &target,
                [("unrelated".to_string(), json!({"whole_graph": true}))],
            ),
            Err(NodeContextBuildError::DependencyNotDeclared { .. })
        ));
        let _ = source;
    }

    #[test]
    fn builder_applies_context_bounds() {
        let (graph, _, target) = context_graph(json!({
            "resources": ["x".repeat(MAX_NODE_CONTEXT_ITEM_CHARS + 1)]
        }));
        assert!(matches!(
            NodeContextBuilder::new().build(&graph, &target, []),
            Err(NodeContextBuildError::InvalidField(_))
        ));
    }
}
