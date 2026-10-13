//! Evidence collection and normalization for ProofLoop goal acceptance.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/02-goal-acceptance.md

use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::contracts::{
    DeliverableArtifactMapping, EvidenceRef, EvidenceSnapshot, EvidenceSource, ExecutionRef,
    GoalContract, ProofError,
};
use crate::db::Database;
use crate::modules::task::artifact::ArtifactService;
use crate::modules::task::execution::{NodeExecution, NodeExecutionId, NodeExecutionStatus};
use crate::modules::task::TaskGraphId;

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
    check_graph_revision(db, graph_id, graph_revision)?;
    let draft = db
        .confirmed_proof_draft_for_goal(&contract.goal_id, contract.version)
        .map_err(|error| ProofError::new("db_error", error.to_string()))?;
    if draft
        .as_ref()
        .is_some_and(|draft| draft.graph_id != graph_id)
    {
        return Err(ProofError::new(
            "execution_graph_mismatch",
            "目标与任务图关联不一致",
        ));
    }
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
        let latest = db
            .load_node_executions(&exec.graph_id, &exec.node_id)
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
            .into_iter()
            .max_by_key(|item| item.attempt);
        if exec.status == NodeExecutionStatus::Stale
            || latest.is_some_and(|item| item.id != exec.id)
        {
            return Err(ProofError::new(
                "proofloop_evidence_stale",
                "执行记录已失效或已被新的 attempt 替代",
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
        if let Some(val) = exec.output.as_ref().filter(|_| verified_execution(&exec)) {
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

                        sources.push(EvidenceSource {
                            source_id: format!("{}_{}", ev_id, idx),
                            evidence_id: ev_id.clone(),
                            uri,
                            retrieved_at_ms: retrieved_at,
                            cited_by_deliverable_ids: Vec::new(),
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

    for del in &contract.deliverables {
        let expected_node = draft
            .as_ref()
            .and_then(|draft| draft.candidate.as_ref())
            .and_then(|plan| {
                plan.deliverable_nodes
                    .iter()
                    .find(|item| item.deliverable_id == del.id)
            })
            .map(|item| item.node_id.as_str());
        let selected_execution = loaded_executions.iter().find(|execution| {
            verified_execution(execution) && expected_node == Some(execution.node_id.as_str())
        });
        let Some(selected_execution) = selected_execution else {
            continue;
        };
        let artifact_ids = db
            .find_proofloop_artifact_ids(
                &contract.workspace_id,
                graph_id,
                selected_execution.node_id.as_str(),
                selected_execution.id.as_str(),
                &del.id,
                &del.title,
            )
            .map_err(|error| ProofError::new("db_error", error.to_string()))?;
        let Some(artifact_id) = artifact_ids.first() else {
            continue;
        };
        let artifact_id = crate::modules::task::model::ArtifactId::new(artifact_id)
            .map_err(|error| ProofError::new("invalid_artifact_id", error.to_string()))?;
        let artifact = db
            .get_artifact(&artifact_id)
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
            .ok_or_else(|| {
                ProofError::new("artifact_read_failed", "精确定位的交付物产物记录已不存在")
            })?;
        let provenance = db
            .artifact_provenance(artifact.id.as_str())
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
            .ok_or_else(|| ProofError::new("artifact_origin_mismatch", "交付物产物缺少来源记录"))?;
        let provenance_matches = matches!(
            &provenance.source,
            crate::modules::task::artifact::ArtifactSource::Node {
                graph_id: source_graph,
                node_id: source_node,
                execution_id: source_execution,
                workspace_id: source_workspace,
            } if source_graph.as_str() == graph_id
                && Some(source_node.as_str()) == expected_node
                && source_execution.as_str() == selected_execution.id.as_str()
                && source_workspace == &contract.workspace_id
        );
        if !provenance_matches {
            continue;
        }

        let art = artifact;
        {
            let art_id_str = art.id.as_str().to_string();
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
            if digest != provenance.sha256 {
                return Err(ProofError::new(
                    "artifact_read_failed",
                    format!("交付物 '{}' 的文件摘要与来源记录不一致", del.id),
                ));
            }
            deliverable_mappings.push(DeliverableArtifactMapping {
                deliverable_id: del.id.clone(),
                artifact_id: art_id_str.clone(),
            });
            let text = String::from_utf8_lossy(&bytes);
            for source in &mut sources {
                if text.contains(&source.uri) {
                    source.cited_by_deliverable_ids.push(del.id.clone());
                }
            }
            evidence_content.insert(art_id_str.clone(), bytes);

            let origin_exec_ref = executions
                .iter()
                .find(|e| e.execution_id == art.task_execution_id.as_str())
                .cloned()
                .ok_or_else(|| {
                    ProofError::new("proofloop_evidence_stale", "产物没有对应的选定执行证据")
                })?;

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

fn verified_execution(execution: &NodeExecution) -> bool {
    execution.status == NodeExecutionStatus::Succeeded
        && execution.validation.as_ref().is_some_and(|validation| {
            validation.status == crate::modules::task::execution::ValidationStatus::Accepted
        })
}

fn check_graph_revision(db: &Database, graph_id: &str, revision: u64) -> Result<(), ProofError> {
    let id = TaskGraphId::new(graph_id)
        .map_err(|error| ProofError::new("invalid_graph_id", error.to_string()))?;
    let stored = db
        .load_task_supervisor_snapshot(&id)
        .map_err(|error| ProofError::new("db_error", error.to_string()))?
        .ok_or_else(|| ProofError::new("graph_not_found", "找不到任务图"))?;
    if stored.supervisor.graph_revision().value() != revision {
        return Err(ProofError::new(
            "proofloop_evidence_stale",
            "图版本已改变，请重新选择执行证据",
        ));
    }
    Ok(())
}

pub fn reload_snapshot_contents(
    db: &Database,
    artifacts: &ArtifactService,
    snapshot: &EvidenceSnapshot,
    allowed_root: &str,
) -> Result<HashMap<String, Vec<u8>>, ProofError> {
    check_graph_revision(db, &snapshot.graph_id, snapshot.graph_revision)?;
    for selected in &snapshot.executions {
        let id = NodeExecutionId::new(&selected.execution_id)
            .map_err(|error| ProofError::new("invalid_execution_id", error.to_string()))?;
        let current = db
            .load_node_execution(&id)
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
            .ok_or_else(|| ProofError::new("proofloop_evidence_stale", "执行证据不存在"))?;
        let latest = db
            .load_node_executions(&current.graph_id, &current.node_id)
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
            .into_iter()
            .max_by_key(|execution| execution.attempt);
        if current.graph_id.as_str() != snapshot.graph_id
            || current.status == NodeExecutionStatus::Stale
            || latest.is_some_and(|execution| execution.id != id)
        {
            return Err(ProofError::new(
                "proofloop_evidence_stale",
                "执行证据已被重跑或失效",
            ));
        }
    }
    let mut content = HashMap::new();
    for evidence in &snapshot.evidence {
        if let Some(id) = &evidence.artifact_id {
            let artifact_id = crate::modules::task::model::ArtifactId::new(id)
                .map_err(|error| ProofError::new("invalid_artifact_id", error.to_string()))?;
            let (_, bytes) = artifacts
                .read_bytes(&artifact_id, allowed_root)
                .map_err(|error| ProofError::new("artifact_read_failed", error.to_string()))?;
            if format!("{:x}", Sha256::digest(&bytes)) != evidence.digest {
                return Err(ProofError::new(
                    "proofloop_evidence_stale",
                    "产物内容与原验收快照不一致",
                ));
            }
            content.insert(id.clone(), bytes);
        }
    }
    Ok(content)
}
