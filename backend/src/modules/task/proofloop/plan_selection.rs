//! Reproducible plan scoring and recommendation selection.

use std::collections::{HashMap, HashSet};

use super::contracts::{
    CandidatePlan, CapabilitySnapshot, CompileResult, CompileStatus, CriterionCheck, GoalContract,
};
use super::enhancement_contracts::{PlanOption, PlanScore};

pub fn weighted_total(c: u16, t: u16, e: u16, r: u16, k: u16) -> i64 {
    30 * i64::from(c) + 20 * i64::from(t) + 25 * i64::from(e)
        - 15 * i64::from(r)
        - 10 * i64::from(k)
}

pub fn score_candidate(
    contract: &GoalContract,
    candidate: &CandidatePlan,
    compilation: &CompileResult,
    snapshot: &CapabilitySnapshot,
) -> PlanScore {
    let node_ids: HashSet<&str> = candidate
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    let deliverable_ids: HashSet<&str> = contract
        .deliverables
        .iter()
        .map(|deliverable| deliverable.id.as_str())
        .collect();
    let mut mapped_deliverables = HashSet::new();
    let mut rejected_codes = Vec::new();

    for mapping in &candidate.deliverable_nodes {
        if deliverable_ids.contains(mapping.deliverable_id.as_str())
            && node_ids.contains(mapping.node_id.as_str())
        {
            mapped_deliverables.insert(mapping.deliverable_id.as_str());
        }
    }
    let coverage_bp = ratio_bp(mapped_deliverables.len(), contract.deliverables.len());
    if mapped_deliverables.len() != contract.deliverables.len() {
        rejected_codes.push("proofloop_goal_uncovered".into());
    }

    for diagnostic in &compilation.diagnostics {
        rejected_codes.push(diagnostic.code.clone());
    }
    if compilation.status != CompileStatus::Ready && compilation.diagnostics.is_empty() {
        rejected_codes.push("proofloop_compile_not_ready".into());
    }
    let type_bp = if compilation.status == CompileStatus::Ready {
        10_000
    } else {
        0
    };

    let mut snapshot_by_id = HashMap::new();
    for capability in &snapshot.capabilities {
        snapshot_by_id.insert(capability.capability_id.as_str(), capability);
    }
    let allowed_ids: HashSet<&str> = contract
        .allowed_capability_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut risk_bp = 0;
    for node in &candidate.nodes {
        if !allowed_ids.contains(node.capability_id.as_str()) {
            rejected_codes.push("proofloop_capability_not_allowed".into());
        }
        match snapshot_by_id.get(node.capability_id.as_str()) {
            Some(capability) if capability.readiness == "ready" => {
                risk_bp = risk_bp.max(capability_risk_bp(&capability.risk));
            }
            _ => rejected_codes.push("proofloop_capability_unavailable".into()),
        }
    }

    let required_criteria: Vec<_> = contract
        .criteria
        .iter()
        .filter(|criterion| criterion.required)
        .collect();
    let verified_criteria = required_criteria
        .iter()
        .filter(|criterion| match &criterion.check {
            CriterionCheck::HumanReview { .. } => false,
            CriterionCheck::ArtifactExists { deliverable_id }
            | CriterionCheck::ArtifactDigestMatches { deliverable_id }
            | CriterionCheck::RequiredSections { deliverable_id, .. }
            | CriterionCheck::SourceTraceability { deliverable_id, .. } => {
                deliverable_ids.contains(deliverable_id.as_str())
                    && mapped_deliverables.contains(deliverable_id.as_str())
            }
        })
        .count();
    let evidence_bp = ratio_bp(verified_criteria, required_criteria.len());
    for criterion in &required_criteria {
        let deliverable_id = match &criterion.check {
            CriterionCheck::ArtifactExists { deliverable_id }
            | CriterionCheck::ArtifactDigestMatches { deliverable_id }
            | CriterionCheck::RequiredSections { deliverable_id, .. }
            | CriterionCheck::SourceTraceability { deliverable_id, .. }
            | CriterionCheck::HumanReview { deliverable_id, .. } => deliverable_id,
        };
        if !deliverable_ids.contains(deliverable_id.as_str()) {
            rejected_codes.push("proofloop_criterion_deliverable_missing".into());
        }
    }

    let cost_bp = ((candidate.nodes.len().min(20) as u64 * 10_000) / 20) as u16;
    rejected_codes.sort();
    rejected_codes.dedup();

    PlanScore {
        coverage_bp,
        type_bp,
        evidence_bp,
        risk_bp,
        cost_bp,
        cost_known: false,
        total: weighted_total(coverage_bp, type_bp, evidence_bp, risk_bp, cost_bp),
        rejected_codes,
    }
}

pub fn select_recommended(options: &[PlanOption]) -> Option<String> {
    let mut eligible: Vec<_> = options
        .iter()
        .filter(|option| {
            option.compilation.status == CompileStatus::Ready
                && option.score.rejected_codes.is_empty()
        })
        .collect();
    eligible.sort_by(|left, right| {
        right
            .score
            .total
            .cmp(&left.score.total)
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    eligible.first().map(|option| option.candidate_id.clone())
}

fn capability_risk_bp(risk: &str) -> u16 {
    match risk.to_ascii_lowercase().as_str() {
        "low" => 0,
        "medium" => 3_333,
        "high" => 6_667,
        "critical" => 10_000,
        _ => 10_000,
    }
}

fn ratio_bp(numerator: usize, denominator: usize) -> u16 {
    if denominator == 0 {
        0
    } else {
        ((numerator as u64 * 10_000) / denominator as u64) as u16
    }
}
