use crate::db::Database;
use crate::modules::cloud::domain::{
    CloudSyncStatus, NewSyncChange, SyncOperation, SyncOutboxItem,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn enqueue_sync_change(
        &self,
        change: NewSyncChange,
    ) -> Result<SyncOutboxItem, rusqlite::Error> {
        let identity = self.get_or_create_cloud_identity()?;
        let change_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let payload_json = change
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_sync_outbox (
                change_id, entity_type, entity_id, base_revision, revision, operation,
                payload_json, device_id, state, attempts, created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending',0,?9,?9)",
            params![
                change_id,
                change.entity_type,
                change.entity_id,
                change.base_revision,
                change.revision,
                change.operation.as_str(),
                payload_json,
                identity.device_id,
                now,
            ],
        )?;
        drop(conn);
        self.sync_outbox_item(&change_id)?
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
            "UPDATE cloud_sync_outbox SET state='error', attempts=attempts+1,
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
             WHERE state='pending' OR next_retry_at IS NULL OR next_retry_at<=?1
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
                params![uuid::Uuid::new_v4().to_string(),entity_type,entity_id,local_revision,revision,local_payload,payload_json,operation.as_str(),now],
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
            params![entity_type,entity_id,fingerprint,revision,if operation==SyncOperation::Delete{1}else{0},now],
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

    pub fn list_sync_conflicts(
        &self,
    ) -> Result<Vec<crate::modules::cloud::domain::SyncConflict>, rusqlite::Error> {
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT id,entity_type,entity_id,local_revision,remote_revision,local_payload_json,remote_payload_json,remote_operation,created_at FROM cloud_sync_conflicts WHERE status='open' ORDER BY created_at")?;
        let rows = statement.query_map([], |row| {
            let local: Option<String> = row.get(5)?;
            let remote: Option<String> = row.get(6)?;
            let operation: String = row.get(7)?;
            Ok(crate::modules::cloud::domain::SyncConflict {
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
        let row:(String,String,i64,i64,Option<String>,Option<String>,String)=transaction.query_row("SELECT entity_type,entity_id,local_revision,remote_revision,local_payload_json,remote_payload_json,remote_operation FROM cloud_sync_conflicts WHERE id=?1 AND status='open'",[id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
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
            transaction.execute("INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,deleted,updated_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,revision=excluded.revision,deleted=excluded.deleted,updated_at=excluded.updated_at",params![row.0,row.1,fingerprint,row.3,if operation==SyncOperation::Delete{1}else{0},now])?;
        } else {
            let identity: String = transaction.query_row(
                "SELECT device_id FROM cloud_identity WHERE singleton=1",
                [],
                |row| row.get(0),
            )?;
            transaction.execute("INSERT INTO cloud_sync_outbox (change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,device_id,state,attempts,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,'upsert',?6,?7,'pending',0,?8,?8)",params![uuid::Uuid::new_v4().to_string(),row.0,row.1,row.3,row.3+1,row.4,identity,now])?;
        }
        transaction.execute(
            "UPDATE cloud_sync_conflicts SET status='resolved',resolved_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn map_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncOutboxItem> {
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

fn decode_optional_json(
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
