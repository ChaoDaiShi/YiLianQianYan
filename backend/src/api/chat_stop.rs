//! Conversation-only stop settlement. The existing turn ID is the cancellation fence.
use crate::{
    app::state::AppServer,
    db::{Database, MessageRow},
    safety::approval::{ApprovalOwner, PendingApproval},
};
use rusqlite::{params, OptionalExtension};
use std::sync::Arc;

pub(super) fn pending(server: &AppServer, conversation: &str) -> Vec<PendingApproval> {
    server.approval_store.list_pending_for_conversation(conversation).into_iter().filter(|a| matches!(a.owner(), Ok(ApprovalOwner::Conversation { conversation_id }) if conversation_id == conversation)).collect()
}
pub(super) fn latest(db: &Database, conversation: &str) -> Result<Option<String>, String> {
    db.conn().query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1", [conversation], |r| r.get(0)).optional().map_err(|e|e.to_string())
}
pub(super) fn cancelled(db: &Database, conversation: &str) -> Result<bool, String> {
    Ok(
        db.get_conversation(conversation)?.run_status
            == crate::db::ConversationRunStatus::Cancelled,
    )
}
/// CAS the captured turn only. Never change a newer turn accepted after stop.
fn cancel_turn(db: &Database, conversation: &str, turn: &str) -> Result<bool, String> {
    let mut conn = db.conn();
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp_millis();
    let changed=tx.execute("UPDATE conversation_turns SET status='cancelled',finished_at=?3 WHERE id=?2 AND conversation_id=?1 AND finished_at IS NULL AND id=(SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1)",params![conversation,turn,now]).map_err(|e|e.to_string())?;
    if changed > 0 {
        tx.execute("UPDATE conversations SET run_status='cancelled',run_error=NULL,run_finished_at=?2,updated_at=?2 WHERE id=?1",params![conversation,now]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed > 0)
}
/// Store transition happens first: persistence errors must never restore authority.
pub(super) fn cancel_approval(
    server: &AppServer,
    approval: &PendingApproval,
) -> Result<bool, String> {
    if !matches!(approval.owner(), Ok(ApprovalOwner::Conversation { .. })) {
        return Ok(false);
    }
    let a = match server
        .approval_store
        .cancel(&approval.approval_id, &approval.conversation_id)
    {
        Ok(a) => a,
        Err(
            crate::safety::approval::ApprovalError::AlreadyProcessed
            | crate::safety::approval::ApprovalError::Cancelled,
        ) => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    super::approvals::record_approval_resolved(server, &a);
    server.db.add_message(&MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: a.conversation_id.clone(),
        role: "tool".into(),
        content: "Approval cancelled; tool was not executed.".into(),
        tool_calls: None,
        tool_call_id: Some(a.tool_call_id.clone()),
        tool_name: Some(a.tool_name.clone()),
        tool_result: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    })?;
    let mut event =
        crate::agent::engine::AgentEvent::error_event(&a.conversation_id, String::new());
    event.event_type = "approval_resolved".into();
    event.error = None;
    event.tool_call_id = Some(a.tool_call_id);
    event.tool_name = Some(a.tool_name);
    event.approval_id = Some(a.approval_id);
    event.status = Some("cancelled".into());
    server.db.apply_execution_event(&event)?;
    Ok(true)
}
/// Only call before the approved original tool has been dispatched. Existing
/// results (including unknown effects) are immutable and must never be replaced.
pub(super) fn record_not_executed(db: &Database, approval: &PendingApproval) -> Result<(), String> {
    let content = serde_json::json!({"state":"not_executed","reason":"cancelled_before_dispatch","tool_call_id":approval.tool_call_id,"message":"User stopped the approved continuation before this tool was dispatched."}).to_string();
    db.conn().execute(
        "INSERT INTO messages(id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at) SELECT ?1,?2,'tool',?3,NULL,?4,?5,?3,?6 WHERE NOT EXISTS(SELECT 1 FROM messages WHERE conversation_id=?2 AND role='tool' AND tool_call_id=?4)",
        params![uuid::Uuid::new_v4().to_string(),approval.conversation_id,content,approval.tool_call_id,approval.tool_name,chrono::Utc::now().timestamp_millis()],
    ).map_err(|error|error.to_string())?;
    Ok(())
}

fn settle(
    server: &AppServer,
    conversation: &str,
    approvals: Vec<PendingApproval>,
    turn: Option<&str>,
) -> Result<bool, String> {
    let mut changed = false;
    let mut failure = None;
    for a in approvals {
        match cancel_approval(server, &a) {
            Ok(done) => changed |= done,
            Err(e) => {
                failure.get_or_insert(e);
            }
        }
    }
    if let Some(turn) = turn {
        match cancel_turn(&server.db, conversation, turn) {
            Ok(done) => changed |= done,
            Err(e) => {
                failure.get_or_insert(e);
            }
        }
    }
    if let Some(e) = failure {
        Err(e)
    } else {
        Ok(changed)
    }
}
pub(super) async fn stop(server: Arc<AppServer>, conversation: &str) -> Result<bool, String> {
    // Same registry lock as registered-turn acquisition: snapshot belongs to the old turn.
    let (active, guard, turn, turn_changed, approvals) = {
        let active = server.active_tasks.lock();
        let token = active.get(conversation).cloned();
        let guard = if token.is_none() {
            super::chat::lifecycle::try_acquire_turn(&server, conversation).ok()
        } else {
            None
        };
        if let Some(token) = token.as_ref() {
            token.cancel();
        }
        let approvals = pending(&server, conversation);
        // A broken database cannot prevent cancellation of known runtime authority.
        // Preserve the read error, while still revoking the captured approvals below.
        let (turn, turn_changed) = match latest(&server.db, conversation) {
            Ok(turn) => {
                // Publish the stop fence before another continuation can publish a token.
                let changed = match turn.as_deref() {
                    Some(turn) => cancel_turn(&server.db, conversation, turn),
                    None => Ok(false),
                };
                (turn, changed)
            }
            Err(error) => (None, Err(error)),
        };
        (token.is_some(), guard, turn, turn_changed, approvals)
    };
    // Revoke in-memory authority even if the durable turn update failed.
    let settled = settle(&server, conversation, approvals, None);
    let changed = turn_changed? | settled?;
    if let Some(_guard) = guard {
        return Ok(server.db.cancel_conversation_inputs(conversation)? || changed || active);
    }
    // Do not wait for provider/host IO to finish before acknowledging stop. The durable
    // cancelled turn blocks late approval; drain approvals created during cancellation.
    let conversation = conversation.to_owned();
    tokio::spawn(async move {
        let _guard = super::chat::lifecycle::acquire_turn(&server, &conversation).await;
        if latest(&server.db, &conversation).ok().flatten() == turn {
            if let Err(error) = settle(
                &server,
                &conversation,
                pending(&server, &conversation),
                turn.as_deref(),
            ) {
                tracing::error!(%error,"stopped conversation approval settlement failed");
            }
            if let Err(error) = server.db.cancel_conversation_inputs(&conversation) {
                tracing::error!(%error,"stopped conversation input settlement failed");
            }
        }
    });
    Ok(changed || active)
}

#[cfg(test)]
#[path = "chat_stop_tests.rs"]
mod tests;
