use crate::db::Database;
use crate::modules::cloud::contracts::{
    compute_canonical_digest, decide_remote_version, validate_sync_payload, ApplyDisposition,
    ApplyPageReport, PullPage, RemoteRecord, ScopeLease, ScopedDb, SyncError, VersionDecision,
};
use crate::modules::cloud::domain::{CloudSyncStatus, SyncOperation};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn cloud_sync_status(&self) -> Result<CloudSyncStatus, rusqlite::Error> {
        let conn = self.conn();
        let (pending_count, error_count) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN state='error' OR state='retry_wait' THEN 1 ELSE 0 END),0) FROM cloud_sync_outbox",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let conflict_count = conn.query_row(
            "SELECT COUNT(*) FROM cloud_sync_conflicts WHERE status='open'",
            [],
            |row| row.get(0),
        )?;
        let cursor = conn
            .query_row(
                "SELECT download_cursor, last_success_at, last_error_code FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (cursor, last_success_at, last_error_code) = cursor.unwrap_or((None, None, None));
        Ok(CloudSyncStatus {
            pending_count,
            conflict_count,
            error_count,
            cursor,
            last_success_at,
            last_error_code,
        })
    }

    pub fn apply_remote_sync_change(
        &self,
        entity_type: &str,
        entity_id: &str,
        revision: i64,
        operation: SyncOperation,
        payload: Option<&serde_json::Value>,
        sequence: i64,
    ) -> Result<bool, rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let payload_json = payload
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let remote_digest =
            compute_canonical_digest(entity_type, entity_id, revision, operation, payload);

        let stored: Option<(i64, Option<String>)> = transaction
            .query_row(
                "SELECT revision, canonical_digest FROM cloud_synced_entities WHERE entity_type=?1 AND entity_id=?2",
                params![entity_type, entity_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let (stored_rev, stored_digest) = match stored {
            Some((rev, dig)) => (Some(rev), dig),
            None => (None, None),
        };

        let decision = decide_remote_version(
            stored_rev,
            stored_digest.as_deref(),
            revision,
            &remote_digest,
        );

        if decision == VersionDecision::Stale || decision == VersionDecision::Duplicate {
            transaction.commit()?;
            return Ok(false);
        }

        let pending: Option<(i64, Option<String>)> = transaction
            .query_row(
                "SELECT revision, payload_json FROM cloud_sync_outbox
                 WHERE entity_type=?1 AND entity_id=?2 ORDER BY created_at DESC LIMIT 1",
                params![entity_type, entity_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((local_revision, local_payload)) = pending {
            transaction.execute(
                "INSERT INTO cloud_sync_conflicts (id,entity_type,entity_id,local_revision,remote_revision,
                    local_payload_json,remote_payload_json,remote_operation,status,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'open',?9)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    entity_type,
                    entity_id,
                    local_revision,
                    revision,
                    local_payload,
                    payload_json,
                    operation.as_str(),
                    now
                ],
            )?;
            transaction.execute(
                "DELETE FROM cloud_sync_outbox WHERE entity_type=?1 AND entity_id=?2",
                params![entity_type, entity_id],
            )?;
            transaction.commit()?;
            return Ok(true);
        }

        if decision == VersionDecision::ContentMismatch {
            transaction.execute(
                "INSERT INTO cloud_sync_conflicts (id,entity_type,entity_id,local_revision,remote_revision,
                    local_payload_json,remote_payload_json,remote_operation,status,created_at)
                 VALUES (?1,?2,?3,?4,?5,NULL,?6,?7,'open',?8)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    entity_type,
                    entity_id,
                    revision,
                    revision,
                    payload_json,
                    operation.as_str(),
                    now
                ],
            )?;
            transaction.commit()?;
            return Ok(true);
        }

        super::projection::apply_remote_entity(
            &transaction,
            entity_type,
            entity_id,
            operation,
            payload,
            now,
        )?;

        transaction.execute(
            "INSERT INTO cloud_synced_entities (
                entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(entity_type,entity_id) DO UPDATE SET
                revision=excluded.revision, deleted=excluded.deleted,
                payload_json=excluded.payload_json, canonical_digest=excluded.canonical_digest,
                server_sequence=excluded.server_sequence, updated_at=excluded.updated_at
             WHERE excluded.revision >= cloud_synced_entities.revision",
            params![
                entity_type,
                entity_id,
                revision,
                if operation == SyncOperation::Delete { 1 } else { 0 },
                payload_json,
                remote_digest,
                sequence,
                now
            ],
        )?;

        transaction.execute(
            "INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,authoritative_revision,local_generation,deleted,updated_at)
             VALUES (?1,?2,?3,?4,?4,?4,?5,?6)
             ON CONFLICT(entity_type,entity_id) DO UPDATE SET
                fingerprint=excluded.fingerprint, revision=excluded.revision, authoritative_revision=excluded.authoritative_revision,
                deleted=excluded.deleted, updated_at=excluded.updated_at",
            params![
                entity_type,
                entity_id,
                remote_digest,
                revision,
                if operation == SyncOperation::Delete { 1 } else { 0 },
                now
            ],
        )?;
        transaction.commit()?;
        Ok(false)
    }

    pub fn save_sync_success(&self, cursor: &str) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_sync_cursor (singleton, cursor, download_cursor, last_success_at, last_error_code, updated_at)
             VALUES (1,?1,?1,?2,NULL,?2)
             ON CONFLICT(singleton) DO UPDATE SET cursor=excluded.cursor, download_cursor=excluded.download_cursor,
                last_success_at=excluded.last_success_at, last_error_code=NULL, updated_at=excluded.updated_at",
            params![cursor, now],
        )?;
        Ok(())
    }
}

pub fn assert_checkpoint_tx(
    tx: &rusqlite::Transaction<'_>,
    expected_cursor: &str,
) -> Result<(), SyncError> {
    let current_cursor: Option<String> = tx
        .query_row(
            "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let current = current_cursor.unwrap_or_else(|| "0".to_string());
    if current != expected_cursor {
        return Err(SyncError::CheckpointChanged);
    }
    Ok(())
}

pub fn validate_page(expected_cursor: &str, page: &PullPage) -> Result<(), SyncError> {
    if page.changes.is_empty() {
        if page.has_more {
            return Err(SyncError::InvalidPage(
                "empty page cannot have has_more=true".into(),
            ));
        }
        if page.next_cursor.is_empty() {
            return Err(SyncError::InvalidPage(
                "empty page next_cursor cannot be empty".into(),
            ));
        }
        let exp_num = expected_cursor.parse::<i64>().unwrap_or(0);
        let next_num = page.next_cursor.parse::<i64>().unwrap_or(0);
        if next_num < exp_num {
            return Err(SyncError::InvalidPage(format!(
                "next_cursor {} moved backwards from expected {}",
                page.next_cursor, expected_cursor
            )));
        }
        return Ok(());
    }

    let mut last_seq = expected_cursor.parse::<i64>().unwrap_or(0);
    for (idx, remote) in page.changes.iter().enumerate() {
        remote.validate()?;
        let seq = remote.sequence.parse::<i64>().map_err(|_| {
            SyncError::InvalidPage(format!("invalid sequence: {}", remote.sequence))
        })?;
        if idx == 0 {
            if seq <= last_seq {
                return Err(SyncError::InvalidPage(format!(
                    "first record sequence {} must be greater than expected cursor {}",
                    seq, expected_cursor
                )));
            }
        } else if seq <= last_seq {
            return Err(SyncError::InvalidPage(format!(
                "sequence not strictly increasing: {} <= {}",
                seq, last_seq
            )));
        }
        last_seq = seq;
    }

    let last_change_seq = &page.changes.last().unwrap().sequence;
    if &page.next_cursor != last_change_seq {
        return Err(SyncError::InvalidPage(format!(
            "next_cursor {} does not equal last change sequence {}",
            page.next_cursor, last_change_seq
        )));
    }

    let exp_num = expected_cursor.parse::<i64>().unwrap_or(0);
    let next_num = page.next_cursor.parse::<i64>().unwrap_or(0);
    if next_num <= exp_num {
        return Err(SyncError::InvalidPage(format!(
            "next_cursor {} did not advance from {}",
            page.next_cursor, expected_cursor
        )));
    }

    Ok(())
}

pub fn apply_one_remote_tx(
    tx: &rusqlite::Transaction<'_>,
    remote: &RemoteRecord,
) -> Result<ApplyDisposition, SyncError> {
    validate_sync_payload(
        &remote.entity_type,
        remote.operation,
        remote.payload.as_ref(),
    )?;
    let remote_digest = compute_canonical_digest(
        &remote.entity_type,
        &remote.entity_id,
        remote.revision,
        remote.operation,
        remote.payload.as_ref(),
    );

    let stored: Option<(i64, Option<String>)> = tx
        .query_row(
            "SELECT revision, canonical_digest FROM cloud_synced_entities WHERE entity_type=?1 AND entity_id=?2",
            params![&remote.entity_type, &remote.entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let (stored_rev, stored_digest) = match stored {
        Some((rev, dig)) => (Some(rev), dig),
        None => (None, None),
    };

    let decision = decide_remote_version(
        stored_rev,
        stored_digest.as_deref(),
        remote.revision,
        &remote_digest,
    );

    match decision {
        VersionDecision::Stale => Ok(ApplyDisposition::Stale),
        VersionDecision::Duplicate => Ok(ApplyDisposition::Duplicate),
        VersionDecision::ContentMismatch => {
            let conflict_id =
                super::conflicts::store_integrity_conflict_tx(tx, remote, &remote_digest)?;
            Ok(ApplyDisposition::ConflictStored { conflict_id })
        }
        VersionDecision::Apply => {
            let now = chrono::Utc::now().timestamp_millis();
            let seq_num = remote.sequence.parse::<i64>().unwrap_or(0);
            let payload_json_str = remote
                .payload
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            let pending: Option<(String, i64, Option<String>, String, i64)> = tx
                .query_row(
                    "SELECT change_id, revision, payload_json, state, local_generation FROM cloud_sync_outbox
                     WHERE entity_type=?1 AND entity_id=?2 ORDER BY created_at DESC LIMIT 1",
                    params![&remote.entity_type, &remote.entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .optional()?;

            if let Some((change_id, local_revision, local_payload, _state, _local_gen)) = pending {
                // If it is our own change echo
                if remote.change_id == change_id
                    || (remote.revision == local_revision && remote.device_id.is_empty())
                {
                    tx.execute(
                        "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                            revision=excluded.revision, deleted=excluded.deleted, payload_json=excluded.payload_json,
                            canonical_digest=excluded.canonical_digest, server_sequence=excluded.server_sequence, updated_at=excluded.updated_at
                         WHERE excluded.revision >= cloud_synced_entities.revision",
                        params![
                            &remote.entity_type,
                            &remote.entity_id,
                            remote.revision,
                            if remote.operation == SyncOperation::Delete { 1 } else { 0 },
                            &payload_json_str,
                            &remote_digest,
                            seq_num,
                            now
                        ],
                    )?;
                    tx.execute(
                        "UPDATE cloud_entity_revisions SET authoritative_revision=?1, updated_at=?2 WHERE entity_type=?3 AND entity_id=?4",
                        params![remote.revision, now, &remote.entity_type, &remote.entity_id],
                    )?;
                    tx.execute(
                        "DELETE FROM cloud_sync_outbox WHERE change_id=?1",
                        [&change_id],
                    )?;
                    return Ok(ApplyDisposition::Applied);
                } else {
                    let conflict_id = super::conflicts::store_conflict_tx(
                        tx,
                        &remote.entity_type,
                        &remote.entity_id,
                        local_revision,
                        remote.revision,
                        local_payload.as_deref(),
                        remote.payload.as_ref(),
                        remote.operation,
                    )?;
                    tx.execute(
                        "UPDATE cloud_sync_outbox SET state='conflicted', last_error_code='CONFLICT', updated_at=?2
                         WHERE entity_type=?1 AND entity_id=?2",
                        params![&remote.entity_type, &remote.entity_id, now],
                    )?;
                    return Ok(ApplyDisposition::ConflictStored { conflict_id });
                }
            }

            super::projection::apply_remote_entity(
                tx,
                &remote.entity_type,
                &remote.entity_id,
                remote.operation,
                remote.payload.as_ref(),
                now,
            )?;

            tx.execute(
                "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                    revision=excluded.revision, deleted=excluded.deleted, payload_json=excluded.payload_json,
                    canonical_digest=excluded.canonical_digest, server_sequence=excluded.server_sequence, updated_at=excluded.updated_at
                 WHERE excluded.revision >= cloud_synced_entities.revision",
                params![
                    &remote.entity_type,
                    &remote.entity_id,
                    remote.revision,
                    if remote.operation == SyncOperation::Delete { 1 } else { 0 },
                    &payload_json_str,
                    &remote_digest,
                    seq_num,
                    now
                ],
            )?;

            tx.execute(
                "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4, ?4, ?5, ?6)
                 ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                    fingerprint=excluded.fingerprint, revision=excluded.revision, authoritative_revision=excluded.authoritative_revision,
                    deleted=excluded.deleted, updated_at=excluded.updated_at",
                params![
                    &remote.entity_type,
                    &remote.entity_id,
                    &remote_digest,
                    remote.revision,
                    if remote.operation == SyncOperation::Delete { 1 } else { 0 },
                    now
                ],
            )?;

            Ok(ApplyDisposition::Applied)
        }
    }
}

pub fn apply_selected_remote_tx(
    tx: &rusqlite::Transaction<'_>,
    remote: &RemoteRecord,
    _expected_local_generation: i64,
) -> Result<ApplyDisposition, SyncError> {
    validate_sync_payload(
        &remote.entity_type,
        remote.operation,
        remote.payload.as_ref(),
    )?;
    let remote_digest = compute_canonical_digest(
        &remote.entity_type,
        &remote.entity_id,
        remote.revision,
        remote.operation,
        remote.payload.as_ref(),
    );
    let now = chrono::Utc::now().timestamp_millis();
    let seq_num = remote.sequence.parse::<i64>().unwrap_or(0);
    let payload_json_str = remote
        .payload
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    super::projection::apply_remote_entity(
        tx,
        &remote.entity_type,
        &remote.entity_id,
        remote.operation,
        remote.payload.as_ref(),
        now,
    )?;

    tx.execute(
        "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
            revision=excluded.revision, deleted=excluded.deleted, payload_json=excluded.payload_json,
            canonical_digest=excluded.canonical_digest, server_sequence=excluded.server_sequence, updated_at=excluded.updated_at",
        params![
            &remote.entity_type,
            &remote.entity_id,
            remote.revision,
            if remote.operation == SyncOperation::Delete { 1 } else { 0 },
            &payload_json_str,
            &remote_digest,
            seq_num,
            now
        ],
    )?;

    tx.execute(
        "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4, ?4, ?5, ?6)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
            fingerprint=excluded.fingerprint, revision=excluded.revision, authoritative_revision=excluded.authoritative_revision,
            deleted=excluded.deleted, updated_at=excluded.updated_at",
        params![
            &remote.entity_type,
            &remote.entity_id,
            &remote_digest,
            remote.revision,
            if remote.operation == SyncOperation::Delete { 1 } else { 0 },
            now
        ],
    )?;

    Ok(ApplyDisposition::Applied)
}

pub fn save_checkpoint_tx(
    tx: &rusqlite::Transaction<'_>,
    expected_cursor: &str,
    next_cursor: &str,
) -> Result<(), SyncError> {
    let now = chrono::Utc::now().timestamp_millis();
    let rows = tx.execute(
        "UPDATE cloud_sync_cursor SET download_cursor=?1, cursor=?1, updated_at=?2
         WHERE singleton=1 AND download_cursor=?3",
        params![next_cursor, now, expected_cursor],
    )?;
    if rows != 1 {
        return Err(SyncError::CheckpointChanged);
    }
    Ok(())
}

pub fn apply_pull_page(
    db: &ScopedDb,
    lease: &ScopeLease,
    expected_cursor: &str,
    page: &PullPage,
) -> Result<ApplyPageReport, SyncError> {
    db.with_active_write(lease, |conn: &mut rusqlite::Connection| {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        assert_checkpoint_tx(&tx, expected_cursor)?;
        validate_page(expected_cursor, page)?;
        let mut report = ApplyPageReport::default();
        for remote in &page.changes {
            let disposition = apply_one_remote_tx(&tx, remote)?;
            match disposition {
                ApplyDisposition::Applied => report.applied += 1,
                ApplyDisposition::Duplicate => report.duplicate += 1,
                ApplyDisposition::Stale => report.stale += 1,
                ApplyDisposition::ConflictStored { .. } => report.conflicts += 1,
            }
        }
        save_checkpoint_tx(&tx, expected_cursor, &page.next_cursor)?;
        tx.commit()?;
        report.download_cursor = page.next_cursor.clone();
        Ok(report)
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub previous_cursor: Option<String>,
    pub reset_cursor: String,
    pub outbox_count: usize,
    pub conflicts_recorded: usize,
}

pub fn reconcile_skipped_history(db: &Database) -> Result<ReconciliationReport, SyncError> {
    let mut conn = db.conn();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous_cursor: Option<String> = tx
        .query_row(
            "SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let outbox_count: usize =
        tx.query_row("SELECT COUNT(*) FROM cloud_sync_outbox", [], |row| {
            row.get(0)
        })?;

    let now = chrono::Utc::now().timestamp_millis();
    tx.execute(
        "UPDATE cloud_sync_cursor SET download_cursor='0', cursor='0', updated_at=?1 WHERE singleton=1",
        params![now],
    )?;

    let mut stmt = tx.prepare(
        "SELECT entity_type, entity_id, revision, payload_json FROM cloud_synced_entities WHERE deleted=0",
    )?;
    let rows: Vec<(String, String, i64, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut conflicts_recorded = 0;
    for (etype, eid, rev, payload_json) in rows {
        if etype == "memory" {
            let local_mem: Option<String> = tx
                .query_row("SELECT content FROM memories WHERE id=?1", [&eid], |row| {
                    row.get(0)
                })
                .optional()?;
            if let Some(content) = local_mem {
                let remote_content = payload_json.as_deref().and_then(|t| {
                    serde_json::from_str::<serde_json::Value>(t)
                        .ok()
                        .and_then(|v| {
                            v.get("content")
                                .and_then(|c| c.as_str())
                                .map(str::to_string)
                        })
                });
                if remote_content.as_deref() != Some(&content) {
                    let conflict_id = uuid::Uuid::new_v4().to_string();
                    let local_payload_json = serde_json::json!({"content": content}).to_string();
                    tx.execute(
                        "INSERT INTO cloud_sync_conflicts (id, entity_type, entity_id, local_revision, remote_revision, local_payload_json, remote_payload_json, remote_operation, status, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 'upsert', 'open', ?7)",
                        params![conflict_id, etype, eid, rev, local_payload_json, payload_json, now],
                    )?;
                    conflicts_recorded += 1;
                }
            }
        }
    }

    tx.commit()?;
    Ok(ReconciliationReport {
        previous_cursor,
        reset_cursor: "0".to_string(),
        outbox_count,
        conflicts_recorded,
    })
}

impl ScopedDb {
    pub fn apply_pull_page(
        &self,
        lease: &ScopeLease,
        expected_cursor: &str,
        page: &PullPage,
    ) -> Result<ApplyPageReport, SyncError> {
        apply_pull_page(self, lease, expected_cursor, page)
    }
}

impl Database {
    pub fn reconcile_skipped_history(&self) -> Result<ReconciliationReport, SyncError> {
        reconcile_skipped_history(self)
    }
}
