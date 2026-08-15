// ============================================================
// Workflow graph validation — structural DAG invariants.
//
// A graph is valid when it has exactly one reachable entry point with no
// incoming edges, no duplicate nodes or edges, no self-edges, no directed
// cycles, and every node is reachable from the entry node. Validation is
// deterministic and returns the first violated invariant.
// ============================================================

use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use super::definition::{
    WorkflowGraphDefinition, MAX_WORKFLOW_EDGES, MAX_WORKFLOW_NODES, WORKFLOW_GRAPH_SCHEMA_VERSION,
};

/// A safe, non-leaking validation error surface.
///
/// Errors carry only node/edge identifiers supplied by the caller, never raw
/// database errors, filesystem paths, or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkflowValidationError {
    #[error("unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("workflow graph must contain at least one node")]
    EmptyGraph,
    #[error("too many nodes: {0}")]
    TooManyNodes(usize),
    #[error("too many edges: {0}")]
    TooManyEdges(usize),
    #[error("invalid node id: {0}")]
    InvalidNodeId(String),
    #[error("duplicate node id: {0}")]
    DuplicateNodeId(String),
    #[error("entry node is missing: {0}")]
    EntryNodeMissing(String),
    #[error("entry node must not have incoming edges: {0}")]
    EntryNodeHasIncomingEdge(String),
    #[error("edge references a missing endpoint: {0} -> {1}")]
    MissingEdgeEndpoint(String, String),
    #[error("edge must not point to itself: {0}")]
    SelfEdge(String),
    #[error("duplicate edge: {0} -> {1}")]
    DuplicateEdge(String, String),
    #[error("cycle detected involving node: {0}")]
    CycleDetected(String),
    #[error("node is unreachable from the entry node: {0}")]
    UnreachableNode(String),
}

/// Validate a graph definition, returning the first violated invariant.
pub fn validate_graph(def: &WorkflowGraphDefinition) -> Result<(), WorkflowValidationError> {
    // 1. Schema version must be supported.
    if def.schema_version != WORKFLOW_GRAPH_SCHEMA_VERSION {
        return Err(WorkflowValidationError::UnsupportedSchemaVersion(
            def.schema_version,
        ));
    }

    // 2. Must contain at least one node.
    if def.nodes.is_empty() {
        return Err(WorkflowValidationError::EmptyGraph);
    }

    // 3. Node count must be within bounds.
    if def.nodes.len() > MAX_WORKFLOW_NODES {
        return Err(WorkflowValidationError::TooManyNodes(def.nodes.len()));
    }

    // 4. Edge count must be within bounds.
    if def.edges.len() > MAX_WORKFLOW_EDGES {
        return Err(WorkflowValidationError::TooManyEdges(def.edges.len()));
    }

    // 5. Node ids must be unique.
    let mut node_ids: HashSet<&str> = HashSet::new();
    for node in &def.nodes {
        if !node_ids.insert(node.id.as_str()) {
            return Err(WorkflowValidationError::DuplicateNodeId(
                node.id.to_string(),
            ));
        }
    }

    // 6. Entry node must exist.
    if !node_ids.contains(def.entry_node_id.as_str()) {
        return Err(WorkflowValidationError::EntryNodeMissing(
            def.entry_node_id.to_string(),
        ));
    }

    // Build adjacency + indegree maps (every node starts present and empty).
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    for node in &def.nodes {
        adjacency.insert(node.id.as_str(), Vec::new());
        indegree.insert(node.id.as_str(), 0);
    }
    let mut seen_edges: HashSet<(&str, &str)> = HashSet::new();

    // 7-9. Validate each edge: endpoints exist, no self-edge, no duplicate.
    for edge in &def.edges {
        let from = edge.from.as_str();
        let to = edge.to.as_str();
        if !node_ids.contains(from) {
            return Err(WorkflowValidationError::MissingEdgeEndpoint(
                edge.from.to_string(),
                edge.to.to_string(),
            ));
        }
        if !node_ids.contains(to) {
            return Err(WorkflowValidationError::MissingEdgeEndpoint(
                edge.from.to_string(),
                edge.to.to_string(),
            ));
        }
        if from == to {
            return Err(WorkflowValidationError::SelfEdge(edge.from.to_string()));
        }
        if !seen_edges.insert((from, to)) {
            return Err(WorkflowValidationError::DuplicateEdge(
                edge.from.to_string(),
                edge.to.to_string(),
            ));
        }
        *indegree.get_mut(to).expect("node id known to exist") += 1;
        adjacency
            .get_mut(from)
            .expect("node id known to exist")
            .push(to);
    }

    // 10. Entry node must have no incoming edges.
    if *indegree
        .get(def.entry_node_id.as_str())
        .expect("entry node known to exist")
        != 0
    {
        return Err(WorkflowValidationError::EntryNodeHasIncomingEdge(
            def.entry_node_id.to_string(),
        ));
    }

    // 11. Every node must be reachable from the entry node.
    let reachable = reachable_nodes(def.entry_node_id.as_str(), &adjacency);
    for node in &def.nodes {
        if !reachable.contains(node.id.as_str()) {
            return Err(WorkflowValidationError::UnreachableNode(
                node.id.to_string(),
            ));
        }
    }

    // 12. No directed cycles.
    if let Some(node) = cycle_start(&adjacency) {
        return Err(WorkflowValidationError::CycleDetected(node));
    }

    Ok(())
}

/// Depth-first reachability from `start` within the adjacency map.
fn reachable_nodes<'a>(
    start: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
) -> HashSet<&'a str> {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    visited.insert(start);
    while let Some(node) = stack.pop() {
        if let Some(neighbors) = adjacency.get(node) {
            for &next in neighbors {
                if visited.insert(next) {
                    stack.push(next);
                }
            }
        }
    }
    visited
}

/// Kahn's algorithm: returns the id of a node involved in a cycle, or `None`
/// if the graph is acyclic.
fn cycle_start<'a>(adjacency: &HashMap<&'a str, Vec<&'a str>>) -> Option<String> {
    let mut indegree: HashMap<&'a str, usize> = HashMap::new();
    for &node in adjacency.keys() {
        indegree.insert(node, 0);
    }
    for neighbors in adjacency.values() {
        for &to in neighbors {
            *indegree.get_mut(to).expect("node id known to exist") += 1;
        }
    }

    let mut queue: VecDeque<&'a str> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&node, _)| node)
        .collect();

    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(neighbors) = adjacency.get(node) {
            for &to in neighbors {
                let d = indegree.get_mut(to).expect("node id known to exist");
                *d -= 1;
                if *d == 0 {
                    queue.push_back(to);
                }
            }
        }
    }

    if visited == adjacency.len() {
        None
    } else {
        indegree
            .into_iter()
            .find(|(_, d)| *d > 0)
            .map(|(node, _)| node.to_string())
    }
}
