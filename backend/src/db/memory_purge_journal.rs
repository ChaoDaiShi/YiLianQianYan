//! Body-free deletion intents kept outside the database snapshots they protect.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use super::{
    cloud::memory_purge::apply_authoritative_deletion, memories::delete_memory_on,
    memory_governance::content_hash, Database,
};

#[derive(Clone, Copy)]
pub(super) enum PurgeKind {
    Memory,
    Activity,
    RemoteMemory,
}

impl PurgeKind {
    fn code(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Activity => "activity",
            Self::RemoteMemory => "remote_memory",
        }
    }

    fn records(self) -> &'static str {
        match self {
            Self::Memory | Self::RemoteMemory => {
                "SELECT id FROM memories
                UNION SELECT entity_id FROM cloud_entity_revisions WHERE entity_type='memory'
                UNION SELECT entity_id FROM cloud_synced_entities WHERE entity_type='memory'
                UNION SELECT entity_id FROM cloud_sync_outbox WHERE entity_type='memory'
                UNION SELECT entity_id FROM cloud_sync_conflicts WHERE entity_type='memory'"
            }
            Self::Activity => "SELECT id FROM memory_activity",
        }
    }
}

pub(super) fn journal_path(db_path: &Path) -> Option<PathBuf> {
    if db_path == Path::new(":memory:") {
        return None;
    }
    // Re-adopting the legacy guest database after loss of its registry can
    // create a new guest directory. Keep intents with the legacy source.
    let legacy = db_path.parent().and_then(|profile| {
        let name = profile
            .file_name()?
            .to_str()?
            .strip_prefix("guest-migrated-")?;
        uuid::Uuid::parse_str(name).ok()?;
        let profiles = profile.parent()?;
        if db_path.file_name()? != "app.db"
            || profiles.file_name()? != crate::modules::cloud::profile::PROFILES_DIRNAME
        {
            return None;
        }
        Some(profiles.parent()?.join("yilianqianyan-v1.db"))
    });
    let journal_owner = legacy.as_deref().unwrap_or(db_path);
    let mut name = OsString::from(journal_owner.file_name()?);
    name.push(".memory-purge.sqlite");
    Some(journal_owner.with_file_name(name))
}

impl Database {
    /// Persist intent before committing the live transaction.
    pub(super) fn record_memory_purge(
        &self,
        kind: PurgeKind,
        ids: &[&str],
    ) -> rusqlite::Result<()> {
        persist_intents(
            self.memory_purge_journal.as_deref(),
            kind,
            ids.iter().map(|id| (*id, 0)),
        )
    }

    pub(super) fn replay_memory_purges(&self) -> rusqlite::Result<()> {
        let Some(path) = self.memory_purge_journal.as_deref() else {
            return Ok(());
        };
        if !path
            .try_exists()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
        {
            return Ok(());
        }
        // Open writable so SQLite can recover an interrupted journal commit.
        let journal =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        if !super::migrations::table_exists(&journal, "purge_intents")? {
            return Ok(());
        }
        let identity = self.get_or_create_cloud_identity()?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (id, revision) in replay_ids(&transaction, &journal, PurgeKind::RemoteMemory)? {
            apply_authoritative_deletion(
                &transaction,
                &id,
                revision,
                chrono::Utc::now().timestamp_millis(),
            )?;
        }
        for (id, _) in replay_ids(&transaction, &journal, PurgeKind::Memory)? {
            if delete_memory_on(&transaction, &id, &identity.device_id)? == 0 {
                let pending_payloads: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM cloud_sync_outbox WHERE entity_type='memory' AND entity_id=?1 AND operation='upsert')
                       OR EXISTS(SELECT 1 FROM cloud_synced_entities WHERE entity_type='memory' AND entity_id=?1 AND payload_json IS NOT NULL)
                       OR EXISTS(SELECT 1 FROM cloud_sync_conflicts WHERE entity_type='memory' AND entity_id=?1 AND (local_payload_json IS NOT NULL OR remote_payload_json IS NOT NULL))",
                    [&id], |row| row.get(0),
                )?;
                if pending_payloads {
                    super::cloud::capture::record_local_deletion_tx(
                        &transaction,
                        "memory",
                        &id,
                        &identity.device_id,
                    )?;
                }
            }
        }
        for (id, _) in replay_ids(&transaction, &journal, PurgeKind::Activity)? {
            // This also invokes the proposal cleanup trigger for the message.
            transaction.execute("DELETE FROM memory_activity WHERE id=?1", [id])?;
        }
        transaction.commit()
    }
}

// Called only by the trusted authoritative projection. The main SQLite path
// comes from its existing transaction so there is no second main DB lock.
pub(super) fn record_remote_memory_purge(
    tx: &Transaction<'_>,
    id: &str,
    revision: i64,
) -> rusqlite::Result<()> {
    let path = tx
        .path()
        .filter(|path| !path.is_empty())
        .and_then(|path| journal_path(Path::new(path)));
    persist_intents(
        path.as_deref(),
        PurgeKind::RemoteMemory,
        std::iter::once((id, revision)),
    )
}

fn persist_intents<'a>(
    path: Option<&Path>,
    kind: PurgeKind,
    intents: impl Iterator<Item = (&'a str, i64)>,
) -> rusqlite::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let mut intents = intents.peekable();
    if intents.peek().is_none() {
        return Ok(());
    }
    let mut journal = Connection::open(path)?;
    journal.pragma_update(None, "synchronous", "FULL")?;
    let transaction = journal.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS purge_intents (
            kind TEXT NOT NULL, id_hash TEXT NOT NULL, revision INTEGER NOT NULL,
            PRIMARY KEY (kind,id_hash)
        );",
    )?;
    for (id, revision) in intents {
        transaction.execute(
            "INSERT INTO purge_intents(kind,id_hash,revision) VALUES(?1,?2,?3)
             ON CONFLICT(kind,id_hash) DO UPDATE SET revision=MAX(revision,excluded.revision)",
            params![kind.code(), content_hash(id), revision],
        )?;
    }
    transaction.commit()
}

fn replay_ids(
    tx: &Transaction<'_>,
    journal: &Connection,
    kind: PurgeKind,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut rows = tx.prepare(kind.records())?;
    let mut intent =
        journal.prepare("SELECT revision FROM purge_intents WHERE kind=?1 AND id_hash=?2")?;
    let mut ids = Vec::new();
    for row in rows.query_map([], |row| row.get::<_, String>(0))? {
        let id = row?;
        if let Some(revision) = intent
            .query_row(params![kind.code(), content_hash(&id)], |row| row.get(0))
            .optional()?
        {
            ids.push((id, revision));
        }
    }
    Ok(ids)
}
