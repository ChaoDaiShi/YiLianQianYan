use crate::db::Database;
use crate::modules::cloud::domain::{NewSyncChange, SyncOperation, SyncOutboxItem};
use rusqlite::{params, OptionalExtension};

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
