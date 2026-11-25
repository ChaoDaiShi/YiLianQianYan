use crate::modules::cloud::domain::SyncOperation;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn record(tx: &Transaction<'_>, id: &str, device: &str) -> rusqlite::Result<()> {
    let state = revision(tx, id)?;
    let base: i64 = tx.query_row(
        "SELECT MAX(COALESCE((SELECT revision FROM cloud_synced_entities WHERE entity_type='memory' AND entity_id=?1),0),
             COALESCE((SELECT MAX(remote_revision) FROM cloud_sync_conflicts WHERE entity_type='memory' AND entity_id=?1),0))",
        [id], |row| row.get(0),
    )?;
    let now = chrono::Utc::now().timestamp_millis();
    clear_payloads(tx, id)?;
    let Some((authoritative, generation, _)) = state else {
        tx.execute("INSERT INTO cloud_entity_revisions(entity_type,entity_id,fingerprint,deleted,updated_at) VALUES('memory',?1,'',1,?2)", params![id,now])?;
        return Ok(());
    };
    queue(tx, id, device, base.max(authoritative), generation, now)
}

fn clear_payloads(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1 AND operation='upsert'", [id])?;
    tx.execute(
        "DELETE FROM cloud_sync_conflicts WHERE entity_type='memory' AND entity_id=?1",
        [id],
    )?;
    tx.execute("UPDATE cloud_synced_entities SET payload_json=NULL,canonical_digest=NULL WHERE entity_type='memory' AND entity_id=?1", [id])?;
    Ok(())
}

pub(in crate::db) fn apply_authoritative_deletion(
    tx: &Transaction<'_>,
    id: &str,
    revision: i64,
    now: i64,
) -> rusqlite::Result<()> {
    let removed = tx.execute("DELETE FROM memories WHERE id=?1", [id])?;
    if removed > 0 {
        super::super::memory_activity::record_memory_removal(tx, now)?;
    }
    super::super::connectors::remove_connector_entity_on(tx, "memory", id)?;
    super::super::connectors::remove_connector_entity_on(tx, "knowledge", id)?;
    // Keep conflict decision metadata usable for idempotent resolution while
    // removing both content snapshots once the server deletion is accepted.
    tx.execute(
        "UPDATE cloud_sync_conflicts SET local_payload_json=NULL,remote_payload_json=NULL
        WHERE entity_type='memory' AND entity_id=?1",
        [id],
    )?;
    tx.execute("UPDATE cloud_synced_entities SET payload_json=NULL WHERE entity_type='memory' AND entity_id=?1", [id])?;
    tx.execute(
        "DELETE FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1",
        [id],
    )?;
    tx.execute("INSERT INTO cloud_entity_revisions(entity_type,entity_id,fingerprint,revision,authoritative_revision,deleted,updated_at)
        VALUES('memory',?1,'',?2,?2,1,?3) ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint='',
        revision=MAX(authoritative_revision,excluded.revision),authoritative_revision=MAX(authoritative_revision,excluded.revision),deleted=1,updated_at=excluded.updated_at", params![id,revision,now])?;
    tx.execute("UPDATE cloud_synced_entities SET revision=MAX(revision,?2),deleted=1,updated_at=?3 WHERE entity_type='memory' AND entity_id=?1", params![id,revision,now])?;
    Ok(())
}

fn revision(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<Option<(i64, i64, bool)>> {
    tx.query_row("SELECT authoritative_revision,local_generation,deleted FROM cloud_entity_revisions WHERE entity_type='memory' AND entity_id=?1", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()
}

fn queue(
    tx: &Transaction<'_>,
    id: &str,
    device: &str,
    base: i64,
    generation: i64,
    now: i64,
) -> rusqlite::Result<()> {
    let next = base.checked_add(1).ok_or(rusqlite::Error::InvalidQuery)?;
    let generation = generation
        .checked_add(1)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    tx.execute(
        "DELETE FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1",
        [id],
    )?;
    tx.execute(
        "INSERT INTO cloud_sync_outbox(change_id,entity_type,entity_id,base_revision,revision,operation,device_id,state,attempts,local_generation,created_at,updated_at)
         VALUES(?1,'memory',?2,?3,?4,'delete',?5,'pending',0,?6,?7,?7)",
        params![uuid::Uuid::new_v4().to_string(), id, base, next, device, generation, now],
    )?;
    tx.execute("UPDATE cloud_entity_revisions SET fingerprint='',revision=?2,authoritative_revision=?3,local_generation=?4,deleted=1,updated_at=?5 WHERE entity_type='memory' AND entity_id=?1", params![id,next,base,generation,now])?;
    Ok(())
}

pub(super) fn handle_remote(
    tx: &Transaction<'_>,
    id: &str,
    remote_revision: i64,
    operation: SyncOperation,
    now: i64,
) -> rusqlite::Result<bool> {
    let Some((authoritative, generation, true)) = revision(tx, id)? else {
        return Ok(false);
    };
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id=?1)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Ok(false);
    }
    if remote_revision < authoritative {
        return Ok(true);
    }
    if operation == SyncOperation::Delete {
        tx.execute("DELETE FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1 AND operation='delete' AND base_revision<=?2", params![id, remote_revision])?;
        tx.execute("UPDATE cloud_entity_revisions SET revision=?2,authoritative_revision=?2,updated_at=?3 WHERE entity_type='memory' AND entity_id=?1", params![id, remote_revision, now])?;
        tx.execute("UPDATE cloud_synced_entities SET revision=?2,deleted=1,payload_json=NULL,canonical_digest=NULL,updated_at=?3 WHERE entity_type='memory' AND entity_id=?1", params![id, remote_revision, now])?;
    } else if remote_revision > authoritative {
        let device: Option<String> = tx
            .query_row(
                "SELECT device_id FROM cloud_identity WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(device) = device {
            queue(tx, id, &device, remote_revision, generation, now)?;
        }
    }
    Ok(true)
}
