//! Local projection repair and basic acceptance are owned by the backend.
//! No model, tool executor, channel sender or approval continuation runs here.
use crate::modules::task::proofloop::{collect_evidence, evaluate_goal};
use crate::modules::task::{TaskExecutionControlState, TaskGraphId};
use crate::{
    app::state::AppServer,
    db::{execution_outcomes::ConversationOutcome, ConversationRunStatus},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn status(raw: &str) -> Result<ConversationRunStatus, String> {
    serde_json::from_value(serde_json::Value::String(raw.into()))
        .map_err(|_| "invalid persisted outcome status".into())
}

/// Only materializes an already committed local fact. Callers serialize this
/// against conversation turns; correlation is checked again before projection.
pub fn project_conversation_outcome(
    server: &AppServer,
    receipt: &ConversationOutcome,
) -> Result<ConversationRunStatus, String> {
    if !server.db.conversation_outcome_is_current(receipt)? {
        server
            .db
            .settle_conversation_outcome(receipt.sequence, None)?;
        return Ok(server
            .db
            .find_conversation(&receipt.conversation_id)?
            .map(|conversation| conversation.run_status)
            .unwrap_or(ConversationRunStatus::Interrupted));
    }
    let current = server.db.get_conversation(&receipt.conversation_id)?;
    let mut requested = status(&receipt.status)?;
    let mut error = receipt.error.clone();
    // Cancellation is authoritative, including cancellation after a worker
    // finished but before its projection committed.
    if current.run_status == ConversationRunStatus::Cancelled {
        requested = ConversationRunStatus::Cancelled;
    }
    // A previous process cannot restore an in-memory approval from its receipt.
    if requested == ConversationRunStatus::WaitingApproval
        && current.run_status == ConversationRunStatus::Interrupted
    {
        requested = ConversationRunStatus::Interrupted;
        error = Some("审批等待已中断；未重放动作，请核对执行记录".into());
    }
    let committed = server.task_world.commit_conversation_outcome_for_turn(
        &receipt.conversation_id,
        requested,
        error.as_deref(),
        Some(&receipt.turn_key),
    )?;
    if !server.db.conversation_outcome_is_current(receipt)? {
        server
            .db
            .settle_conversation_outcome(receipt.sequence, None)?;
        return Ok(committed);
    }
    crate::modules::automation::personal::finish_turn(
        server,
        &receipt.conversation_id,
        committed,
        error.as_deref(),
    )?;
    server
        .db
        .settle_conversation_outcome(receipt.sequence, None)?;
    Ok(committed)
}

/// Called before the server and scheduler are published. Every old receipt is
/// visited once in bounded pages, before occurrence recovery can classify it.
pub fn recover_startup_receipts(server: &AppServer) -> Result<(), String> {
    let mut cursor = 0;
    loop {
        let rows = server.db.pending_conversation_outcomes_after(cursor, 32)?;
        if rows.is_empty() {
            break;
        }
        for receipt in rows {
            cursor = receipt.sequence;
            if let Err(error) = project_conversation_outcome(server, &receipt) {
                server.db.settle_conversation_outcome(
                    receipt.sequence,
                    Some(&crate::utils::text::truncate_chars(&error, 1000)),
                )?;
            }
        }
    }
    Ok(())
}

/// Evaluate the latest eligible evidence once. Human criteria remain pending;
/// matching prior human decisions are preserved only for identical evidence.
/// Returns false while work is active/paused, leaving the queue pending.
pub fn evaluate_graph_if_ready(server: &AppServer, graph: &str) -> Result<bool, String> {
    let Some(draft) = server
        .db
        .confirmed_proof_draft_for_graph(graph)
        .map_err(|e| e.to_string())?
    else {
        return Ok(true);
    };
    let graph_id = TaskGraphId::new(graph).map_err(|e| e.to_string())?;
    // Completed graph deletion intentionally preserves proof history. Its
    // explicit tombstone settles queued observation, never a new evaluation.
    if server
        .db
        .task_graph_was_deleted(&graph_id)
        .map_err(|e| e.to_string())?
    {
        return Ok(true);
    }
    let detail = server
        .task_world
        .get_graph_detail(&graph_id)
        .map_err(|e| e.to_string())?;
    if detail.control_state != TaskExecutionControlState::Running {
        return Ok(false);
    };
    let conversations = server
        .db
        .goal_conversation_ids(&draft.contract.goal_id, draft.contract.version)?;
    for id in conversations {
        let c = server.db.get_conversation(&id)?;
        if matches!(
            c.run_status,
            ConversationRunStatus::Running
                | ConversationRunStatus::WaitingApproval
                | ConversationRunStatus::WaitingInput
        ) {
            return Ok(false);
        }
    }
    let executions = detail
        .nodes
        .iter()
        .filter_map(|node| node.latest_execution.as_ref())
        .collect::<Vec<_>>();
    if executions.is_empty()
        || executions.iter().any(|e| {
            !e.status.is_terminal()
                || matches!(
                    e.status,
                    crate::modules::task::NodeExecutionStatus::Cancelled
                        | crate::modules::task::NodeExecutionStatus::Stale
                )
        })
    {
        return Ok(false);
    }
    let ids = executions
        .iter()
        .map(|e| e.execution_id.clone())
        .collect::<Vec<_>>();
    let now = chrono::Utc::now().timestamp_millis();
    let artifact_service =
        crate::modules::task::artifact::ArtifactService::new(server.db.clone_connection());
    let (mut snapshot, contents) = collect_evidence(
        &server.db,
        &artifact_service,
        &draft.contract,
        graph,
        detail.revision,
        &ids,
        &server.workspace_root,
        now,
    )
    .map_err(|e| format!("{}: {}", e.code, e.message))?;
    let reports = server
        .db
        .list_acceptance_reports(&draft.contract.goal_id)
        .map_err(|e| e.to_string())?;
    for report in reports {
        if report.goal_version != snapshot.goal_version
            || report.graph_id != snapshot.graph_id
            || report.graph_revision != snapshot.graph_revision
        {
            continue;
        }
        if let Some((_report, old)) = server
            .db
            .get_acceptance_report(&report.report_id)
            .map_err(|e| e.to_string())?
        {
            if old.executions == snapshot.executions
                && old.evidence == snapshot.evidence
                && old.deliverables == snapshot.deliverables
                && old.sources == snapshot.sources
            {
                return Ok(true);
            }
        }
    }
    snapshot.snapshot_id.clear();
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?)
    );
    snapshot.snapshot_id = format!("auto-snapshot-{digest}");
    let mut report = evaluate_goal(&draft.contract, &snapshot, &contents, &[], now)
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    report.report_id = format!("auto-report-{digest}");
    server
        .db
        .insert_acceptance_report_if_current(&report, &snapshot, now)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// One bounded pass is also the controlled local recovery entry point. It
/// never resumes an engine or sends an external message.
pub async fn reconcile_once(server: &AppServer) -> Result<(), String> {
    if let Err(error) = server.task_world.reconcile_paused_outcomes() {
        tracing::warn!(%error, "paused outcome reconciliation will retry");
    }
    let workflow_error =
        crate::modules::workflow::application::task_parent::reconcile(server).err();
    for receipt in server.db.pending_conversation_outcomes(32)? {
        if server
            .active_tasks
            .lock()
            .contains_key(&receipt.conversation_id)
        {
            server.db.defer_conversation_outcome(receipt.sequence)?;
            continue;
        }
        let Ok(_turn) =
            crate::api::chat::lifecycle::try_acquire_turn(server, &receipt.conversation_id)
        else {
            server.db.defer_conversation_outcome(receipt.sequence)?;
            continue;
        };
        if let Err(error) = project_conversation_outcome(server, &receipt) {
            server.db.settle_conversation_outcome(
                receipt.sequence,
                Some(&crate::utils::text::truncate_chars(&error, 1000)),
            )?;
        }
    }
    for work in server.db.pending_goal_evaluations()? {
        match evaluate_graph_if_ready(server, &work.graph_id) {
            Ok(true) => server.db.settle_goal_evaluation(&work, None)?,
            Ok(false) => server.db.defer_goal_evaluation(&work)?,
            Err(error) => server.db.settle_goal_evaluation(
                &work,
                Some(&crate::utils::text::truncate_chars(&error, 1000)),
            )?,
        }
    }
    if let Some(error) = workflow_error {
        return Err(error);
    }
    Ok(())
}

pub fn start_background(server: &Arc<AppServer>) {
    let weak = Arc::downgrade(server);
    let cancel = server.cloud_revocation_retry_cancel.child_token();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            tokio::select! {_ = cancel.cancelled()=>break, _=interval.tick()=>{
                let Some(server)=weak.upgrade() else {break};
                if let Err(error)=reconcile_once(&server).await {tracing::warn!(%error,"local integration reconciliation deferred");}
            }}
        }
    });
}

#[cfg(test)]
#[path = "reconciliation_tests.rs"]
mod tests;
