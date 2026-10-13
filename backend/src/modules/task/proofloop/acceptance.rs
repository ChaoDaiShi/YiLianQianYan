//! Goal-level acceptance evaluator and criterion verifiers for ProofLoop.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/02-goal-acceptance.md

use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::contracts::{
    AcceptanceReport, CriterionCheck, CriterionResult, CriterionStatus, DeliverableResult,
    EvidenceSnapshot, GoalContract, GoalStatus, HumanReview, ProofError,
};

/// Evaluates a GoalContract against an EvidenceSnapshot and loaded artifact contents.
pub fn evaluate_goal(
    contract: &GoalContract,
    snapshot: &EvidenceSnapshot,
    evidence_content: &HashMap<String, Vec<u8>>,
    existing_reviews: &[HumanReview],
    now_ms: i64,
) -> Result<AcceptanceReport, ProofError> {
    if snapshot.goal_id != contract.goal_id {
        return Err(ProofError::new(
            "proofloop_evidence_stale",
            format!(
                "证据快照的目标 ID '{}' 与契约目标 ID '{}' 不一致",
                snapshot.goal_id, contract.goal_id
            ),
        ));
    }

    if snapshot.goal_version != contract.version {
        return Err(ProofError::new(
            "proofloop_evidence_stale",
            format!(
                "证据快照的目标版本 '{}' 与契约版本 '{}' 不一致",
                snapshot.goal_version, contract.version
            ),
        ));
    }

    let deliverable_results = evaluate_deliverables(contract, snapshot, evidence_content);
    let mut results = Vec::with_capacity(contract.criteria.len());

    for criterion in &contract.criteria {
        let (status, evidence_ids, reason) = evaluate_criterion(
            &criterion.check,
            snapshot,
            evidence_content,
            &deliverable_results,
            existing_reviews,
            &criterion.id,
        );

        results.push(CriterionResult {
            criterion_id: criterion.id.clone(),
            status,
            evidence_ids,
            reason,
        });
    }

    let has_required_criteria = contract.criteria.iter().any(|criterion| criterion.required);
    let mut required_statuses: Vec<CriterionStatus> = contract
        .criteria
        .iter()
        .zip(&results)
        .filter(|(c, _)| c.required)
        .map(|(_, r)| r.status)
        .collect();
    required_statuses.extend(deliverable_results.iter().map(|result| result.status));

    let goal_status = if contract.deliverables.is_empty() || !has_required_criteria {
        GoalStatus::Incomplete
    } else {
        aggregate_required(&required_statuses)
    };

    Ok(AcceptanceReport {
        report_id: uuid::Uuid::new_v4().to_string(),
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        graph_id: snapshot.graph_id.clone(),
        graph_revision: snapshot.graph_revision,
        snapshot_id: snapshot.snapshot_id.clone(),
        status: goal_status,
        results,
        deliverable_results,
        human_reviews: existing_reviews.to_vec(),
        checked_at_ms: now_ms,
    })
}

/// Checks every declared deliverable independently of the quality criteria.
pub fn evaluate_deliverables(
    contract: &GoalContract,
    snapshot: &EvidenceSnapshot,
    contents: &HashMap<String, Vec<u8>>,
) -> Vec<DeliverableResult> {
    contract
        .deliverables
        .iter()
        .map(|deliverable| {
            let mappings: Vec<_> = snapshot
                .deliverables
                .iter()
                .filter(|mapping| mapping.deliverable_id == deliverable.id)
                .collect();
            if mappings.is_empty() {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_missing",
                    CriterionStatus::Fail,
                    Vec::new(),
                    format!("未找到交付物 '{}' 的产物映射", deliverable.id),
                );
            }
            if mappings.len() != 1 {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_origin_mismatch",
                    CriterionStatus::Fail,
                    Vec::new(),
                    format!("交付物 '{}' 存在多个产物映射", deliverable.id),
                );
            }

            let artifact_id = mappings[0].artifact_id.as_str();
            let artifact_evidence: Vec<_> = snapshot
                .evidence
                .iter()
                .filter(|evidence| evidence.artifact_id.as_deref() == Some(artifact_id))
                .collect();
            if artifact_evidence.len() != 1 {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_origin_mismatch",
                    CriterionStatus::Fail,
                    Vec::new(),
                    format!("产物 '{}' 没有唯一的执行来源证据", artifact_id),
                );
            }

            let evidence = artifact_evidence[0];
            let origin = &evidence.origin;
            let selected_execution = snapshot.executions.iter().any(|execution| {
                execution.graph_id == snapshot.graph_id
                    && execution.graph_revision == snapshot.graph_revision
                    && origin.graph_id == snapshot.graph_id
                    && origin.graph_revision == snapshot.graph_revision
                    && execution.node_id == origin.node_id
                    && execution.execution_id == origin.execution_id
            });
            if !selected_execution {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_origin_mismatch",
                    CriterionStatus::Fail,
                    vec![evidence.evidence_id.clone()],
                    format!("产物 '{}' 不属于本次选定的图、节点和执行", artifact_id),
                );
            }

            let content = match contents.get(artifact_id) {
                Some(content) => content,
                None => {
                    return deliverable_result(
                        deliverable.id.clone(),
                        "artifact_missing",
                        CriterionStatus::Fail,
                        vec![evidence.evidence_id.clone()],
                        format!("交付物 '{}' 的实际文件不存在", deliverable.id),
                    );
                }
            };
            if content.is_empty() {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_empty",
                    CriterionStatus::Fail,
                    vec![evidence.evidence_id.clone()],
                    format!("交付物 '{}' 的实际文件内容为空", deliverable.id),
                );
            }

            let actual_digest = format!("{:x}", Sha256::digest(content));
            if actual_digest != evidence.digest {
                return deliverable_result(
                    deliverable.id.clone(),
                    "artifact_digest_mismatch",
                    CriterionStatus::Fail,
                    vec![evidence.evidence_id.clone()],
                    format!("产物 '{}' 实际摘要与快照证据摘要不符", artifact_id),
                );
            }

            deliverable_result(
                deliverable.id.clone(),
                "verified",
                CriterionStatus::Pass,
                vec![evidence.evidence_id.clone()],
                format!("交付物 '{}' 的实际文件、来源和摘要已核验", deliverable.id),
            )
        })
        .collect()
}

fn deliverable_result(
    deliverable_id: String,
    code: &str,
    status: CriterionStatus,
    evidence_ids: Vec<String>,
    reason: String,
) -> DeliverableResult {
    DeliverableResult {
        deliverable_id,
        status,
        code: code.into(),
        evidence_ids,
        reason,
    }
}

/// Aggregates required criterion statuses into GoalStatus.
/// - Returns Incomplete if required criteria list is empty (no vacuous pass).
/// - Returns Fail if any required criterion failed.
/// - Returns Pass if and only if all required criteria passed.
/// - Returns Incomplete if any required criterion is pending, not checked or stale.
pub fn aggregate_required(statuses: &[CriterionStatus]) -> GoalStatus {
    if statuses.is_empty() {
        return GoalStatus::Incomplete;
    }
    if statuses.iter().any(|s| matches!(s, CriterionStatus::Fail)) {
        return GoalStatus::Fail;
    }
    if statuses.iter().all(|s| matches!(s, CriterionStatus::Pass)) {
        GoalStatus::Pass
    } else {
        GoalStatus::Incomplete
    }
}

fn evaluate_criterion(
    check: &CriterionCheck,
    snapshot: &EvidenceSnapshot,
    evidence_content: &HashMap<String, Vec<u8>>,
    deliverable_results: &[DeliverableResult],
    existing_reviews: &[HumanReview],
    criterion_id: &str,
) -> (CriterionStatus, Vec<String>, String) {
    match check {
        CriterionCheck::ArtifactExists { deliverable_id } => {
            let mapping = match snapshot
                .deliverables
                .iter()
                .find(|d| &d.deliverable_id == deliverable_id)
            {
                Some(m) => m,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("未在证据快照中找到交付物 '{}' 的产物映射", deliverable_id),
                    );
                }
            };

            let content = match evidence_content.get(&mapping.artifact_id) {
                Some(c) => c,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("产物文件 '{}' 不存在", mapping.artifact_id),
                    );
                }
            };

            if content.is_empty() {
                return (
                    CriterionStatus::Fail,
                    Vec::new(),
                    format!("产物文件 '{}' 内容为空", mapping.artifact_id),
                );
            }

            let evidence_ids: Vec<String> = snapshot
                .evidence
                .iter()
                .filter(|e| e.artifact_id.as_deref() == Some(&mapping.artifact_id))
                .map(|e| e.evidence_id.clone())
                .collect();

            (
                CriterionStatus::Pass,
                evidence_ids,
                format!(
                    "产物文件 '{}' 存在且内容有效 (大小: {} 字节)",
                    mapping.artifact_id,
                    content.len()
                ),
            )
        }

        CriterionCheck::ArtifactDigestMatches { deliverable_id } => {
            let mapping = match snapshot
                .deliverables
                .iter()
                .find(|d| &d.deliverable_id == deliverable_id)
            {
                Some(m) => m,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("未在证据快照中找到交付物 '{}' 的产物映射", deliverable_id),
                    );
                }
            };

            let content = match evidence_content.get(&mapping.artifact_id) {
                Some(c) => c,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("产物文件 '{}' 不存在", mapping.artifact_id),
                    );
                }
            };

            let ev = match snapshot
                .evidence
                .iter()
                .find(|e| e.artifact_id.as_deref() == Some(&mapping.artifact_id))
            {
                Some(e) => e,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("未找到产物 '{}' 的物化证据记录", mapping.artifact_id),
                    );
                }
            };

            let actual_digest = format!("{:x}", Sha256::digest(content));
            if actual_digest == ev.digest {
                (
                    CriterionStatus::Pass,
                    vec![ev.evidence_id.clone()],
                    format!("产物摘要核验一致: {}", actual_digest),
                )
            } else {
                (
                    CriterionStatus::Fail,
                    vec![ev.evidence_id.clone()],
                    format!(
                        "产物实际摘要 '{}' 与记录摘要 '{}' 不符",
                        actual_digest, ev.digest
                    ),
                )
            }
        }

        CriterionCheck::RequiredSections {
            deliverable_id,
            headings,
        } => {
            let mapping = match snapshot
                .deliverables
                .iter()
                .find(|d| &d.deliverable_id == deliverable_id)
            {
                Some(m) => m,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("未在证据快照中找到交付物 '{}' 的产物映射", deliverable_id),
                    );
                }
            };

            let content = match evidence_content.get(&mapping.artifact_id) {
                Some(c) => c,
                None => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!("产物文件 '{}' 不存在", mapping.artifact_id),
                    );
                }
            };

            let text = match std::str::from_utf8(content) {
                Ok(t) => t,
                Err(_) => {
                    return (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!(
                            "产物 '{}' 不是有效的 UTF-8 Markdown 文本",
                            mapping.artifact_id
                        ),
                    );
                }
            };

            let parsed_headings = parse_markdown_headings(text);
            let mut missing = Vec::new();

            for req in headings {
                let req_norm = normalize_heading(req);
                if !parsed_headings
                    .iter()
                    .any(|h| normalize_heading(h) == req_norm)
                {
                    missing.push(req.clone());
                }
            }

            let evidence_ids: Vec<String> = snapshot
                .evidence
                .iter()
                .filter(|e| e.artifact_id.as_deref() == Some(&mapping.artifact_id))
                .map(|e| e.evidence_id.clone())
                .collect();

            if missing.is_empty() {
                (
                    CriterionStatus::Pass,
                    evidence_ids,
                    format!("已包含全部要求章节 ({:?})", headings),
                )
            } else {
                (
                    CriterionStatus::Fail,
                    evidence_ids,
                    format!("缺少必要章节: {:?}", missing),
                )
            }
        }

        CriterionCheck::SourceTraceability {
            deliverable_id,
            min_sources,
        } => {
            let matching_sources: Vec<_> = snapshot
                .sources
                .iter()
                .filter(|s| {
                    s.cited_by_deliverable_ids.contains(deliverable_id)
                        && !s.uri.trim().is_empty()
                        && s.retrieved_at_ms > 0
                        && !s.evidence_id.is_empty()
                })
                .collect();

            let evidence_ids: Vec<String> = matching_sources
                .iter()
                .map(|s| s.evidence_id.clone())
                .collect();

            let count = matching_sources
                .iter()
                .map(|source| source.uri.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len() as u32;
            if count >= *min_sources {
                (
                    CriterionStatus::Pass,
                    evidence_ids,
                    format!("可追溯来源数量 {} 达到要求 (至少 {})", count, min_sources),
                )
            } else {
                (
                    CriterionStatus::Fail,
                    evidence_ids,
                    format!("可追溯来源数量 {} 不足 (要求至少 {})", count, min_sources),
                )
            }
        }

        CriterionCheck::HumanReview {
            deliverable_id,
            rubric,
        } => {
            let deliverable = deliverable_results
                .iter()
                .find(|result| result.deliverable_id == *deliverable_id);
            if !deliverable.is_some_and(|result| result.status == CriterionStatus::Pass) {
                let reason = deliverable.map_or_else(
                    || format!("人工复核引用了未声明的交付物 '{}'", deliverable_id),
                    |result| format!("人工复核不能替代交付物基础检查: {}", result.reason),
                );
                return (
                    CriterionStatus::Fail,
                    deliverable
                        .map(|result| result.evidence_ids.clone())
                        .unwrap_or_default(),
                    reason,
                );
            }
            if let Some(review) = existing_reviews
                .iter()
                .find(|r| r.criterion_id == criterion_id)
            {
                if review.decision.eq_ignore_ascii_case("pass") {
                    (
                        CriterionStatus::Pass,
                        Vec::new(),
                        format!(
                            "人工复核通过: {} (评审人: {})",
                            review.reason, review.reviewer_subject
                        ),
                    )
                } else {
                    (
                        CriterionStatus::Fail,
                        Vec::new(),
                        format!(
                            "人工复核不通过: {} (评审人: {})",
                            review.reason, review.reviewer_subject
                        ),
                    )
                }
            } else {
                (
                    CriterionStatus::HumanPending,
                    Vec::new(),
                    format!("等待人工质量复核: {}", rubric),
                )
            }
        }
    }
}

pub fn parse_markdown_headings(content: &str) -> Vec<String> {
    let mut headings = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading_text = trimmed.trim_start_matches('#').trim();
            if !heading_text.is_empty() {
                headings.push(heading_text.to_string());
            }
        }
    }
    headings
}

fn normalize_heading(s: &str) -> String {
    s.trim().to_lowercase().replace(' ', "")
}
