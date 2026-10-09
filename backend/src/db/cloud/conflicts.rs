use crate::db::Database;
use crate::modules::cloud::domain::{SyncConflict, SyncOperation, SyncOutboxItem};
use rusqlite::{params, TransactionBehavior};

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

    pub fn resolve_sync_conflict(&self, id: &str, resolution: &str) -> Result<(), rusqlite::Error> {
        if !matches!(resolution, "local" | "remote") {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid conflict resolution".into(),
            ));
        }
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row: (
            String,
            String,
            i64,
            i64,
            Option<String>,
            Option<String>,
            String,
        ) = transaction.query_row(
            "SELECT entity_type,entity_id,local_revision,remote_revision,local_payload_json,remote_payload_json,remote_operation
             FROM cloud_sync_conflicts WHERE id=?1 AND status='open'",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )?;
        let now = chrono::Utc::now().timestamp_millis();
        let operation = SyncOperation::parse(&row.6).ok_or(rusqlite::Error::InvalidQuery)?;
        if resolution == "remote" {
            let payload = decode_optional_json(row.5.clone(), 5)?;
            super::projection::apply_remote_entity(
                &transaction,
                &row.0,
                &row.1,
                operation,
                payload.as_ref(),
                now,
            )?;
            let fingerprint: String = payload
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .map(|text| {
                    use sha2::{Digest, Sha256};
                    Sha256::digest(text.as_bytes())
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect()
                })
                .unwrap_or_default();
            transaction.execute(
                "INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,deleted,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,revision=excluded.revision,deleted=excluded.deleted,updated_at=excluded.updated_at",
                params![row.0, row.1, fingerprint, row.3, if operation == SyncOperation::Delete { 1 } else { 0 }, now],
            )?;
        } else {
            let identity: String = transaction.query_row(
                "SELECT device_id FROM cloud_identity WHERE singleton=1",
                [],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,device_id,state,attempts,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,'upsert',?6,?7,'pending',0,?8,?8)",
                params![uuid::Uuid::new_v4().to_string(), row.0, row.1, row.3, row.3 + 1, row.4, identity, now],
            )?;
        }
        transaction.execute(
            "UPDATE cloud_sync_conflicts SET status='resolved',resolved_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

pub(super) fn decode_optional_json(
    value: Option<String>,
    column: usize,
) -> Result<Option<serde_json::Value>, rusqlite::Error> {
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
