use parking_lot::RwLock;
use rusqlite::OptionalExtension;
use std::sync::Arc;

use crate::db::Database;
use crate::modules::cloud::contracts::{ScopeError, ScopeLease, SyncError};

/// ScopedDb wraps an isolated Database, enforcing scope lease matching
/// and providing a read/write gate latch that blocks runtime quiescing / profile switches
/// while a database write transaction is active.
#[derive(Clone)]
pub struct ScopedDb {
    db: Database,
    lease: ScopeLease,
    gate: Arc<RwLock<()>>,
}

impl std::fmt::Debug for ScopedDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedDb")
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}

impl ScopedDb {
    pub fn new(db: Database, lease: ScopeLease) -> Self {
        Self {
            db,
            lease,
            gate: Arc::new(RwLock::new(())),
        }
    }

    pub fn new_with_gate(db: Database, lease: ScopeLease, gate: Arc<RwLock<()>>) -> Self {
        Self { db, lease, gate }
    }

    pub fn lease(&self) -> &ScopeLease {
        &self.lease
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn gate(&self) -> &Arc<RwLock<()>> {
        &self.gate
    }

    /// Validate that the provided lease matches this ScopedDb's immutable scope_id and generation.
    pub fn validate_lease(&self, lease: &ScopeLease) -> Result<(), SyncError> {
        if self.lease.scope_id != lease.scope_id || self.lease.generation != lease.generation {
            return Err(SyncError::ScopeChanged);
        }
        Ok(())
    }

    /// Execute a write closure within an active read lock on the gate latch.
    /// This prevents any concurrent runtime quiesce/profile switch (which acquires a write lock on the gate)
    /// from occurring until this write closure completes.
    pub fn with_active_write<T, F>(&self, lease: &ScopeLease, f: F) -> Result<T, SyncError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<T, SyncError>,
    {
        self.validate_lease(lease)?;
        let _guard = self.gate.read();
        self.validate_lease(lease)?;
        let mut conn = self.db.conn();
        f(&mut conn)
    }

    /// Execute a write closure within an immediate transaction and active read lock on the gate latch.
    pub fn with_write_tx<T, F>(&self, lease: &ScopeLease, f: F) -> Result<T, SyncError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, SyncError>,
    {
        self.with_active_write(lease, |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let result = f(&tx)?;
            tx.commit()?;
            Ok(result)
        })
    }

    /// Execute a read closure within an active read transaction.
    pub fn with_read_tx<T, F>(&self, lease: &ScopeLease, f: F) -> Result<T, SyncError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, SyncError>,
    {
        self.validate_lease(lease)?;
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        f(&tx)
    }

    /// Validates that the metadata in table `cloud_profile_metadata` matches this ScopedDb's scope_id.
    /// Fail closed if not present or mismatched.
    pub fn validate_stored_scope(&self) -> Result<(), SyncError> {
        let conn = self.db.conn();
        let stored_scope: Option<String> = conn
            .query_row(
                "SELECT scope_id FROM cloud_profile_metadata WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        match stored_scope {
            Some(ref stored) if stored == &self.lease.scope_id => Ok(()),
            Some(ref mismatch) => Err(SyncError::Scope(ScopeError::ScopeMismatch(format!(
                "Stored scope {mismatch} does not match active lease scope {}",
                self.lease.scope_id
            )))),
            None => Err(SyncError::Scope(ScopeError::ScopeMismatch(format!(
                "No profile metadata found in database for scope {}",
                self.lease.scope_id
            )))),
        }
    }

    /// Initialize or update profile metadata record in SQLite
    pub fn init_profile_metadata(
        &self,
        service_origin: &str,
        account_id: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.db.conn();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO cloud_profile_metadata (singleton, scope_id, service_origin, account_id, generation, created_at, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(singleton) DO UPDATE SET
                scope_id = excluded.scope_id,
                service_origin = excluded.service_origin,
                account_id = excluded.account_id,
                generation = excluded.generation,
                updated_at = excluded.updated_at",
            rusqlite::params![
                self.lease.scope_id,
                service_origin,
                account_id,
                self.lease.generation,
                now,
            ],
        )?;
        Ok(())
    }
}
