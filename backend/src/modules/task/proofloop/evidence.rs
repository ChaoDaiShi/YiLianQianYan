//! Evidence collection and normalization for ProofLoop goal acceptance.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/02-goal-acceptance.md

use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::contracts::{
    DeliverableArtifactMapping, EvidenceRef, EvidenceSnapshot, EvidenceSource, ExecutionRef,
    GoalContract, ProofError,
};
use crate::db::{ArtifactQuery, Database};
use crate::modules::task::artifact::ArtifactService;
use crate::modules::task::execution::{NodeExecution, NodeExecutionId};
use crate::workspace::WorkspaceId;

pub fn collect_evidence(
    db: &Database,
    artifact_service: &ArtifactService,
    contract: &GoalContract,
    graph_id: &str,
    graph_revision: u64,
    execution_ids: &[String],
    allowed_root: &str,
    _now_ms: i64,
) -> Result<(EvidenceSnapshot, HashMap<String, Vec<u8>>), ProofError> {
    if execution_ids.is_empty() {
        return Err(ProofError::new(
            "empty_execution_ids",
            "必须提供明确的执行 ID 列表，禁止隐式查询全局最新输出",
        ));
    }

    let mut executions: Vec<ExecutionRef> = Vec::new();
    let mut evidence_refs: Vec<EvidenceRef> = Vec::new();
    let mut loaded_executions: Vec<NodeExecution> = Vec::new();
    let mut sources: Vec<EvidenceSource> = Vec::new();

    // 1. Load and validate executions
    for eid_str in execution_ids {
        let node_exec_id = NodeExecutionId::new(eid_str)
            .map_err(|e| ProofError::new("invalid_execution_id", e.to_string()))?;

        let exec = db
            .load_node_execution(&node_exec_id)
            .map_err(|e| ProofError::new("db_error", e.to_string()))?
            .ok_or_else(|| {
                ProofError::new(
                    "execution_not_found",
                    format!("未找到执行记录 '{}'", eid_str),
                )
            })?;

        if exec.graph_id.as_str() != graph_id {
            return Err(ProofError::new(
                "execution_graph_mismatch",
                format!(
                    "执行记录 '{}' 属于图 '{}'，与当前图 '{}' 不一致",
                    eid_str,
                    exec.graph_id.as_str(),
                    graph_id
                ),
            ));
        }

        let exec_ref = ExecutionRef {
            graph_id: exec.graph_id.as_str().to_string(),
            graph_revision,
            node_id: exec.node_id.as_str().to_string(),
            execution_id: exec.id.as_str().to_string(),
        };

        // Output digest
        let (output_digest, output_path) = match &exec.output {
            Some(val) => {
                let serialized = serde_json::to_vec(val).unwrap_or_default();
                (
                    format!("{:x}", Sha256::digest(&serialized)),
                    Some(vec!["structured".to_string()]),
                )
            }
            None => ("none".to_string(), None),
        };

        let ev_id = format!("ev_{}", exec.id.as_str());
        let observed_at = exec.finished_at.unwrap_or(exec.updated_at);

        evidence_refs.push(EvidenceRef {
            evidence_id: ev_id.clone(),
            origin: exec_ref.clone(),
            artifact_id: None,
            output_path,
            digest: output_digest,
            observed_at_ms: observed_at,
        });

        // Normalize structured sources if present in output
        if let Some(val) = &exec.output {
            let sources_val = val
                .get("structured")
                .and_then(|s| s.get("sources"))
                .or_else(|| val.get("sources"));

            if let Some(src_arr) = sources_val.and_then(|v| v.as_array()) {
                for (idx, item) in src_arr.iter().enumerate() {
                    let uri = item
                        .get("uri")
                        .or_else(|| item.get("url"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !uri.is_empty() {
                        let retrieved_at = item
                            .get("retrieved_at_ms")
                            .or_else(|| item.get("retrieved_at"))
                            .and_then(|t| t.as_i64())
                            .unwrap_or(observed_at);

                        let cited_deliverables: Vec<String> =
                            contract.deliverables.iter().map(|d| d.id.clone()).collect();

                        sources.push(EvidenceSource {
                            source_id: format!("{}_{}", ev_id, idx),
                            evidence_id: ev_id.clone(),
                            uri,
                            retrieved_at_ms: retrieved_at,
                            cited_by_deliverable_ids: cited_deliverables,
                        });
                    }
                }
            }
        }

        executions.push(exec_ref);
        loaded_executions.push(exec);
    }

    // 2. Locate deliverables and read artifact content
    let mut deliverable_mappings: Vec<DeliverableArtifactMapping> = Vec::new();
    let mut evidence_content: HashMap<String, Vec<u8>> = HashMap::new();

    let ws_id = WorkspaceId::new(&contract.workspace_id)
        .map_err(|e| ProofError::new("invalid_workspace_id", e.to_string()))?;

    let artifacts = db
        .list_artifacts(&ArtifactQuery {
            workspace_id: Some(ws_id),
            limit: Some(100),
            ..Default::default()
        })
        .map_err(|e| ProofError::new("db_error", e.to_string()))?;

    for del in &contract.deliverables {
        let matched = artifacts.iter().find(|art| {
            let matches_exec = loaded_executions
                .iter()
                .any(|e| e.id.as_str() == art.task_execution_id.as_str());

            matches_exec
                || art.name == del.title
                || art.name.contains(&del.id)
                || art.id.as_str() == del.id
        });

        if let Some(art) = matched {
            let art_id_str = art.id.as_str().to_string();
            deliverable_mappings.push(DeliverableArtifactMapping {
                deliverable_id: del.id.clone(),
                artifact_id: art_id_str.clone(),
            });

            let (_, bytes) = match artifact_service.read_bytes(&art.id, allowed_root) {
                Ok(res) => res,
                Err(e) => {
                    return Err(ProofError::new(
                        "artifact_read_failed",
                        format!("读取交付物产物失败: {}", e),
                    ));
                }
            };

            let digest = format!("{:x}", Sha256::digest(&bytes));
            evidence_content.insert(art_id_str.clone(), bytes);

            let origin_exec_ref = executions
                .iter()
                .find(|e| e.execution_id == art.task_execution_id.as_str())
                .cloned()
                .unwrap_or_else(|| ExecutionRef {
                    graph_id: graph_id.to_string(),
                    graph_revision,
                    node_id: "artifact".to_string(),
                    execution_id: art.task_execution_id.as_str().to_string(),
                });

            evidence_refs.push(EvidenceRef {
                evidence_id: format!("art_{}", art_id_str),
                origin: origin_exec_ref,
                artifact_id: Some(art_id_str),
                output_path: None,
                digest,
                observed_at_ms: art.updated_at,
            });
        }
    }

    let snapshot = EvidenceSnapshot {
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        graph_id: graph_id.to_string(),
        graph_revision,
        executions,
        evidence: evidence_refs,
        deliverables: deliverable_mappings,
        sources,
    };

    Ok((snapshot, evidence_content))
}
