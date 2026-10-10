use crate::db::Database;
use crate::modules::cloud::contracts::{ScopeLease, ScopedDb, SyncError};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub fn read_checkpoint(
        &self,
        _lease: &ScopeLease,
    ) -> Result<(String, Option<String>), SyncError> {
        let conn = self.conn();
        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT download_cursor, sync_epoch FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row.unwrap_or_else(|| ("0".into(), None)))
    }

    pub fn bind_sync_epoch(&self, epoch: &str) -> Result<(), SyncError> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        let current: Option<String> = conn
            .query_row(
                "SELECT sync_epoch FROM cloud_sync_cursor WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if current.as_deref().is_some_and(|stored| stored != epoch) {
            return Err(SyncError::CursorResetRequired);
        }
        conn.execute(
            "INSERT INTO cloud_sync_cursor (singleton, cursor, download_cursor, sync_epoch, updated_at)
             VALUES (1, '0', '0', ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET sync_epoch=excluded.sync_epoch, updated_at=excluded.updated_at",
            params![epoch, now],
        )?;
        Ok(())
    }

    pub fn replace_sync_epoch_after_reset(&self, epoch: &str) -> Result<(), SyncError> {
        let conn = self.conn();
        let changed = conn.execute(
            "UPDATE cloud_sync_cursor
             SET cursor='0', download_cursor='0', sync_epoch=?1, updated_at=?2
             WHERE singleton=1",
            params![epoch, chrono::Utc::now().timestamp_millis()],
        )?;
        if changed != 1 {
            return Err(SyncError::CheckpointChanged);
        }
        Ok(())
    }

    pub fn requeue_push_batch(
        &self,
        change_ids: &[String],
        error_code: &str,
    ) -> Result<(), SyncError> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for change_id in change_ids {
            tx.execute(
                "UPDATE cloud_sync_outbox
                 SET state='retry_wait', attempts=attempts+1, next_retry_at=?2,
                     last_error_code=?3, updated_at=?2
                 WHERE change_id=?1 AND state='in_flight'",
                params![change_id, now, error_code],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

impl ScopedDb {
    pub fn read_checkpoint(
        &self,
        lease: &ScopeLease,
    ) -> Result<(String, Option<String>), SyncError> {
        self.validate_lease(lease)?;
        self.db().read_checkpoint(lease)
    }

    pub fn bind_sync_epoch(&self, lease: &ScopeLease, epoch: &str) -> Result<(), SyncError> {
        self.validate_lease(lease)?;
        self.db().bind_sync_epoch(epoch)
    }

    pub fn replace_sync_epoch_after_reset(
        &self,
        lease: &ScopeLease,
        epoch: &str,
    ) -> Result<(), SyncError> {
        self.validate_lease(lease)?;
        self.db().replace_sync_epoch_after_reset(epoch)
    }

    pub fn requeue_push_batch(
        &self,
        lease: &ScopeLease,
        change_ids: &[String],
        error_code: &str,
    ) -> Result<(), SyncError> {
        self.validate_lease(lease)?;
        self.db().requeue_push_batch(change_ids, error_code)
    }
}
