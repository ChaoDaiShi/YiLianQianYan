//! Project existing conversation run summaries into the shared goal view.
use crate::db::Database;
use crate::modules::task::proofloop::ProofError;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) fn project(
    db: &Database,
    conversation_ids: &[String],
) -> Result<Vec<Value>, ProofError> {
    if conversation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = conversation_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    db.list_conversations()
        .map(|conversations| {
            conversations
                .into_iter()
                .filter(|conversation| ids.contains(conversation.id.as_str()))
                .map(|conversation| {
                    json!({
                        "conversation_id": conversation.id,
                        "title": conversation.title,
                        "run_status": conversation.run_status,
                        "run_error": conversation.run_error,
                        "run_started_at": conversation.run_started_at,
                        "run_finished_at": conversation.run_finished_at,
                    })
                })
                .collect()
        })
        .map_err(|error| ProofError::new("db_error", error))
}
