use crate::db::Database;
use crate::modules::cloud::contracts::{
    ConflictRecord, PendingChange, RemoteRecord, ResolutionChoice, ResolveConflictRequest,
    SyncError,
};
use crate::modules::cloud::domain::{SyncConflict, SyncOperation, SyncOutboxItem};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

impl Database {
    pub fn record_sync_conflict(
        &self,
        item: &SyncOutboxItem,
        authoritative_revision: i64,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let local_payload = item
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_sync_conflicts (
                id, entity_type, entity_id, local_revision, remote_revision,
                local_payload_json, remote_payload_json, status, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,NULL,'open',?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                item.entity_type,
                item.entity_id,
                item.revision,
                authoritative_revision,
                local_payload,
                now
            ],
        )?;
        conn.execute(
            "DELETE FROM cloud_sync_outbox WHERE change_id=?1",
            [&item.change_id],
        )?;
        Ok(())
    }

    pub fn list_sync_conflicts(&self) -> Result<Vec<SyncConflict>, rusqlite::Error> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT id,entity_type,entity_id,local_revision,remote_revision,local_payload_json,remote_payload_json,remote_operation,created_at
             FROM cloud_sync_conflicts WHERE status='open' ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            let local: Option<String> = row.get(5)?;
            let remote: Option<String> = row.get(6)?;
            let operation: String = row.get(7)?;
            Ok(SyncConflict {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                local_revision: row.get(3)?,
                remote_revision: row.get(4)?,
                local_payload: decode_optional_json(local, 5)?,
                remote_payload: decode_optional_json(remote, 6)?,
                remote_operation: SyncOperation::parse(&operation)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_conflict_records(&self) -> Result<Vec<ConflictRecord>, rusqlite::Error> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT id, entity_type, entity_id, local_revision, remote_revision,
                    local_operation, remote_operation, local_payload_json, remote_payload_json,
                    remote_change_id, remote_sequence, conflict_version, local_generation,
                    status, resolution_request_id, queued_change_id, copied_entity_id,
                    created_at, resolved_at
             FROM cloud_sync_conflicts ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| map_conflict_row(row))?;
        rows.collect()
    }

    pub fn resolve_sync_conflict(&self, id: &str, resolution: &str) -> Result<(), rusqlite::Error> {
        let choice = if resolution == "remote" {
            ResolutionChoice::UseRemote
        } else {
            ResolutionChoice::KeepLocal
        };
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let conflict = get_conflict_tx(&tx, id).map_err(|e| match e {
            SyncError::Database(err) => err,
            _ => rusqlite::Error::InvalidQuery,
        })?;
        let request = ResolveConflictRequest {
            conflict_id: id.to_string(),
            expected_conflict_version: conflict.conflict_version,
            expected_remote_revision: conflict.remote_revision,
            choice,
            request_id: uuid::Uuid::new_v4().to_string(),
        };
        let identity: String = tx
            .query_row(
                "SELECT device_id FROM cloud_identity WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        resolve_conflict_tx(&tx, &request, &identity).map_err(|e| match e {
            SyncError::Database(err) => err,
            _ => rusqlite::Error::InvalidQuery,
        })?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn decode_optional_json(
    value: Option<String>,
    column: usize,
) -> Result<Option<Value>, rusqlite::Error> {
    value
        .map(|text| {
            serde_json::from_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    column,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

fn map_conflict_row(row: &rusqlite::Row<'_>) -> Result<ConflictRecord, rusqlite::Error> {
    let local_op_str: String = row.get(5)?;
    let remote_op_str: String = row.get(6)?;
    let local_payload_str: Option<String> = row.get(7)?;
    let remote_payload_str: Option<String> = row.get(8)?;

    Ok(ConflictRecord {
        id: row.get(0)?,
        entity_type: row.get(1)?,
        entity_id: row.get(2)?,
        local_revision: row.get(3)?,
        remote_revision: row.get(4)?,
        local_operation: SyncOperation::parse(&local_op_str).unwrap_or(SyncOperation::Upsert),
        remote_operation: SyncOperation::parse(&remote_op_str).unwrap_or(SyncOperation::Upsert),
        local_payload: decode_optional_json(local_payload_str, 7)?,
        remote_payload: decode_optional_json(remote_payload_str, 8)?,
        remote_change_id: row.get(9)?,
        remote_sequence: row.get(10)?,
        conflict_version: row.get(11)?,
        local_generation: row.get(12)?,
        status: row.get(13)?,
        resolution_request_id: row.get(14)?,
        queued_change_id: row.get(15)?,
        copied_entity_id: row.get(16)?,
        created_at: row.get(17)?,
        resolved_at: row.get(18)?,
    })
}

pub fn get_conflict_tx(
    tx: &rusqlite::Transaction<'_>,
    conflict_id: &str,
) -> Result<ConflictRecord, SyncError> {
    tx.query_row(
        "SELECT id, entity_type, entity_id, local_revision, remote_revision,
                local_operation, remote_operation, local_payload_json, remote_payload_json,
                remote_change_id, remote_sequence, conflict_version, local_generation,
                status, resolution_request_id, queued_change_id, copied_entity_id,
                created_at, resolved_at
         FROM cloud_sync_conflicts WHERE id=?1",
        [conflict_id],
        |row| map_conflict_row(row),
    )
    .map_err(SyncError::from)
}

pub fn store_conflict_tx(
    tx: &rusqlite::Transaction<'_>,
    local: &PendingChange,
    remote: &RemoteRecord,
) -> Result<String, SyncError> {
    let now = chrono::Utc::now().timestamp_millis();
    let local_payload_str = local
        .payload
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let remote_payload_str = remote
        .payload
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    // Check for an existing open or resolving conflict for this entity
    let existing: Option<(String, i64, Option<String>, i64)> = tx
        .query_row(
            "SELECT id, conflict_version, remote_change_id, remote_revision FROM cloud_sync_conflicts
             WHERE entity_type=?1 AND entity_id=?2 AND status IN ('open', 'resolving')
             ORDER BY created_at DESC LIMIT 1",
            params![&local.entity_type, &local.entity_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;

    if let Some((existing_id, version, existing_remote_change_id, existing_remote_rev)) = existing {
        // If identical remote change / revision, return existing conflict id without creating unbounded rows
        if existing_remote_change_id.as_deref() == Some(&remote.change_id)
            || (remote.change_id.is_empty() && existing_remote_rev == remote.revision)
        {
            return Ok(existing_id);
        }

        // Remote revision has changed or updated while conflict was open/resolving: update it
        tx.execute(
            "UPDATE cloud_sync_conflicts SET
                local_revision=?2, remote_revision=?3, local_operation=?4, remote_operation=?5,
                local_payload_json=?6, remote_payload_json=?7, remote_change_id=?8, remote_sequence=?9,
                conflict_version=?10, local_generation=?11, status='open', resolution_request_id=NULL,
                queued_change_id=NULL, created_at=?12
             WHERE id=?1",
            params![
                &existing_id,
                local.revision,
                remote.revision,
                local.operation.as_str(),
                remote.operation.as_str(),
                local_payload_str,
                remote_payload_str,
                &remote.change_id,
                &remote.sequence,
                version + 1,
                local.local_generation,
                now,
            ],
        )?;
        return Ok(existing_id);
    }

    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO cloud_sync_conflicts (
            id, entity_type, entity_id, local_revision, remote_revision,
            local_operation, remote_operation, local_payload_json, remote_payload_json,
            remote_change_id, remote_sequence, conflict_version, local_generation,
            status, created_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12,'open',?13)",
        params![
            &id,
            &local.entity_type,
            &local.entity_id,
            local.revision,
            remote.revision,
            local.operation.as_str(),
            remote.operation.as_str(),
            local_payload_str,
            remote_payload_str,
            &remote.change_id,
            &remote.sequence,
            local.local_generation,
            now,
        ],
    )?;
    Ok(id)
}

pub fn store_integrity_conflict_tx(
    tx: &rusqlite::Transaction<'_>,
    remote: &RemoteRecord,
    _remote_digest: &str,
) -> Result<String, SyncError> {
    // Look up local entity data from outbox or synced entities
    let local_outbox: Option<(i64, String, Option<String>, i64)> = tx
        .query_row(
            "SELECT revision, operation, payload_json, local_generation FROM cloud_sync_outbox
             WHERE entity_type=?1 AND entity_id=?2 ORDER BY created_at DESC LIMIT 1",
            params![&remote.entity_type, &remote.entity_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;

    let (local_rev, local_op, local_payload, local_gen) = match local_outbox {
        Some((rev, op, payload, gen)) => (
            rev,
            SyncOperation::parse(&op).unwrap_or(SyncOperation::Upsert),
            payload.and_then(|p| serde_json::from_str(&p).ok()),
            gen,
        ),
        None => {
            let synced: Option<(i64, Option<String>)> = tx
                .query_row(
                    "SELECT revision, payload_json FROM cloud_synced_entities WHERE entity_type=?1 AND entity_id=?2",
                    params![&remote.entity_type, &remote.entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            match synced {
                Some((rev, p)) => (
                    rev,
                    SyncOperation::Upsert,
                    p.and_then(|str_val| serde_json::from_str(&str_val).ok()),
                    0,
                ),
                None => (remote.revision, SyncOperation::Upsert, None, 0),
            }
        }
    };

    let local = PendingChange {
        change_id: uuid::Uuid::new_v4().to_string(),
        entity_type: remote.entity_type.clone(),
        entity_id: remote.entity_id.clone(),
        base_revision: local_rev,
        revision: local_rev,
        operation: local_op,
        payload: local_payload,
        device_id: String::new(),
        changed_at: chrono::Utc::now().timestamp_millis(),
        local_generation: local_gen,
    };

    store_conflict_tx(tx, &local, remote)
}

pub use super::conflict_resolve::*;
