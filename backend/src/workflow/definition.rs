// ============================================================
// Workflow graph definition — the static, typed DAG model.
//
// A [`WorkflowGraphDefinition`] describes WHAT a workflow should run and WHEN
// (its node ordering), but never carries execution policy. Security decisions
// remain the responsibility of the Trusted Execution layer.
// ============================================================

use serde::{Deserialize, Serialize};

use super::validation::{validate_graph, WorkflowValidationError};

/// Current schema version for the workflow graph definition.
pub const WORKFLOW_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Maximum number of nodes allowed in a single workflow graph.
pub const MAX_WORKFLOW_NODES: usize = 64;

/// Maximum number of edges allowed in a single workflow graph.
pub const MAX_WORKFLOW_EDGES: usize = 256;

/// Maximum length of a single node id, in bytes.
const MAX_WORKFLOW_NODE_ID_LEN: usize = 64;

/// A validated workflow node identifier.
///
/// Accepts only ASCII letters, digits, `_`, `-`, and `.`, with a length of
/// 1..=64. Empty ids, spaces, control characters, path separators (`/`, `\`),
/// and any non-ASCII character are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowNodeId(String);

impl WorkflowNodeId {
    /// Construct a validated node id.
    pub fn new(raw: impl Into<String>) -> Result<Self, WorkflowValidationError> {
        let raw = raw.into();
        if raw.is_empty() || raw.len() > MAX_WORKFLOW_NODE_ID_LEN {
            return Err(WorkflowValidationError::InvalidNodeId(raw));
        }
        if raw.chars().any(|c| {
            !(c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.')
        }) {
            return Err(WorkflowValidationError::InvalidNodeId(raw));
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkflowNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The kind of execution a node represents.
///
/// This is the fixed, first-version set. No additional kinds are supported yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Agent,
    Tool,
    Subagent,
    Condition,
    Output,
}

/// A single node in a workflow graph.
///
/// In this first step it only carries an id and a kind — no arbitrary
/// per-node configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNodeDefinition {
    pub id: WorkflowNodeId,
    pub kind: WorkflowNodeKind,
}

/// A directed edge between two workflow nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdgeDefinition {
    pub from: WorkflowNodeId,
    pub to: WorkflowNodeId,
}

/// The static, typed definition of an executable workflow as a DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowGraphDefinition {
    pub schema_version: u32,
    pub entry_node_id: WorkflowNodeId,
    pub nodes: Vec<WorkflowNodeDefinition>,
    pub edges: Vec<WorkflowEdgeDefinition>,
}

impl WorkflowGraphDefinition {
    /// Validate this graph definition against every structural DAG invariant.
    pub fn validate(&self) -> Result<(), WorkflowValidationError> {
        validate_graph(self)
    }
}
