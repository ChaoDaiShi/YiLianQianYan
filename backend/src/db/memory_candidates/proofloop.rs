use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::modules::task::proofloop::{
    AcceptanceReport, CriterionStatus, EvidenceSnapshot, GoalContract, GoalStatus,
};
use crate::utils::text::truncate_chars;

use super::super::{memory_governance::content_hash, migrations::table_exists};

pub(in crate::db) fn stage_verified_report(
    tx: &Transaction<'_>,
    report: &AcceptanceReport,
    evidence: &EvidenceSnapshot,
    now: i64,
) -> rusqlite::Result<()> {
    if report.status != GoalStatus::Pass
        || report.goal_id != evidence.goal_id
        || report.goal_version != evidence.goal_version
        || report.snapshot_id != evidence.snapshot_id
        || report.deliverable_results.is_empty()
    {
        return Ok(());
    }
    let contract_json: Option<String> = tx.query_row(
        "SELECT v.contract_json FROM proofloop_goal_versions v WHERE v.goal_id=?1 AND v.version=?2
         AND EXISTS(SELECT 1 FROM proofloop_drafts d WHERE d.goal_id=v.goal_id AND d.goal_version=v.version AND d.graph_id=?3 AND d.status='confirmed')",
        params![report.goal_id, report.goal_version, report.graph_id], |row| row.get(0),
    ).optional()?;
    let Some(contract) = contract_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<GoalContract>(json).ok())
    else {
        return Ok(());
    };

    // A passing report alone does not give the proposal content. Every listed
    // deliverable must cite its selected artifact evidence from the snapshot.
    let mut verified = Vec::new();
    for deliverable in &contract.deliverables {
        let result = report.deliverable_results.iter().find(|item| {
            item.deliverable_id == deliverable.id && item.status == CriterionStatus::Pass
        });
        let Some(result) = result else {
            return Ok(());
        };
        let Some(mapping) = evidence
            .deliverables
            .iter()
            .find(|item| item.deliverable_id == deliverable.id)
        else {
            return Ok(());
        };
        let Some(artifact) = evidence.evidence.iter().find(|item| {
            item.artifact_id.as_deref() == Some(mapping.artifact_id.as_str())
                && result.evidence_ids.contains(&item.evidence_id)
                && evidence.executions.contains(&item.origin)
                && !item.digest.is_empty()
        }) else {
            return Ok(());
        };
        verified.push((
            deliverable.id.as_str(),
            artifact.origin.execution_id.as_str(),
            artifact.digest.as_str(),
        ));
    }
    if verified.is_empty()
        || !contract.criteria.iter().any(|item| item.required)
        || contract
            .criteria
            .iter()
            .filter(|item| item.required)
            .any(|item| {
                !report.results.iter().any(|result| {
                    result.criterion_id == item.id && result.status == CriterionStatus::Pass
                })
            })
    {
        return Ok(());
    }
    verified.sort_unstable();
    let titles = contract
        .deliverables
        .iter()
        .take(8)
        .map(|item| truncate_chars(&item.title, 100))
        .collect::<Vec<_>>()
        .join("、");
    let content = format!(
        "任务「{}」通过验收。已验证交付物（{} 项）：{}{}。",
        truncate_chars(&contract.user_goal, 280),
        verified.len(),
        titles,
        if verified.len() > 8 { "等" } else { "" }
    );
    if crate::safety::contains_sensitive_content(&content) {
        return Ok(());
    }
    let fingerprint = content_hash(
        &serde_json::json!([
            "proofloop_acceptance",
            report.graph_id,
            report.goal_id,
            report.goal_version,
            report.graph_revision,
            verified,
        ])
        .to_string(),
    );
    tx.execute("INSERT INTO memory_candidates(id,category,content,fingerprint,workspace_id,created_at,graph_id,report_id)
         VALUES(?1,'note',?2,?3,?4,?5,?6,?7) ON CONFLICT(fingerprint) DO NOTHING",
         params![uuid::Uuid::new_v4().to_string(), content, fingerprint, contract.workspace_id, now, report.graph_id, report.report_id])?;
    Ok(())
}

pub(in crate::db) fn live_report_exists(
    conn: &Connection,
    graph: &str,
    report: &str,
) -> rusqlite::Result<bool> {
    // Standalone databases need not have the Task World tables installed.
    if !table_exists(conn, "task_world_supervisor_snapshots")?
        || !table_exists(conn, "proofloop_acceptance_reports")?
    {
        return Ok(false);
    }
    conn.query_row("SELECT EXISTS(SELECT 1 FROM proofloop_acceptance_reports r
        JOIN task_world_supervisor_snapshots g ON g.graph_id=r.graph_id AND g.graph_revision=r.graph_revision
        WHERE r.graph_id=?1 AND r.report_id=?2 AND json_extract(r.report_json,'$.status')='pass')", params![graph, report], |row| row.get(0))
}
