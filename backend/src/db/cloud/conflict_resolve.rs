use crate::modules::cloud::contracts::{
    ConflictRecord, PendingChange, PushResult, PushStatus, RemoteRecord, ResolutionChoice,
    ResolveConflictRequest, ResolveConflictResult, SyncError, ValidatedResolution,
};
use crate::modules::cloud::domain::SyncOperation;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use super::conflicts::get_conflict_tx;

pub fn validate_resolution(
    conflict: &ConflictRecord,
    request: &ResolveConflictRequest,
    current_local_gen: Option<i64>,
) -> Result<ValidatedResolution, SyncError> {
    if conflict.status != "open" {
        if conflict.resolution_request_id.as_deref() == Some(&request.request_id) {
            return Ok(ValidatedResolution {
                conflict: conflict.clone(),
                choice: request.choice,
            });
        }
        return Err(SyncError::ConflictChanged(format!(
            "conflict {} is not open (current status: {})",
            conflict.id, conflict.status
        )));
    }

    if request.expected_conflict_version != conflict.conflict_version {
        return Err(SyncError::ConflictChanged(format!(
            "conflict version mismatch: expected {}, got {}",
            request.expected_conflict_version, conflict.conflict_version
        )));
    }

    if request.expected_remote_revision != conflict.remote_revision {
        return Err(SyncError::ConflictChanged(format!(
            "remote revision mismatch: expected {}, got {}",
            request.expected_remote_revision, conflict.remote_revision
        )));
    }

    if let Some(current_gen) = current_local_gen {
        if current_gen > conflict.local_generation {
            return Err(SyncError::ConflictChanged(format!(
                "local entity was edited since conflict recorded (current local_generation {} > recorded {})",
                current_gen, conflict.local_generation
            )));
        }
    }

    match request.choice {
        ResolutionChoice::UseRemote => {
            if conflict.remote_operation == SyncOperation::Upsert {
                match &conflict.remote_payload {
                    None | Some(Value::Null) => {
                        return Err(SyncError::RemotePayloadMissing(
                            "remote payload is missing or null for upsert operation".into(),
                        ));
                    }
                    Some(val) if !val.is_object() => {
                        return Err(SyncError::RemotePayloadMissing(
                            "remote payload is not an object".into(),
                        ));
                    }
                    _ => {}
                }
            }
        }
        ResolutionChoice::KeepLocal => {
            if conflict.remote_operation == SyncOperation::Delete
                && conflict.local_operation == SyncOperation::Upsert
            {
                return Err(SyncError::RestoreRequiresNewEntity(
                    "cannot restore deleted remote entity with KeepLocal; use KeepLocalAsCopy"
                        .into(),
                ));
            }
            if conflict.local_operation == SyncOperation::Upsert {
                match &conflict.local_payload {
                    None | Some(Value::Null) => {
                        return Err(SyncError::PayloadInvalid(
                            "local payload is missing or null for upsert KeepLocal".into(),
                        ));
                    }
                    _ => {}
                }
            }
        }
        ResolutionChoice::KeepLocalAsCopy => {
            if conflict.local_operation == SyncOperation::Upsert {
                match &conflict.local_payload {
                    None | Some(Value::Null) => {
                        return Err(SyncError::PayloadInvalid(
                            "local payload is missing or null for KeepLocalAsCopy".into(),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(ValidatedResolution {
        conflict: conflict.clone(),
        choice: request.choice,
    })
}

pub fn build_resolution_change(
    local: &PendingChange,
    remote: &RemoteRecord,
    new_change_id: String,
) -> Result<PendingChange, SyncError> {
    if remote.operation == SyncOperation::Delete && local.operation == SyncOperation::Upsert {
        return Err(SyncError::RestoreRequiresNewEntity(
            "cannot restore deleted remote entity with KeepLocal".into(),
        ));
    }
    let next = remote
        .revision
        .checked_add(1)
        .ok_or_else(|| SyncError::InvalidRevision("revision overflow".into()))?;
    if next > 9_007_199_254_740_991 {
        return Err(SyncError::InvalidRevision(
            "revision exceeds max safe integer".into(),
        ));
    }
    Ok(PendingChange {
        change_id: new_change_id,
        entity_type: local.entity_type.clone(),
        entity_id: local.entity_id.clone(),
        base_revision: remote.revision,
        revision: next,
        operation: local.operation,
        payload: if local.operation == SyncOperation::Delete {
            None
        } else {
            local.payload.clone()
        },
        device_id: local.device_id.clone(),
        changed_at: chrono::Utc::now().timestamp_millis(),
        local_generation: local.local_generation,
    })
}

pub fn clone_entity_with_references_tx(
    tx: &rusqlite::Transaction<'_>,
    entity_type: &str,
    old_id: &str,
    new_id: &str,
    payload: Option<&Value>,
) -> Result<String, SyncError> {
    let now = chrono::Utc::now().timestamp_millis();
    match entity_type {
        "conversation" => {
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT title, run_status FROM conversations WHERE id=?1",
                    [old_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;

            if let Some((title, run_status)) = row {
                let copy_title = format!("{title} (副本)");
                tx.execute(
                    "INSERT INTO conversations (id, title, run_status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![new_id, copy_title, run_status, now],
                )?;

                // Clone associated messages
                let mut stmt = tx.prepare(
                    "SELECT role, content, tool_calls_json, tool_call_id, message_status, token_count, created_at
                     FROM messages WHERE conversation_id=?1 ORDER BY created_at",
                )?;
                let messages: Vec<(
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    String,
                    Option<i64>,
                    i64,
                )> = stmt
                    .query_map([old_id], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    })?
                    .filter_map(Result::ok)
                    .collect();

                for (role, content, tc, tid, ms, tc_cnt, msg_created) in messages {
                    let msg_id = uuid::Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO messages (id, conversation_id, role, content, tool_calls_json, tool_call_id, message_status, token_count, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![msg_id, new_id, role, content, tc, tid, ms, tc_cnt, msg_created],
                    )?;
                }
            } else if let Some(payload) = payload {
                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                let copy_title = format!("{title} (副本)");
                let run_status = payload
                    .get("run_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("idle");
                tx.execute(
                    "INSERT INTO conversations (id, title, run_status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![new_id, copy_title, run_status, now],
                )?;
            }
        }
        "memory" => {
            let row: Option<(String, String, String, i64, i64)> = tx
                .query_row(
                    "SELECT content, category, source, created_at, updated_at FROM memories WHERE id=?1",
                    [old_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;

            if let Some((content, cat, src, cr, up)) = row {
                tx.execute(
                    "INSERT INTO memories (id, content, category, source, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![new_id, content, cat, src, cr, up],
                )?;
            } else if let Some(payload) = payload {
                let content = payload
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cat = payload
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general");
                let src = payload
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                tx.execute(
                    "INSERT INTO memories (id, content, category, source, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![new_id, content, cat, src, now, now],
                )?;
            }
        }
        "message" => {
            let row: Option<(String, String, String, Option<String>, Option<String>, String, Option<i64>, i64)> = tx
                .query_row(
                    "SELECT conversation_id, role, content, tool_calls_json, tool_call_id, message_status, token_count, created_at
                     FROM messages WHERE id=?1",
                    [old_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
                )
                .optional()?;
            if let Some((conv_id, role, content, tc, tid, ms, tc_cnt, msg_created)) = row {
                tx.execute(
                    "INSERT INTO messages (id, conversation_id, role, content, tool_calls_json, tool_call_id, message_status, token_count, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![new_id, conv_id, role, content, tc, tid, ms, tc_cnt, msg_created],
                )?;
            }
        }
        _ => {
            if let Some(payload) = payload {
                super::projection::apply_remote_entity(
                    tx,
                    entity_type,
                    new_id,
                    SyncOperation::Upsert,
                    Some(payload),
                    now,
                )?;
            }
        }
    }

    tx.execute(
        "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
         VALUES (?1, ?2, '', 1, 0, 1, 0, ?3)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
            revision=1, local_generation=local_generation+1, deleted=0, updated_at=excluded.updated_at",
        params![entity_type, new_id, now],
    )?;

    Ok(new_id.to_string())
}

pub fn resolve_conflict_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &ResolveConflictRequest,
    device_id: &str,
) -> Result<ResolveConflictResult, SyncError> {
    let conflict = get_conflict_tx(tx, &request.conflict_id)?;

    if conflict.status != "open"
        && conflict.resolution_request_id.as_deref() == Some(&request.request_id)
    {
        return Ok(ResolveConflictResult {
            conflict_id: conflict.id,
            state: conflict.status,
            queued_change_id: conflict.queued_change_id,
            copied_entity_id: conflict.copied_entity_id,
        });
    }

    let current_local_gen: Option<i64> = tx
        .query_row(
            "SELECT local_generation FROM cloud_entity_revisions WHERE entity_type=?1 AND entity_id=?2",
            params![&conflict.entity_type, &conflict.entity_id],
            |row| row.get(0),
        )
        .optional()?;

    let validated = validate_resolution(&conflict, request, current_local_gen)?;
    let now = chrono::Utc::now().timestamp_millis();

    match validated.choice {
        ResolutionChoice::UseRemote => {
            let remote_rec = RemoteRecord {
                change_id: conflict.remote_change_id.clone().unwrap_or_default(),
                entity_type: conflict.entity_type.clone(),
                entity_id: conflict.entity_id.clone(),
                revision: conflict.remote_revision,
                operation: conflict.remote_operation,
                payload: conflict.remote_payload.clone(),
                sequence: conflict
                    .remote_sequence
                    .clone()
                    .unwrap_or_else(|| "0".into()),
                device_id: String::new(),
                changed_at: now,
            };

            super::apply::apply_selected_remote_tx(tx, &remote_rec, conflict.local_generation)?;

            tx.execute(
                "DELETE FROM cloud_sync_outbox WHERE entity_type=?1 AND entity_id=?2
                 AND state IN ('pending','in_flight','retry_wait','conflicted')",
                params![&conflict.entity_type, &conflict.entity_id],
            )?;

            tx.execute(
                "UPDATE cloud_sync_conflicts SET
                    status='resolved', resolution_request_id=?2, resolved_at=?3
                 WHERE id=?1",
                params![&conflict.id, &request.request_id, now],
            )?;

            Ok(ResolveConflictResult {
                conflict_id: conflict.id,
                state: "resolved".to_string(),
                queued_change_id: None,
                copied_entity_id: None,
            })
        }
        ResolutionChoice::KeepLocal => {
            let local_pending = PendingChange {
                change_id: uuid::Uuid::new_v4().to_string(),
                entity_type: conflict.entity_type.clone(),
                entity_id: conflict.entity_id.clone(),
                base_revision: conflict.local_revision,
                revision: conflict.local_revision,
                operation: conflict.local_operation,
                payload: conflict.local_payload.clone(),
                device_id: device_id.to_string(),
                changed_at: now,
                local_generation: conflict.local_generation,
            };

            let remote_rec = RemoteRecord {
                change_id: conflict.remote_change_id.clone().unwrap_or_default(),
                entity_type: conflict.entity_type.clone(),
                entity_id: conflict.entity_id.clone(),
                revision: conflict.remote_revision,
                operation: conflict.remote_operation,
                payload: conflict.remote_payload.clone(),
                sequence: conflict.remote_sequence.clone().unwrap_or_default(),
                device_id: String::new(),
                changed_at: now,
            };

            let new_change_id = uuid::Uuid::new_v4().to_string();
            let change =
                build_resolution_change(&local_pending, &remote_rec, new_change_id.clone())?;

            tx.execute(
                "DELETE FROM cloud_sync_outbox WHERE entity_type=?1 AND entity_id=?2",
                params![&conflict.entity_type, &conflict.entity_id],
            )?;

            let payload_json_str = change
                .payload
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            tx.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id, entity_type, entity_id, base_revision, revision,
                    operation, payload_json, device_id, state, attempts,
                    local_generation, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', 0, ?9, ?10, ?10)",
                params![
                    &change.change_id,
                    &change.entity_type,
                    &change.entity_id,
                    change.base_revision,
                    change.revision,
                    change.operation.as_str(),
                    payload_json_str,
                    &change.device_id,
                    change.local_generation,
                    now,
                ],
            )?;

            tx.execute(
                "UPDATE cloud_sync_conflicts SET
                    status='resolving', resolution_request_id=?2, queued_change_id=?3
                 WHERE id=?1",
                params![&conflict.id, &request.request_id, &new_change_id],
            )?;

            Ok(ResolveConflictResult {
                conflict_id: conflict.id,
                state: "resolving".to_string(),
                queued_change_id: Some(new_change_id),
                copied_entity_id: None,
            })
        }
        ResolutionChoice::KeepLocalAsCopy => {
            let new_entity_id = uuid::Uuid::new_v4().to_string();

            clone_entity_with_references_tx(
                tx,
                &conflict.entity_type,
                &conflict.entity_id,
                &new_entity_id,
                conflict.local_payload.as_ref(),
            )?;

            let mut new_payload = conflict.local_payload.clone().unwrap_or(Value::Null);
            if let Value::Object(ref mut map) = new_payload {
                map.insert("id".to_string(), Value::String(new_entity_id.clone()));
            }

            let new_change_id = uuid::Uuid::new_v4().to_string();
            let new_payload_str = serde_json::to_string(&new_payload).unwrap_or_default();

            tx.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id, entity_type, entity_id, base_revision, revision,
                    operation, payload_json, device_id, state, attempts,
                    local_generation, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, 0, 1, 'upsert', ?4, ?5, 'pending', 0, 1, ?6, ?6)",
                params![
                    &new_change_id,
                    &conflict.entity_type,
                    &new_entity_id,
                    &new_payload_str,
                    device_id,
                    now,
                ],
            )?;

            let remote_rec = RemoteRecord {
                change_id: conflict.remote_change_id.clone().unwrap_or_default(),
                entity_type: conflict.entity_type.clone(),
                entity_id: conflict.entity_id.clone(),
                revision: conflict.remote_revision,
                operation: conflict.remote_operation,
                payload: conflict.remote_payload.clone(),
                sequence: conflict
                    .remote_sequence
                    .clone()
                    .unwrap_or_else(|| "0".into()),
                device_id: String::new(),
                changed_at: now,
            };

            super::apply::apply_selected_remote_tx(tx, &remote_rec, conflict.local_generation)?;

            tx.execute(
                "DELETE FROM cloud_sync_outbox WHERE entity_type=?1 AND entity_id=?2
                 AND state IN ('pending','in_flight','retry_wait','conflicted')",
                params![&conflict.entity_type, &conflict.entity_id],
            )?;

            tx.execute(
                "UPDATE cloud_sync_conflicts SET
                    status='resolved', resolution_request_id=?2, queued_change_id=?3,
                    copied_entity_id=?4, resolved_at=?5
                 WHERE id=?1",
                params![
                    &conflict.id,
                    &request.request_id,
                    &new_change_id,
                    &new_entity_id,
                    now
                ],
            )?;

            Ok(ResolveConflictResult {
                conflict_id: conflict.id,
                state: "resolved".to_string(),
                queued_change_id: Some(new_change_id),
                copied_entity_id: Some(new_entity_id),
            })
        }
    }
}

pub fn finalize_resolution_ack_tx(
    tx: &rusqlite::Transaction<'_>,
    change_id: &str,
    result: &PushResult,
) -> Result<(), SyncError> {
    let conflict_row: Option<(String, i64)> = tx
        .query_row(
            "SELECT id, conflict_version FROM cloud_sync_conflicts WHERE queued_change_id=?1",
            [change_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let Some((conflict_id, current_version)) = conflict_row else {
        return Ok(());
    };

    let now = chrono::Utc::now().timestamp_millis();
    match result.status {
        PushStatus::Applied | PushStatus::Duplicate => {
            tx.execute(
                "UPDATE cloud_sync_conflicts SET status='resolved', resolved_at=?2 WHERE id=?1",
                params![&conflict_id, now],
            )?;
        }
        PushStatus::Conflict => {
            let authoritative_rev = result
                .authoritative_revision
                .or_else(|| result.authoritative.as_ref().map(|a| a.revision))
                .unwrap_or(0);
            let authoritative_payload = result
                .authoritative
                .as_ref()
                .and_then(|a| a.payload.as_ref())
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            let authoritative_op = result
                .authoritative
                .as_ref()
                .map(|a| a.operation.as_str())
                .unwrap_or("upsert");
            let authoritative_change_id =
                result.authoritative.as_ref().map(|a| a.change_id.as_str());
            let authoritative_seq = result.authoritative.as_ref().map(|a| a.sequence.as_str());

            tx.execute(
                "UPDATE cloud_sync_conflicts SET
                    status='open', remote_revision=?2, remote_operation=?3,
                    remote_payload_json=?4, remote_change_id=?5, remote_sequence=?6,
                    conflict_version=?7, queued_change_id=NULL, resolution_request_id=NULL
                 WHERE id=?1",
                params![
                    &conflict_id,
                    authoritative_rev,
                    authoritative_op,
                    authoritative_payload,
                    authoritative_change_id,
                    authoritative_seq,
                    current_version + 1,
                ],
            )?;
        }
        PushStatus::Rejected => {
            tx.execute(
                "UPDATE cloud_sync_conflicts SET status='open', queued_change_id=NULL WHERE id=?1",
                [&conflict_id],
            )?;
        }
    }

    Ok(())
}
