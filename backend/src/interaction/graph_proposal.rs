//! Preview-only graph mutation proposals for Task/Conversation interaction.

use thiserror::Error;

use crate::shared::interaction::{
    GraphMutationOperation, GraphMutationProposal, GraphProposalValidation,
};
use crate::task::{GraphRevision, TaskGraph, TaskGraphId, TaskWorldRuntime, TaskWorldRuntimeError};

const MAX_PROPOSAL_OPERATIONS: usize = 16;

#[derive(Debug, Error)]
pub enum GraphProposalError {
    #[error("graph proposal request is not a supported parallelization utterance")]
    UnsupportedRequest,
    #[error("graph proposal exceeds the bounded removal limit of {limit}; refusing to truncate")]
    OperationLimitExceeded { limit: usize },
    #[error("graph proposal revision overflow")]
    RevisionOverflow,
    #[error("task graph is not available: {0}")]
    Runtime(#[from] TaskWorldRuntimeError),
}

/// Reads the authoritative TaskGraph and builds an in-memory candidate.  It
/// never calls a TaskWorldRuntime mutation method.
#[derive(Clone)]
pub struct GraphProposalService {
    runtime: TaskWorldRuntime,
}

impl GraphProposalService {
    pub fn new(runtime: TaskWorldRuntime) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> TaskWorldRuntime {
        self.runtime.clone()
    }

    pub fn propose(
        &self,
        graph_id: &TaskGraphId,
        utterance: &str,
    ) -> Result<GraphMutationProposal, GraphProposalError> {
        if !is_parallelization_request(utterance) {
            return Err(GraphProposalError::UnsupportedRequest);
        }
        let graph = self
            .runtime
            .get_graph(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        self.propose_from_graph(&graph, utterance)
    }

    /// Pure helper exposed for callers that already hold an authoritative
    /// graph snapshot and want no runtime access at all.
    pub fn propose_from_graph(
        &self,
        graph: &TaskGraph,
        utterance: &str,
    ) -> Result<GraphMutationProposal, GraphProposalError> {
        if !is_parallelization_request(utterance) {
            return Err(GraphProposalError::UnsupportedRequest);
        }
        let test_nodes = graph
            .nodes
            .iter()
            .filter(|node| is_test_node(node.title.as_str(), node.id.as_str()))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        let document_nodes = graph
            .nodes
            .iter()
            .filter(|node| is_document_node(node.title.as_str(), node.id.as_str()))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();

        let mut candidate = graph.clone();
        let mut operations = Vec::new();
        let mut remaining_edges = Vec::with_capacity(candidate.edges.len());
        for edge in &candidate.edges {
            let connects_requested_nodes = (test_nodes.contains(&edge.from)
                && document_nodes.contains(&edge.to))
                || (document_nodes.contains(&edge.from) && test_nodes.contains(&edge.to));
            if connects_requested_nodes {
                if operations.len() >= MAX_PROPOSAL_OPERATIONS {
                    return Err(GraphProposalError::OperationLimitExceeded {
                        limit: MAX_PROPOSAL_OPERATIONS,
                    });
                }
                operations.push(GraphMutationOperation {
                    operation: "remove_dependency".to_string(),
                    node_id: None,
                    from_node_id: Some(edge.from.as_str().to_string()),
                    to_node_id: Some(edge.to.as_str().to_string()),
                });
            } else {
                remaining_edges.push(edge.clone());
            }
        }
        candidate.edges = remaining_edges;

        let valid = if operations.is_empty() {
            false
        } else {
            candidate.validate().is_ok()
        };
        let issues = if valid {
            Vec::new()
        } else if operations.is_empty() {
            vec!["未找到可拆除的测试与文档依赖".to_string()]
        } else {
            vec!["候选图校验失败".to_string()]
        };
        let candidate_revision = if operations.is_empty() {
            graph.revision.value()
        } else {
            graph
                .revision
                .value()
                .checked_add(1)
                .ok_or(GraphProposalError::RevisionOverflow)?
        };
        // Keep the candidate's revision explicit even though it is not
        // returned; validation therefore exercises the same revision shape a
        // later confirmed mutation would use.
        if valid {
            candidate.revision = GraphRevision::new(candidate_revision)
                .map_err(|_| GraphProposalError::RevisionOverflow)?;
        }

        let impact_summary = if operations.is_empty() {
            "没有可预览的依赖变更".to_string()
        } else {
            format!(
                "将移除 {} 条测试与文档之间的依赖，让两类工作可以并行",
                operations.len()
            )
        };
        Ok(GraphMutationProposal {
            proposal_id: uuid::Uuid::new_v4().to_string(),
            graph_id: graph.id.to_string(),
            base_revision: graph.revision.value(),
            candidate_revision,
            operations,
            validation: GraphProposalValidation { valid, issues },
            impact_summary,
            requires_confirmation: valid,
        })
    }
}

fn is_parallelization_request(utterance: &str) -> bool {
    let normalized = normalize(utterance);
    (normalized.contains("并行") || normalized.contains("parallel"))
        && (normalized.contains("测试")
            || normalized.contains("文档")
            || normalized.contains("test")
            || normalized.contains("document"))
}

fn is_test_node(title: &str, id: &str) -> bool {
    let value = format!("{}{}", normalize(title), normalize(id));
    value.contains("测试") || value.contains("test")
}

fn is_document_node(title: &str, id: &str) -> bool {
    let value = format!("{}{}", normalize(title), normalize(id));
    value.contains("文档") || value.contains("document") || value.contains("docs")
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(
                    character,
                    '。' | '！' | '？' | '?' | '!' | ',' | '，' | '、' | ':' | '：' | ';' | '；'
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}
