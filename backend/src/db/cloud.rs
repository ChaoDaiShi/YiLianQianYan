use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::cloud::domain::{
    CloudSyncStatus, IdentitySnapshot, IdentityStatus, NewSyncChange, SyncOperation, SyncOutboxItem,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const CLOUD_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2000,
    "2000_v11_cloud_client",
    MigrationOwner::V11Cloud,
    "CREATE TABLE cloud_identity (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        local_identity_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        device_label TEXT NOT NULL,
        account_id TEXT,
        email TEXT,
        status TEXT NOT NULL CHECK (status IN ('guest','authenticated','offline_authenticated','closing')),
        cloud_base_url TEXT,
        sync_existing_choice TEXT NOT NULL DEFAULT 'undecided' CHECK (sync_existing_choice IN ('undecided','sync','local_only')),
        access_token_ref_json TEXT,
        refresh_token_ref_json TEXT,
        current_session_id TEXT,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE cloud_sync_outbox (
        change_id TEXT PRIMARY KEY,
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
        revision INTEGER NOT NULL CHECK (revision > 0),
        operation TEXT NOT NULL CHECK (operation IN ('upsert','delete')),
        payload_json TEXT,
        device_id TEXT NOT NULL,
        state TEXT NOT NULL CHECK (state IN ('pending','error')),
        attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
        next_retry_at INTEGER,
        last_error_code TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX idx_cloud_sync_outbox_state_retry ON cloud_sync_outbox(state, next_retry_at, created_at);
    CREATE TABLE cloud_sync_cursor (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        cursor TEXT,
        last_success_at INTEGER,
        last_error_code TEXT,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE cloud_session_metadata (
        session_id TEXT PRIMARY KEY,
        account_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        last_seen_at INTEGER NOT NULL
    );
    CREATE TABLE cloud_sync_conflicts (
        id TEXT PRIMARY KEY,
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        local_revision INTEGER NOT NULL,
        remote_revision INTEGER NOT NULL,
        local_payload_json TEXT,
        remote_payload_json TEXT,
        status TEXT NOT NULL CHECK (status IN ('open','resolved')),
        created_at INTEGER NOT NULL,
        resolved_at INTEGER
    );
    CREATE INDEX idx_cloud_sync_conflicts_status ON cloud_sync_conflicts(status, created_at);
    CREATE TABLE cloud_lifecycle_jobs (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('export','deletion')),
        status TEXT NOT NULL CHECK (status IN ('queued','running','completed','failed')),
        progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
        result_ref TEXT,
        error_code TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );",
);

const CLOUD_SEARCH_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2001,
    "2001_v11_search_config",
    MigrationOwner::V11Cloud,
    "CREATE TABLE cloud_search_config (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        provider TEXT NOT NULL DEFAULT 'anysearch',
        base_url TEXT NOT NULL DEFAULT 'https://api.anysearch.com',
        api_key_ref_json TEXT,
        updated_at INTEGER NOT NULL
    );",
);

const CLOUD_SYNC_ENTITY_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2002,
    "2002_v11_synced_entities",
    MigrationOwner::V11Cloud,
    "CREATE TABLE cloud_synced_entities (
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        revision INTEGER NOT NULL,
        deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1)),
        payload_json TEXT,
        server_sequence INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (entity_type, entity_id)
    );
    CREATE INDEX idx_cloud_synced_entities_sequence ON cloud_synced_entities(server_sequence);",
);

impl Database {
    pub fn initialize_cloud_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migrations(&[CLOUD_SCHEMA, CLOUD_SEARCH_SCHEMA, CLOUD_SYNC_ENTITY_SCHEMA])
    }

    pub fn get_or_create_cloud_identity(&self) -> Result<IdentitySnapshot, rusqlite::Error> {
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row = transaction
            .query_row(
                "SELECT local_identity_id, device_id, account_id, email, status FROM cloud_identity WHERE singleton=1",
                [],
                map_identity,
            )
            .optional()?;
        if let Some(identity) = row {
            transaction.commit()?;
            return Ok(identity);
        }
        let local_identity_id = uuid::Uuid::new_v4().to_string();
        let device_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        transaction.execute(
            "INSERT INTO cloud_identity (singleton, local_identity_id, device_id, device_label, status, updated_at)
             VALUES (1, ?1, ?2, 'Windows device', 'guest', ?3)",
            params![local_identity_id, device_id, now],
        )?;
        transaction.commit()?;
        Ok(IdentitySnapshot::guest(local_identity_id, device_id))
    }

    pub fn cloud_identity(&self) -> Result<Option<IdentitySnapshot>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT local_identity_id, device_id, account_id, email, status FROM cloud_identity WHERE singleton=1",
            [],
            map_identity,
        )
        .optional()
    }

    pub fn save_authenticated_cloud_session(
        &self,
        account_id: &str,
        email: &str,
        session_id: &str,
        expires_at: i64,
        access_token_ref: &crate::integrations::secret::SecretRef,
        refresh_token_ref: &crate::integrations::secret::SecretRef,
    ) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let access_ref = serde_json::to_string(access_token_ref)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let refresh_ref = serde_json::to_string(refresh_token_ref)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET account_id=?1, email=?2, status='authenticated',
                    access_token_ref_json=?3, refresh_token_ref_json=?4,
                    current_session_id=?5, updated_at=?6 WHERE singleton=1",
            params![account_id, email, access_ref, refresh_ref, session_id, now],
        )?;
        let device_id: String = conn.query_row(
            "SELECT device_id FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO cloud_session_metadata (session_id, account_id, device_id, expires_at, last_seen_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(session_id) DO UPDATE SET expires_at=excluded.expires_at, last_seen_at=excluded.last_seen_at",
            params![session_id, account_id, device_id, expires_at, now],
        )?;
        Ok(())
    }

    pub fn save_cloud_configuration(
        &self,
        cloud_base_url: &str,
        sync_existing_choice: &str,
    ) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET cloud_base_url=?1, sync_existing_choice=?2, updated_at=?3 WHERE singleton=1",
            params![cloud_base_url, sync_existing_choice, now],
        )?;
        Ok(())
    }

    pub fn cloud_connection_config(&self) -> Result<Option<(String, String)>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT cloud_base_url, sync_existing_choice FROM cloud_identity
             WHERE singleton=1 AND cloud_base_url IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    }

    pub fn clear_authenticated_cloud_session(&self) -> Result<(), rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET account_id=NULL, email=NULL, status='guest',
                    access_token_ref_json=NULL, refresh_token_ref_json=NULL,
                    current_session_id=NULL, updated_at=?1 WHERE singleton=1",
            [now],
        )?;
        Ok(())
    }

    pub fn enqueue_sync_change(&self, change: NewSyncChange) -> Result<SyncOutboxItem, rusqlite::Error> {
        let identity = self.get_or_create_cloud_identity()?;
        let change_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let payload_json = change.payload.as_ref().map(serde_json::to_string).transpose()
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
        self.sync_outbox_item(&change_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn sync_outbox_item(&self, change_id: &str) -> Result<Option<SyncOutboxItem>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT change_id, entity_type, entity_id, base_revision, revision, operation,
                    payload_json, device_id, state, attempts, next_retry_at, last_error_code,
                    created_at, updated_at
             FROM cloud_sync_outbox WHERE change_id=?1",
            [change_id],
            map_outbox,
        ).optional()
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

    pub fn pending_sync_changes(&self, limit: usize) -> Result<Vec<SyncOutboxItem>, rusqlite::Error> {
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
        conn.execute("DELETE FROM cloud_sync_outbox WHERE change_id=?1", [change_id])?;
        Ok(())
    }

    pub fn record_sync_conflict(
        &self,
        item: &SyncOutboxItem,
        authoritative_revision: i64,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let local_payload = item.payload.as_ref().map(serde_json::to_string).transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_sync_conflicts (
                id, entity_type, entity_id, local_revision, remote_revision,
                local_payload_json, remote_payload_json, status, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,NULL,'open',?7)",
            params![uuid::Uuid::new_v4().to_string(), item.entity_type, item.entity_id,
                item.revision, authoritative_revision, local_payload, now],
        )?;
        conn.execute("DELETE FROM cloud_sync_outbox WHERE change_id=?1", [&item.change_id])?;
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
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let payload = payload.map(serde_json::to_string).transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_synced_entities (
                entity_type, entity_id, revision, deleted, payload_json, server_sequence, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(entity_type,entity_id) DO UPDATE SET
                revision=excluded.revision, deleted=excluded.deleted,
                payload_json=excluded.payload_json, server_sequence=excluded.server_sequence,
                updated_at=excluded.updated_at
             WHERE excluded.revision >= cloud_synced_entities.revision",
            params![entity_type, entity_id, revision,
                if operation == SyncOperation::Delete { 1 } else { 0 }, payload, sequence, now],
        )?;
        Ok(())
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

    pub fn cloud_search_config(
        &self,
    ) -> Result<(String, String, Option<crate::integrations::secret::SecretRef>), rusqlite::Error> {
        let conn = self.conn();
        let row: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT provider, base_url, api_key_ref_json FROM cloud_search_config WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((provider, base_url, secret_json)) = row else {
            return Ok(("anysearch".into(), "https://api.anysearch.com".into(), None));
        };
        let secret_ref = secret_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            ))?;
        Ok((provider, base_url, secret_ref))
    }

    pub fn save_cloud_search_config(
        &self,
        base_url: &str,
        secret_ref: Option<&crate::integrations::secret::SecretRef>,
    ) -> Result<(), rusqlite::Error> {
        let secret_json = secret_ref
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_search_config (singleton, provider, base_url, api_key_ref_json, updated_at)
             VALUES (1, 'anysearch', ?1, ?2, ?3)
             ON CONFLICT(singleton) DO UPDATE SET base_url=excluded.base_url,
                api_key_ref_json=excluded.api_key_ref_json, updated_at=excluded.updated_at",
            params![base_url, secret_json, now],
        )?;
        Ok(())
    }
}

fn map_identity(row: &rusqlite::Row<'_>) -> rusqlite::Result<IdentitySnapshot> {
    let status: String = row.get(4)?;
    let status = match status.as_str() {
        "guest" => IdentityStatus::Guest,
        "authenticated" => IdentityStatus::Authenticated,
        "offline_authenticated" => IdentityStatus::OfflineAuthenticated,
        "closing" => IdentityStatus::Closing,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(IdentitySnapshot {
        local_identity_id: row.get(0)?,
        device_id: row.get(1)?,
        account_id: row.get(2)?,
        email: row.get(3)?,
        status,
        local_core_available: true,
        cloud_enabled: status != IdentityStatus::Guest,
    })
}

fn map_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncOutboxItem> {
    let operation: String = row.get(5)?;
    let payload: Option<String> = row.get(6)?;
    let payload = payload
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(error),
        ))?;
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
