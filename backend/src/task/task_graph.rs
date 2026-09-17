//! v1 Task World's product-level task graph.
//!
//! This model is intentionally separate from [`crate::workflow`].  A
//! `TaskGraph` describes how a user organises work and its dependencies; a
//! `WorkflowGraph` remains the executable tool/agent DAG.  This module is
//! pure data and validation only.  It does not execute commands or persist
//! state.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::executor_ref::{validate_command_binding, CommandBinding};

pub const TASK_GRAPH_SCHEMA_VERSION: u32 = 1;
pub const MAX_TASK_GRAPH_NODES: usize = 128;
pub const MAX_TASK_GRAPH_EDGES: usize = 512;
pub const MAX_TASK_GRAPH_ID_CHARS: usize = 64;
pub const MAX_TASK_NODE_TITLE_CHARS: usize = 200;

/// A stable identity for a product TaskGraph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskGraphId(String);

impl TaskGraphId {
    pub fn new(raw: impl Into<String>) -> Result<Self, TaskGraphValidationError> {
        let raw = raw.into();
        if !valid_identifier(&raw) {
            return Err(TaskGraphValidationError::InvalidGraphId(raw));
        }
        Ok(Self(raw))
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskGraphId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated node identity within a TaskGraph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskNodeId(String);

impl TaskNodeId {
    pub fn new(raw: impl Into<String>) -> Result<Self, TaskGraphValidationError> {
        let raw = raw.into();
        if !valid_identifier(&raw) {
            return Err(TaskGraphValidationError::InvalidNodeId(raw));
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Monotonically increasing graph revision.  Revision zero is reserved for
/// an absent/uninitialised graph and cannot be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GraphRevision(u64);

impl GraphRevision {
    pub fn initial() -> Self {
        Self(1)
    }

    pub fn new(value: u64) -> Result<Self, TaskGraphValidationError> {
        if value == 0 {
            return Err(TaskGraphValidationError::InvalidRevision(value));
        }
        Ok(Self(value))
    }

    pub fn value(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> Result<Self, TaskGraphValidationError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(TaskGraphValidationError::RevisionOverflow)
    }
}

/// Product-level kind.  These are semantic labels; execution of any work is
/// deliberately outside this pure Task World core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskNodeKind {
    Work,
    Approval,
    UserCheckpoint,
}

/// Retry policy carried by a node definition for the later execution slice.
/// A value of one means that the first attempt is the only allowed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 1 }
    }
}

impl RetryPolicy {
    pub fn new(max_attempts: u32) -> Result<Self, TaskGraphValidationError> {
        if max_attempts == 0 {
            return Err(TaskGraphValidationError::InvalidRetryPolicy);
        }
        Ok(Self { max_attempts })
    }
}

/// A product task node.  `input` is structured data owned by the graph; the
/// supervisor keeps runtime output separately so a graph edit can invalidate
/// only the affected results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: TaskNodeId,
    pub kind: TaskNodeKind,
    pub title: String,
    pub input: serde_json::Value,
    pub retry_policy: RetryPolicy,
}

impl TaskNode {
    pub fn new(
        id: TaskNodeId,
        kind: TaskNodeKind,
        title: impl Into<String>,
        input: serde_json::Value,
    ) -> Result<Self, TaskGraphValidationError> {
        let title = title.into();
        validate_node_title(&id, &title)?;
        let node = Self {
            id,
            kind,
            title,
            input,
            retry_policy: RetryPolicy::default(),
        };
        node.validate()?;
        Ok(node)
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Validate fields that serde can construct without going through
    /// [`TaskNode::new`].  Command bindings are deliberately parsed by the
    /// executor-ref module so the graph stores one strict, auditable shape.
    pub fn validate(&self) -> Result<(), TaskGraphValidationError> {
        validate_node_title(&self.id, &self.title)?;
        validate_command_binding(&self.input)
            .map(|_| ())
            .map_err(|error| TaskGraphValidationError::InvalidCommandBinding(error.to_string()))
    }

    /// Return the validated command binding, if this node carries one.
    pub fn command_binding(&self) -> Result<Option<CommandBinding>, TaskGraphValidationError> {
        validate_command_binding(&self.input)
            .map_err(|error| TaskGraphValidationError::InvalidCommandBinding(error.to_string()))
    }
}

/// A directed dependency edge.  `from` must complete before `to` can run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEdge {
    pub from: TaskNodeId,
    pub to: TaskNodeId,
}

impl TaskEdge {
    pub fn new(from: TaskNodeId, to: TaskNodeId) -> Self {
        Self { from, to }
    }
}

/// A validated product DAG.  Multiple roots and branches are valid.  There is
/// intentionally no entry-node field: that is an executable WorkflowGraph
/// concern, not TaskGraph presentation/organisation semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskGraph {
    pub schema_version: u32,
    pub id: TaskGraphId,
    pub revision: GraphRevision,
    pub nodes: Vec<TaskNode>,
    pub edges: Vec<TaskEdge>,
}

impl TaskGraph {
    pub fn new(
        id: TaskGraphId,
        revision: GraphRevision,
        nodes: Vec<TaskNode>,
        edges: Vec<TaskEdge>,
    ) -> Result<Self, TaskGraphValidationError> {
        let graph = Self {
            schema_version: TASK_GRAPH_SCHEMA_VERSION,
            id,
            revision,
            nodes,
            edges,
        };
        graph.validate()?;
        Ok(graph)
    }

    pub fn validate(&self) -> Result<(), TaskGraphValidationError> {
        if self.schema_version != TASK_GRAPH_SCHEMA_VERSION {
            return Err(TaskGraphValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if !valid_identifier(self.id.as_str()) {
            return Err(TaskGraphValidationError::InvalidGraphId(
                self.id.to_string(),
            ));
        }
        if self.revision.value() == 0 {
            return Err(TaskGraphValidationError::InvalidRevision(0));
        }
        if self.nodes.len() > MAX_TASK_GRAPH_NODES {
            return Err(TaskGraphValidationError::TooManyNodes(self.nodes.len()));
        }
        if self.edges.len() > MAX_TASK_GRAPH_EDGES {
            return Err(TaskGraphValidationError::TooManyEdges(self.edges.len()));
        }

        let mut indexes = HashMap::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            if !valid_identifier(node.id.as_str()) {
                return Err(TaskGraphValidationError::InvalidNodeId(node.id.to_string()));
            }
            node.validate()?;
            if indexes.insert(node.id.as_str(), index).is_some() {
                return Err(TaskGraphValidationError::DuplicateNodeId(
                    node.id.to_string(),
                ));
            }
            if node.retry_policy.max_attempts == 0 {
                return Err(TaskGraphValidationError::InvalidRetryPolicy);
            }
        }

        let mut adjacency = vec![Vec::new(); self.nodes.len()];
        let mut seen_edges = HashSet::with_capacity(self.edges.len());
        for edge in &self.edges {
            let Some(&from) = indexes.get(edge.from.as_str()) else {
                return Err(TaskGraphValidationError::MissingEdgeEndpoint {
                    from: edge.from.to_string(),
                    to: edge.to.to_string(),
                });
            };
            let Some(&to) = indexes.get(edge.to.as_str()) else {
                return Err(TaskGraphValidationError::MissingEdgeEndpoint {
                    from: edge.from.to_string(),
                    to: edge.to.to_string(),
                });
            };
            if from == to {
                return Err(TaskGraphValidationError::SelfEdge(edge.from.to_string()));
            }
            if !seen_edges.insert((from, to)) {
                return Err(TaskGraphValidationError::DuplicateEdge {
                    from: edge.from.to_string(),
                    to: edge.to.to_string(),
                });
            }
            adjacency[from].push(to);
        }

        let mut indegree = vec![0usize; self.nodes.len()];
        for neighbors in &adjacency {
            for &to in neighbors {
                indegree[to] += 1;
            }
        }
        let mut queue = VecDeque::new();
        for (index, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(index);
            }
        }
        let mut visited = 0usize;
        while let Some(index) = queue.pop_front() {
            visited += 1;
            for &to in &adjacency[index] {
                indegree[to] -= 1;
                if indegree[to] == 0 {
                    queue.push_back(to);
                }
            }
        }
        if visited != self.nodes.len() {
            let node = indegree
                .iter()
                .position(|degree| *degree > 0)
                .expect("a partial topological sort leaves a node in a cycle");
            return Err(TaskGraphValidationError::CycleDetected(
                self.nodes[node].id.to_string(),
            ));
        }

        Ok(())
    }

    pub fn node(&self, node_id: &TaskNodeId) -> Option<&TaskNode> {
        self.nodes.iter().find(|node| &node.id == node_id)
    }

    pub(crate) fn node_mut(&mut self, node_id: &TaskNodeId) -> Option<&mut TaskNode> {
        self.nodes.iter_mut().find(|node| &node.id == node_id)
    }

    /// Direct predecessors in edge definition order.
    pub fn dependencies(&self, node_id: &TaskNodeId) -> Vec<TaskNodeId> {
        self.edges
            .iter()
            .filter(|edge| &edge.to == node_id)
            .map(|edge| edge.from.clone())
            .collect()
    }

    /// Direct successors in edge definition order.
    pub fn dependents(&self, node_id: &TaskNodeId) -> Vec<TaskNodeId> {
        self.edges
            .iter()
            .filter(|edge| &edge.from == node_id)
            .map(|edge| edge.to.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskGraphValidationError {
    #[error("unsupported task graph schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("invalid task graph id: {0}")]
    InvalidGraphId(String),
    #[error("invalid task node id: {0}")]
    InvalidNodeId(String),
    #[error("invalid graph revision: {0}")]
    InvalidRevision(u64),
    #[error("too many task graph nodes: {0}")]
    TooManyNodes(usize),
    #[error("too many task graph edges: {0}")]
    TooManyEdges(usize),
    #[error("duplicate task node id: {0}")]
    DuplicateNodeId(String),
    #[error("invalid task node title: {0}")]
    InvalidNodeTitle(String),
    #[error("invalid task command binding: {0}")]
    InvalidCommandBinding(String),
    #[error("invalid retry policy")]
    InvalidRetryPolicy,
    #[error("edge references a missing endpoint: {from} -> {to}")]
    MissingEdgeEndpoint { from: String, to: String },
    #[error("task edge must not point to itself: {0}")]
    SelfEdge(String),
    #[error("duplicate task edge: {from} -> {to}")]
    DuplicateEdge { from: String, to: String },
    #[error("task graph cycle detected involving node: {0}")]
    CycleDetected(String),
    #[error("task graph revision overflow")]
    RevisionOverflow,
}

fn valid_identifier(raw: &str) -> bool {
    !raw.is_empty()
        && raw.chars().count() <= MAX_TASK_GRAPH_ID_CHARS
        && raw.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '_'
                || character == '-'
                || character == '.'
        })
}

fn validate_node_title(id: &TaskNodeId, title: &str) -> Result<(), TaskGraphValidationError> {
    if title.trim().is_empty() || title.chars().count() > MAX_TASK_NODE_TITLE_CHARS {
        return Err(TaskGraphValidationError::InvalidNodeTitle(id.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn focus_input() -> serde_json::Value {
        json!({
            "executor_ref": "command://desktop.app.focus",
            "command_binding": {
                "command": "desktop.app.focus",
                "args": {"app_id": "app:code.exe"}
            }
        })
    }

    #[test]
    fn task_node_constructor_rejects_invalid_focus_binding() {
        let invalid = json!({
            "executor_ref": "command://desktop.app.focus",
            "command_binding": {
                "command": "desktop.app.focus",
                "args": {"app_id": "app:code.exe", "pid": 42}
            }
        });

        let error = TaskNode::new(
            TaskNodeId::new("focus").unwrap(),
            TaskNodeKind::Work,
            "Focus app",
            invalid,
        )
        .expect_err("constructor must enforce the command binding schema");
        assert!(matches!(
            error,
            TaskGraphValidationError::InvalidCommandBinding(_)
        ));
    }

    #[test]
    fn empty_task_graph_is_valid_as_an_editable_canvas() {
        let graph = TaskGraph::new(
            TaskGraphId::new("empty-canvas").unwrap(),
            GraphRevision::initial(),
            Vec::new(),
            Vec::new(),
        );

        assert!(graph.is_ok());
    }

    #[test]
    fn graph_validation_rechecks_serde_constructed_focus_nodes() {
        let node: TaskNode = serde_json::from_value(json!({
            "id": "focus",
            "kind": "work",
            "title": "Focus app",
            "input": focus_input(),
            "retry_policy": {"max_attempts": 1}
        }))
        .unwrap();
        let graph = TaskGraph {
            schema_version: TASK_GRAPH_SCHEMA_VERSION,
            id: TaskGraphId::new("focus-graph").unwrap(),
            revision: GraphRevision::initial(),
            nodes: vec![node],
            edges: Vec::new(),
        };
        assert!(graph.validate().is_ok());

        let mut invalid_graph = graph;
        invalid_graph.nodes[0].input["command_binding"]["args"] =
            json!({"app_id": "app:code.exe", "hwnd": 123});
        let error = invalid_graph
            .validate()
            .expect_err("graph validation must not trust serde input");
        assert!(matches!(
            error,
            TaskGraphValidationError::InvalidCommandBinding(_)
        ));
    }
}
