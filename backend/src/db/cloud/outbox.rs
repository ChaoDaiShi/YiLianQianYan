use crate::db::Database;
use crate::modules::cloud::contracts::{
    compute_canonical_digest, AckReport, PendingChange, PushAck, PushStatus, RemoteRecord,
    ScopeLease, ScopedDb, SyncError,
};
use crate::modules::cloud::domain::{NewSyncChange, SyncOperation, SyncOutboxItem};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::collections::HashSet;

impl Database {
    pub fn enqueue_sync_change(
        &self,
        change: NewSyncChange,
    ) -> Result<SyncOutboxItem, rusqlite::Error> {
        let identity = self.get_or_create_cloud_identity()?;
        let change_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let canonical_digest = compute_canonical_digest(
            &change.entity_type,
            &change.entity_id,
            change.revision,
            change.operation,
            change.payload.as_ref(),
        );
        let payload_json = change
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Determine local_generation and bump it
        let current_gen: Option<i64> = tx
            .query_row(
                "SELECT local_generation FROM cloud_entity_revisions WHERE entity_type=?1 AND entity_id=?2",
                params![&change.entity_type, &change.entity_id],
                |row| row.get(0),
            )
            .optional()?;
        let local_gen = current_gen.unwrap_or(0) + 1;

        tx.execute(
            "INSERT INTO cloud_entity_revisions (entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)
             ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                fingerprint=excluded.fingerprint,
                revision=excluded.revision,
                local_generation=excluded.local_generation,
                deleted=excluded.deleted,
                updated_at=excluded.updated_at",
            params![
                &change.entity_type,
                &change.entity_id,
                &canonical_digest,
                change.base_revision,
                local_gen,
                if change.operation == SyncOperation::Delete { 1 } else { 0 },
                now
            ],
        )?;

        // Check if an un-sent pending change exists for this entity to merge drafts
        let pending_unsent: Option<String> = tx
            .query_row(
                "SELECT change_id FROM cloud_sync_outbox WHERE entity_type=?1 AND entity_id=?2 AND state='pending' LIMIT 1",
                params![&change.entity_type, &change.entity_id],
                |row| row.get(0),
            )
            .optional()?;

        let actual_change_id = if let Some(existing_id) = pending_unsent {
            tx.execute(
                "UPDATE cloud_sync_outbox SET
                    base_revision=?2, revision=?3, operation=?4, payload_json=?5, canonical_digest=?6,
                    local_generation=?7, updated_at=?8
                 WHERE change_id=?1",
                params![
                    existing_id,
                    change.base_revision,
                    change.revision,
                    change.operation.as_str(),
                    payload_json,
                    canonical_digest,
                    local_gen,
                    now
                ],
            )?;
            existing_id
        } else {
            tx.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, canonical_digest, device_id, state, attempts, local_generation, created_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',0,?10,?11,?11)",
                params![
                    change_id,
                    change.entity_type,
                    change.entity_id,
                    change.base_revision,
                    change.revision,
                    change.operation.as_str(),
                    payload_json,
                    canonical_digest,
                    identity.device_id,
                    local_gen,
                    now,
                ],
            )?;
            change_id
        };
        tx.commit()?;
        drop(conn);

        self.sync_outbox_item(&actual_change_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn sync_outbox_item(
        &self,
        change_id: &str,
    ) -> Result<Option<SyncOutboxItem>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, device_id, state, attempts, next_retry_at, last_error_code,
                    created_at, updated_at
             FROM cloud_sync_outbox WHERE change_id=?1",
            [change_id],
            map_outbox,
        )
        .optional()
    }

    pub fn mark_sync_change_failed(
        &self,
        change_id: &str,
        error_code: &str,
        next_retry_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_sync_outbox SET state='retry_wait', attempts=attempts+1,
                    next_retry_at=?2, last_error_code=?3, updated_at=?4 WHERE change_id=?1",
            params![change_id, next_retry_at, error_code, now],
        )?;
        conn.execute(
            "INSERT INTO cloud_sync_cursor (singleton, last_error_code, updated_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET last_error_code=excluded.last_error_code, updated_at=excluded.updated_at",
            params![error_code, now],
        )?;
        Ok(())
    }

    pub fn pending_sync_changes(
        &self,
        limit: usize,
    ) -> Result<Vec<SyncOutboxItem>, rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, device_id, state, attempts, next_retry_at, last_error_code,
                    created_at, updated_at
             FROM cloud_sync_outbox
             WHERE state='pending' OR (state='retry_wait' AND (next_retry_at IS NULL OR next_retry_at<=?1))
             ORDER BY created_at LIMIT ?2",
        )?;
        let rows = statement.query_map(params![now, limit.min(100) as i64], map_outbox)?;
        rows.collect()
    }

    pub fn remove_sync_change(&self, change_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM cloud_sync_outbox WHERE change_id=?1",
            [change_id],
        )?;
        Ok(())
    }

    pub fn pending_changes(
        &self,
        _lease: &ScopeLease,
        limit: u32,
        now_ms: i64,
    ) -> Result<Vec<PendingChange>, SyncError> {
        let conn = self.conn();
        let mut in_flight_set: HashSet<(String, String)> = HashSet::new();
        let mut in_flight_stmt = conn.prepare(
            "SELECT entity_type, entity_id FROM cloud_sync_outbox WHERE state='in_flight'",
        )?;
        let in_flight_rows = in_flight_stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        for row in in_flight_rows {
            let (etype, eid) = row?;
            in_flight_set.insert((etype, eid));
        }

        let mut stmt = conn.prepare(
            "SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, device_id, changed_at_or_created, local_generation
             FROM (
                 SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                        payload_json, device_id, created_at AS changed_at_or_created, local_generation, state, next_retry_at, created_at
                 FROM cloud_sync_outbox
             )
             WHERE state='pending' OR (state='retry_wait' AND (next_retry_at IS NULL OR next_retry_at<=?1))
             ORDER BY created_at ASC",
        )?;
        let mut seen_in_batch: HashSet<(String, String)> = HashSet::new();
        let mut changes = Vec::new();
        let mut rows = stmt.query(params![now_ms])?;
        while let Some(row) = rows.next()? {
            let entity_type: String = row.get(1)?;
            let entity_id: String = row.get(2)?;
            let key = (entity_type.clone(), entity_id.clone());
            if in_flight_set.contains(&key) || seen_in_batch.contains(&key) {
                continue;
            }
            seen_in_batch.insert(key);

            let change_id: String = row.get(0)?;
            let base_revision: i64 = row.get(3)?;
            let revision: i64 = row.get(4)?;
            let op_str: String = row.get(5)?;
            let payload_json: Option<String> = row.get(6)?;
            let payload = payload_json.and_then(|text| serde_json::from_str(&text).ok());
            let device_id: String = row.get(7)?;
            let changed_at: i64 = row.get(8)?;
            let local_generation: i64 = row.get(9)?;

            changes.push(PendingChange {
                change_id,
                entity_type,
                entity_id,
                base_revision,
                revision,
                operation: SyncOperation::parse(&op_str).unwrap_or(SyncOperation::Upsert),
                payload,
                device_id,
                changed_at,
                local_generation,
            });
            if changes.len() >= limit as usize {
                break;
            }
        }
        Ok(changes)
    }

    pub fn load_or_create_push_batch(
        &self,
        _lease: &ScopeLease,
        limit: usize,
    ) -> Result<Vec<PendingChange>, SyncError> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let policy = crate::db::cloud::capture::read_sync_policy_tx(&tx)?;

        let mut in_flight_set: HashSet<(String, String)> = HashSet::new();
        {
            let mut in_flight_stmt = tx.prepare(
                "SELECT entity_type, entity_id FROM cloud_sync_outbox WHERE state='in_flight'",
            )?;
            let in_flight_rows =
                in_flight_stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            for row in in_flight_rows {
                let (etype, eid) = row?;
                in_flight_set.insert((etype, eid));
            }
        }

        let mut stmt = tx.prepare(
            "SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, device_id, created_at, local_generation
             FROM cloud_sync_outbox
             WHERE state='pending' OR (state='retry_wait' AND (next_retry_at IS NULL OR next_retry_at<=?1))
             ORDER BY created_at ASC",
        )?;
        let mut seen_in_batch: HashSet<(String, String)> = HashSet::new();
        let mut batch = Vec::new();
        let mut change_ids_to_in_flight = Vec::new();

        let mut rows = stmt.query(params![now])?;
        while let Some(row) = rows.next()? {
            let entity_type: String = row.get(1)?;
            if !policy.allows_type(&entity_type)? {
                continue;
            }
            let entity_id: String = row.get(2)?;
            let key = (entity_type.clone(), entity_id.clone());
            if in_flight_set.contains(&key) || seen_in_batch.contains(&key) {
                continue;
            }
            seen_in_batch.insert(key);

            let change_id: String = row.get(0)?;
            let base_revision: i64 = row.get(3)?;
            let revision: i64 = row.get(4)?;
            let op_str: String = row.get(5)?;
            let payload_json: Option<String> = row.get(6)?;
            let payload = payload_json.and_then(|text| serde_json::from_str(&text).ok());
            let device_id: String = row.get(7)?;
            let changed_at: i64 = row.get(8)?;
            let local_generation: i64 = row.get(9)?;

            change_ids_to_in_flight.push(change_id.clone());
            batch.push(PendingChange {
                change_id,
                entity_type,
                entity_id,
                base_revision,
                revision,
                operation: SyncOperation::parse(&op_str).unwrap_or(SyncOperation::Upsert),
                payload,
                device_id,
                changed_at,
                local_generation,
            });
            if batch.len() >= limit {
                break;
            }
        }
        drop(rows);
        drop(stmt);

        for cid in &change_ids_to_in_flight {
            tx.execute(
                "UPDATE cloud_sync_outbox SET state='in_flight', updated_at=?2 WHERE change_id=?1",
                params![cid, now],
            )?;
        }
        tx.commit()?;
        Ok(batch)
    }

    pub fn ack_push(&self, _lease: &ScopeLease, ack: &PushAck) -> Result<AckReport, SyncError> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut report = AckReport::default();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for result in &ack.results {
            if !seen_ids.insert(result.change_id.clone()) {
                return Err(SyncError::InvalidPage(
                    "duplicate change_id in push ack".into(),
                ));
            }

            let item_opt: Option<(String, String, i64, String, Option<String>, Option<String>, i64, String, i64)> = tx
                .query_row(
                    "SELECT entity_type, entity_id, revision, operation, payload_json, canonical_digest, base_revision, device_id, local_generation
                     FROM cloud_sync_outbox WHERE change_id=?1",
                    [&result.change_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?)),
                )
                .optional()?;

            let Some((
                entity_type,
                entity_id,
                local_revision,
                op_str,
                payload_json,
                digest,
                base_rev,
                device_id,
                local_gen,
            )) = item_opt
            else {
                continue;
            };

            let operation = SyncOperation::parse(&op_str).unwrap_or(SyncOperation::Upsert);

            super::conflicts::finalize_resolution_ack_tx(&tx, &result.change_id, result)?;

            match result.status {
                PushStatus::Applied | PushStatus::Duplicate => {
                    let auth_rev = result.authoritative_revision.unwrap_or(local_revision);
                    let digest_val = digest.unwrap_or_else(|| {
                        compute_canonical_digest(
                            &entity_type,
                            &entity_id,
                            auth_rev,
                            operation,
                            payload_json
                                .as_deref()
                                .and_then(|t| serde_json::from_str(t).ok())
                                .as_ref(),
                        )
                    });
                    tx.execute(
                        "INSERT INTO cloud_synced_entities (entity_type, entity_id, revision, deleted, payload_json, canonical_digest, server_sequence, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
                         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                            revision=excluded.revision, deleted=excluded.deleted, payload_json=excluded.payload_json,
                            canonical_digest=excluded.canonical_digest, updated_at=excluded.updated_at
                         WHERE excluded.revision >= cloud_synced_entities.revision",
                        params![
                            &entity_type,
                            &entity_id,
                            auth_rev,
                            if operation == SyncOperation::Delete { 1 } else { 0 },
                            &payload_json,
                            &digest_val,
                            now
                        ],
                    )?;
                    tx.execute(
                        "UPDATE cloud_entity_revisions SET revision=?1, authoritative_revision=?1, updated_at=?2
                         WHERE entity_type=?3 AND entity_id=?4",
                        params![auth_rev, now, &entity_type, &entity_id],
                    )?;
                    tx.execute(
                        "DELETE FROM cloud_sync_outbox WHERE change_id=?1",
                        [&result.change_id],
                    )?;

                    if result.status == PushStatus::Applied {
                        report.applied += 1;
                    } else {
                        report.duplicate += 1;
                    }
                }
                PushStatus::Conflict => {
                    let auth_rev = result.authoritative_revision.unwrap_or(local_revision);
                    let remote_payload = result
                        .authoritative
                        .as_ref()
                        .and_then(|r| r.payload.as_ref());
                    let remote_op = result
                        .authoritative
                        .as_ref()
                        .map(|r| r.operation)
                        .unwrap_or(SyncOperation::Upsert);
                    let local_change = PendingChange {
                        change_id: result.change_id.clone(),
                        entity_type: entity_type.clone(),
                        entity_id: entity_id.clone(),
                        base_revision: base_rev,
                        revision: local_revision,
                        operation,
                        payload: payload_json
                            .as_deref()
                            .and_then(|t| serde_json::from_str(t).ok()),
                        device_id,
                        changed_at: now,
                        local_generation: local_gen,
                    };
                    let remote_rec = result
                        .authoritative
                        .clone()
                        .unwrap_or_else(|| RemoteRecord {
                            change_id: String::new(),
                            entity_type: entity_type.clone(),
                            entity_id: entity_id.clone(),
                            revision: auth_rev,
                            operation: remote_op,
                            payload: remote_payload.cloned(),
                            sequence: "0".to_string(),
                            device_id: String::new(),
                            changed_at: now,
                        });
                    super::conflicts::store_conflict_tx(&tx, &local_change, &remote_rec)?;
                    tx.execute(
                        "UPDATE cloud_sync_outbox SET state='conflicted', last_error_code='CONFLICT', updated_at=?2 WHERE change_id=?1",
                        params![&result.change_id, now],
                    )?;
                    report.conflicts += 1;
                }
                PushStatus::Rejected => {
                    let err = result.error_code.as_deref().unwrap_or("SYNC_REJECTED");
                    tx.execute(
                        "UPDATE cloud_sync_outbox SET state='rejected', last_error_code=?2, updated_at=?3 WHERE change_id=?1",
                        params![&result.change_id, err, now],
                    )?;
                    report.rejected += 1;
                }
            }
        }
        tx.commit()?;
        Ok(report)
    }
}

impl ScopedDb {
    pub fn pending_changes(
        &self,
        lease: &ScopeLease,
        limit: u32,
        now_ms: i64,
    ) -> Result<Vec<PendingChange>, SyncError> {
        self.validate_lease(lease)?;
        self.db().pending_changes(lease, limit, now_ms)
    }

    pub fn load_or_create_push_batch(
        &self,
        lease: &ScopeLease,
        limit: usize,
    ) -> Result<Vec<PendingChange>, SyncError> {
        self.validate_lease(lease)?;
        self.db().load_or_create_push_batch(lease, limit)
    }

    pub fn ack_push(&self, lease: &ScopeLease, ack: &PushAck) -> Result<AckReport, SyncError> {
        self.validate_lease(lease)?;
        self.db().ack_push(lease, ack)
    }
}

pub(super) fn map_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncOutboxItem> {
    let operation: String = row.get(5)?;
    let payload: Option<String> = row.get(6)?;
    let payload = payload
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(SyncOutboxItem {
        change_id: row.get(0)?,
        entity_type: row.get(1)?,
        entity_id: row.get(2)?,
        base_revision: row.get(3)?,
        revision: row.get(4)?,
        operation: SyncOperation::parse(&operation).ok_or(rusqlite::Error::InvalidQuery)?,
        payload,
        device_id: row.get(7)?,
        state: row.get(8)?,
        attempts: row.get(9)?,
        next_retry_at: row.get(10)?,
        last_error_code: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}
