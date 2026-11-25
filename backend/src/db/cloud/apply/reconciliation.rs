use crate::db::Database;
use crate::modules::cloud::contracts::SyncError;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

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

impl Database {
    pub fn run_reliability_reconciliation_once(
        &self,
    ) -> Result<Option<ReconciliationReport>, SyncError> {
        let version: i64 = self.conn().query_row(
            "SELECT reconciliation_version FROM cloud_reliability_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        if version >= 1 {
            return Ok(None);
        }
        let report = reconcile_skipped_history(self)?;
        self.conn().execute(
            "UPDATE cloud_reliability_state SET reconciliation_version=1, updated_at=?1 WHERE singleton=1",
            [chrono::Utc::now().timestamp_millis()],
        )?;
        Ok(Some(report))
    }

    pub fn reconcile_skipped_history(&self) -> Result<ReconciliationReport, SyncError> {
        reconcile_skipped_history(self)
    }
}
