use crate::db::Database;
use crate::modules::cloud::domain::{CloudSyncStatus, SyncOperation};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn cloud_sync_status(&self) -> Result<CloudSyncStatus, rusqlite::Error> {
        let conn = self.conn();
        let (pending_count, error_count) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN state='error' THEN 1 ELSE 0 END),0) FROM cloud_sync_outbox",
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
                "SELECT cursor, last_success_at, last_error_code FROM cloud_sync_cursor WHERE singleton=1",
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
        use sha2::{Digest, Sha256};
        let now = chrono::Utc::now().timestamp_millis();
        let payload_json = payload
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending: Option<(i64, Option<String>)> = transaction
            .query_row(
                "SELECT revision,payload_json FROM cloud_sync_outbox
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
        transaction.execute(
            "INSERT INTO cloud_synced_entities (
                entity_type, entity_id, revision, deleted, payload_json, server_sequence, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(entity_type,entity_id) DO UPDATE SET
                revision=excluded.revision, deleted=excluded.deleted,
                payload_json=excluded.payload_json, server_sequence=excluded.server_sequence,
                updated_at=excluded.updated_at
             WHERE excluded.revision >= cloud_synced_entities.revision",
            params![
                entity_type,
                entity_id,
                revision,
                if operation == SyncOperation::Delete {
                    1
                } else {
                    0
                },
                payload_json,
                sequence,
                now
            ],
        )?;
        super::projection::apply_remote_entity(
            &transaction,
            entity_type,
            entity_id,
            operation,
            payload,
            now,
        )?;
        let fingerprint = if operation == SyncOperation::Delete {
            String::new()
        } else {
            Sha256::digest(payload_json.as_deref().unwrap_or("null").as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        };
        transaction.execute(
            "INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,deleted,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,
                revision=excluded.revision,deleted=excluded.deleted,updated_at=excluded.updated_at",
            params![
                entity_type,
                entity_id,
                fingerprint,
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
            "INSERT INTO cloud_sync_cursor (singleton, cursor, last_success_at, last_error_code, updated_at)
             VALUES (1,?1,?2,NULL,?2)
             ON CONFLICT(singleton) DO UPDATE SET cursor=excluded.cursor,
                last_success_at=excluded.last_success_at, last_error_code=NULL, updated_at=excluded.updated_at",
            params![cursor, now],
        )?;
        Ok(())
    }
}
