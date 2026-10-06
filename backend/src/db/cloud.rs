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

const CLOUD_LOCAL_REVISION_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2003,
    "2003_v11_local_sync_revisions",
    MigrationOwner::V11Cloud,
    "CREATE TABLE cloud_entity_revisions (
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        revision INTEGER NOT NULL CHECK (revision > 0),
        deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1)),
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (entity_type, entity_id)
    );",
);

const CLOUD_CONFLICT_OPERATION_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2004,
    "2004_v11_conflict_operation",
    MigrationOwner::V11Cloud,
    "ALTER TABLE cloud_sync_conflicts ADD COLUMN remote_operation TEXT NOT NULL DEFAULT 'upsert'
     CHECK (remote_operation IN ('upsert','delete'));",
);

impl Database {
    pub fn initialize_cloud_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migrations(&[
            CLOUD_SCHEMA,
            CLOUD_SEARCH_SCHEMA,
            CLOUD_SYNC_ENTITY_SCHEMA,
            CLOUD_LOCAL_REVISION_SCHEMA,
            CLOUD_CONFLICT_OPERATION_SCHEMA,
        ])
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

    pub fn cloud_current_session_id(&self) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn();
        conn.query_row(
            "SELECT current_session_id FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.flatten())
    }

    pub fn set_sync_existing_choice(&self, choice: &str) -> Result<(), rusqlite::Error> {
        if !matches!(choice, "undecided" | "sync" | "local_only") {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid sync existing choice".into(),
            ));
        }
        self.get_or_create_cloud_identity()?;
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET sync_existing_choice=?1,updated_at=?2 WHERE singleton=1",
            params![choice, now],
        )?;
        Ok(())
    }

    pub fn cloud_sync_existing_choice(&self) -> Result<String, rusqlite::Error> {
        self.get_or_create_cloud_identity()?;
        let conn = self.conn();
        conn.query_row(
            "SELECT sync_existing_choice FROM cloud_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )
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

    pub fn mark_cloud_identity_closing(&self) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn();
        conn.execute(
            "UPDATE cloud_identity SET status='closing',updated_at=?1 WHERE singleton=1",
            [now],
        )?;
        Ok(())
    }

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
        apply_remote_entity(
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
            apply_remote_entity(
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

    pub fn save_cloud_lifecycle_job(
        &self,
        job: &crate::modules::cloud::domain::LifecycleJob,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO cloud_lifecycle_jobs (id,kind,status,progress,result_ref,error_code,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,progress=excluded.progress,
                result_ref=excluded.result_ref,error_code=excluded.error_code,updated_at=excluded.updated_at",
            params![job.id,job.kind,job.status,job.progress,job.result_ref,job.error_code,job.created_at,job.updated_at],
        )?;
        Ok(())
    }

    pub fn capture_sync_candidates(&self) -> Result<usize, rusqlite::Error> {
        use sha2::{Digest, Sha256};
        use std::collections::HashSet;

        let identity = self.get_or_create_cloud_identity()?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sources = collect_sync_sources(&transaction)?;
        let current_keys = sources
            .iter()
            .map(|source| (source.entity_type.to_string(), source.entity_id.clone()))
            .collect::<HashSet<_>>();
        let now = chrono::Utc::now().timestamp_millis();
        let mut captured = 0usize;

        for source in sources {
            let payload = crate::modules::cloud::domain::sanitize_sync_payload(source.payload);
            let payload_json = serde_json::to_string(&payload)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            let fingerprint = Sha256::digest(payload_json.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let existing: Option<(String, i64, bool)> = transaction
                .query_row(
                    "SELECT fingerprint,revision,deleted FROM cloud_entity_revisions
                     WHERE entity_type=?1 AND entity_id=?2",
                    params![source.entity_type, source.entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)),
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|(stored, _, deleted)| stored == &fingerprint && !deleted)
            {
                continue;
            }
            let base_revision = existing.as_ref().map(|item| item.1).unwrap_or(0);
            let revision = base_revision + 1;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,
                    device_id,state,attempts,created_at,updated_at
                 ) VALUES (?1,?2,?3,?4,?5,'upsert',?6,?7,'pending',0,?8,?8)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    source.entity_type,
                    source.entity_id,
                    base_revision,
                    revision,
                    payload_json,
                    identity.device_id,
                    now
                ],
            )?;
            transaction.execute(
                "INSERT INTO cloud_entity_revisions (entity_type,entity_id,fingerprint,revision,deleted,updated_at)
                 VALUES (?1,?2,?3,?4,0,?5)
                 ON CONFLICT(entity_type,entity_id) DO UPDATE SET fingerprint=excluded.fingerprint,
                    revision=excluded.revision,deleted=0,updated_at=excluded.updated_at",
                params![source.entity_type,source.entity_id,fingerprint,revision,now],
            )?;
            captured += 1;
        }

        let tracked = {
            let mut statement = transaction.prepare(
                "SELECT entity_type,entity_id,revision FROM cloud_entity_revisions WHERE deleted=0",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (entity_type, entity_id, base_revision) in tracked {
            if current_keys.contains(&(entity_type.clone(), entity_id.clone())) {
                continue;
            }
            let revision = base_revision + 1;
            transaction.execute(
                "INSERT INTO cloud_sync_outbox (
                    change_id,entity_type,entity_id,base_revision,revision,operation,payload_json,
                    device_id,state,attempts,created_at,updated_at
                 ) VALUES (?1,?2,?3,?4,?5,'delete',NULL,?6,'pending',0,?7,?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    entity_type,
                    entity_id,
                    base_revision,
                    revision,
                    identity.device_id,
                    now
                ],
            )?;
            transaction.execute(
                "UPDATE cloud_entity_revisions SET fingerprint='',revision=?3,deleted=1,updated_at=?4
                 WHERE entity_type=?1 AND entity_id=?2",
                params![entity_type,entity_id,revision,now],
            )?;
            captured += 1;
        }
        transaction.commit()?;
        Ok(captured)
    }

    pub fn cloud_search_config(
        &self,
    ) -> Result<
        (
            String,
            String,
            Option<crate::integrations::secret::SecretRef>,
        ),
        rusqlite::Error,
    > {
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
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
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

struct SyncSource {
    entity_type: &'static str,
    entity_id: String,
    payload: serde_json::Value,
}

fn collect_sync_sources(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Vec<SyncSource>, rusqlite::Error> {
    let mut sources = Vec::new();
    {
        let mut statement = transaction
            .prepare("SELECT id,title,run_status,created_at,updated_at FROM conversations")?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "conversation",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"title":row.get::<_,String>(1)?,"run_status":row.get::<_,String>(2)?,"created_at":row.get::<_,i64>(3)?,"updated_at":row.get::<_,i64>(4)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,conversation_id,role,content,created_at FROM messages
             WHERE role IN ('user','assistant','system')",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "message",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"conversation_id":row.get::<_,String>(1)?,"role":row.get::<_,String>(2)?,"content":row.get::<_,String>(3)?,"created_at":row.get::<_,i64>(4)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,content,category,source,source_conversation_id,metadata,created_at,updated_at FROM memories",
        )?;
        let rows = statement.query_map([], |row| {
            let metadata: Option<String> = row.get(5)?;
            Ok(SyncSource {
                entity_type: "memory",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"content":row.get::<_,String>(1)?,"category":row.get::<_,String>(2)?,"source":row.get::<_,String>(3)?,"source_conversation_id":row.get::<_,Option<String>>(4)?,"metadata":metadata.map(parse_json_or_string),"created_at":row.get::<_,i64>(6)?,"updated_at":row.get::<_,i64>(7)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at FROM agent_definitions",
        )?;
        let rows = statement.query_map([], |row| {
            let allowed: String = row.get(4)?;
            let capabilities: String = row.get(6)?;
            Ok(SyncSource {
                entity_type: "agent_profile",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"name":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"instructions":row.get::<_,String>(3)?,"allowed_tools":parse_json_or_string(allowed),"model":row.get::<_,Option<String>>(5)?,"capabilities":parse_json_or_string(capabilities),"max_iterations":row.get::<_,i64>(7)?,"enabled":row.get::<_,i64>(8)?!=0,"source":row.get::<_,String>(9)?,"created_at":row.get::<_,i64>(10)?,"updated_at":row.get::<_,i64>(11)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,name,description,schema_version,definition_json,created_at,updated_at FROM workflow_graphs",
        )?;
        let rows = statement.query_map([], |row| {
            let definition: String = row.get(4)?;
            Ok(SyncSource {
                entity_type: "workflow_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"name":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"schema_version":row.get::<_,i64>(3)?,"definition":parse_json_or_string(definition),"created_at":row.get::<_,Option<i64>>(5)?,"updated_at":row.get::<_,Option<i64>>(6)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    {
        let mut statement = transaction.prepare(
            "SELECT id,workspace_id,task_id,task_execution_id,name,artifact_type,mime_type,size,summary,created_at,updated_at FROM artifacts",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SyncSource {
                entity_type: "artifact_metadata",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"workspace_id":row.get::<_,String>(1)?,"task_id":row.get::<_,String>(2)?,"task_execution_id":row.get::<_,String>(3)?,"name":row.get::<_,String>(4)?,"artifact_type":row.get::<_,String>(5)?,"mime_type":row.get::<_,Option<String>>(6)?,"size":row.get::<_,Option<i64>>(7)?,"summary":row.get::<_,String>(8)?,"created_at":row.get::<_,i64>(9)?,"updated_at":row.get::<_,i64>(10)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    if sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? {
        let mut statement = transaction.prepare(
            "SELECT graph_id,graph_revision,graph_json,node_states_json,updated_at FROM task_world_supervisor_snapshots",
        )?;
        let rows = statement.query_map([], |row| {
            let graph: String = row.get(2)?;
            let states: String = row.get(3)?;
            Ok(SyncSource {
                entity_type: "task_graph",
                entity_id: row.get(0)?,
                payload: serde_json::json!({"graph_revision":row.get::<_,i64>(1)?,"graph":parse_json_or_string(graph),"node_states":parse_json_or_string(states),"updated_at":row.get::<_,i64>(4)?}),
            })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    if sqlite_table_exists(transaction, "task_canvas_views")? {
        let mut statement = transaction.prepare(
            "SELECT graph_id,view_revision,graph_revision_seen,viewport_json,node_layouts_json,selection_json,groups_json,updated_at FROM task_canvas_views",
        )?;
        let rows = statement.query_map([], |row| {
            let viewport:String=row.get(3)?;let layouts:String=row.get(4)?;let selection:String=row.get(5)?;let groups:String=row.get(6)?;
            Ok(SyncSource { entity_type:"canvas_view",entity_id:row.get(0)?,payload:serde_json::json!({"view_revision":row.get::<_,i64>(1)?,"graph_revision_seen":row.get::<_,i64>(2)?,"viewport":parse_json_or_string(viewport),"node_layouts":parse_json_or_string(layouts),"selection":parse_json_or_string(selection),"groups":parse_json_or_string(groups),"updated_at":row.get::<_,i64>(7)?}) })
        })?;
        sources.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    Ok(sources)
}

fn parse_json_or_string(value: String) -> serde_json::Value {
    serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value))
}

fn sqlite_table_exists(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
) -> Result<bool, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
}

fn apply_remote_entity(
    transaction: &rusqlite::Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    operation: SyncOperation,
    payload: Option<&serde_json::Value>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if operation == SyncOperation::Delete {
        match entity_type {
            "conversation" => {
                transaction
                    .execute("DELETE FROM messages WHERE conversation_id=?1", [entity_id])?;
                transaction.execute("DELETE FROM conversations WHERE id=?1", [entity_id])?;
            }
            "message" => {
                transaction.execute("DELETE FROM messages WHERE id=?1", [entity_id])?;
            }
            "memory" => {
                transaction.execute("DELETE FROM memories WHERE id=?1", [entity_id])?;
            }
            "agent_profile" => {
                transaction.execute("DELETE FROM agent_definitions WHERE id=?1", [entity_id])?;
            }
            "workflow_metadata" => {
                transaction.execute("DELETE FROM workflow_graphs WHERE id=?1", [entity_id])?;
            }
            "artifact_metadata" => {
                transaction.execute("DELETE FROM artifacts WHERE id=?1", [entity_id])?;
            }
            "task_graph"
                if sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? =>
            {
                transaction.execute(
                    "DELETE FROM task_world_supervisor_snapshots WHERE graph_id=?1",
                    [entity_id],
                )?;
            }
            "canvas_view" if sqlite_table_exists(transaction, "task_canvas_views")? => {
                transaction.execute(
                    "DELETE FROM task_canvas_views WHERE graph_id=?1",
                    [entity_id],
                )?;
            }
            _ => {}
        }
        return Ok(());
    }
    let Some(payload) = payload else {
        return Ok(());
    };
    let payload = crate::modules::cloud::domain::sanitize_sync_payload(payload.clone());
    match entity_type {
        "conversation" => {
            let title = json_text(&payload, "title").unwrap_or("恢复的对话");
            let status = json_text(&payload, "run_status")
                .filter(|value| {
                    matches!(
                        *value,
                        "idle" | "running" | "completed" | "failed" | "interrupted"
                    )
                })
                .unwrap_or("completed");
            let created = json_number(&payload, "created_at").unwrap_or(now);
            let updated = json_number(&payload, "updated_at").unwrap_or(now);
            transaction.execute("INSERT INTO conversations (id,title,run_status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET title=excluded.title,run_status=excluded.run_status,updated_at=excluded.updated_at",params![entity_id,title,status,created,updated])?;
        }
        "message" => {
            let conversation = json_text(&payload, "conversation_id").unwrap_or("cloud-restored");
            let created = json_number(&payload, "created_at").unwrap_or(now);
            transaction.execute("INSERT OR IGNORE INTO conversations (id,title,run_status,created_at,updated_at) VALUES (?1,'恢复的云端对话','completed',?2,?2)",params![conversation,created])?;
            transaction.execute("INSERT INTO messages (id,conversation_id,role,content,created_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,role=excluded.role,content=excluded.content,created_at=excluded.created_at",params![entity_id,conversation,json_text(&payload,"role").unwrap_or("assistant"),json_text(&payload,"content").unwrap_or(""),created])?;
            transaction.execute(
                "UPDATE conversations SET updated_at=MAX(updated_at,?2) WHERE id=?1",
                params![conversation, created],
            )?;
        }
        "memory" => {
            transaction.execute("INSERT INTO memories (id,content,category,source,source_conversation_id,metadata,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET content=excluded.content,category=excluded.category,source=excluded.source,source_conversation_id=excluded.source_conversation_id,metadata=excluded.metadata,updated_at=excluded.updated_at",params![entity_id,json_text(&payload,"content").unwrap_or(""),json_text(&payload,"category").unwrap_or("note"),json_text(&payload,"source").unwrap_or("cloud_sync"),json_optional_text(&payload,"source_conversation_id"),json_compact(payload.get("metadata")),json_number(&payload,"created_at").unwrap_or(now),json_number(&payload,"updated_at").unwrap_or(now)])?;
        }
        "agent_profile" => {
            transaction.execute("INSERT INTO agent_definitions (id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'cloud_sync',?10,?11) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,instructions=excluded.instructions,allowed_tools=excluded.allowed_tools,model=excluded.model,capabilities=excluded.capabilities,max_iterations=excluded.max_iterations,enabled=excluded.enabled,updated_at=excluded.updated_at",params![entity_id,json_text(&payload,"name").unwrap_or("云端智能体"),json_text(&payload,"description").unwrap_or(""),json_text(&payload,"instructions").unwrap_or(""),json_compact(payload.get("allowed_tools")).unwrap_or_else(||"[]".into()),json_optional_text(&payload,"model"),json_compact(payload.get("capabilities")).unwrap_or_else(||"[]".into()),json_number(&payload,"max_iterations").unwrap_or(10),if payload.get("enabled").and_then(|v|v.as_bool()).unwrap_or(true){1}else{0},json_number(&payload,"created_at").unwrap_or(now),json_number(&payload,"updated_at").unwrap_or(now)])?;
        }
        "workflow_metadata" => {
            transaction.execute("INSERT INTO workflow_graphs (id,name,description,schema_version,definition_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,schema_version=excluded.schema_version,definition_json=excluded.definition_json,updated_at=excluded.updated_at",params![entity_id,json_text(&payload,"name").unwrap_or("云端工作流"),json_text(&payload,"description").unwrap_or(""),json_number(&payload,"schema_version").unwrap_or(1),json_compact(payload.get("definition")).unwrap_or_else(||"{}".into()),json_optional_number(&payload,"created_at"),json_optional_number(&payload,"updated_at")])?;
        }
        "artifact_metadata" => {
            transaction.execute("INSERT INTO artifacts (id,workspace_id,task_id,task_execution_id,name,artifact_type,mime_type,size,summary,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET name=excluded.name,artifact_type=excluded.artifact_type,mime_type=excluded.mime_type,size=excluded.size,summary=excluded.summary,updated_at=excluded.updated_at",params![entity_id,json_text(&payload,"workspace_id").unwrap_or("cloud"),json_text(&payload,"task_id").unwrap_or("cloud"),json_text(&payload,"task_execution_id").unwrap_or("cloud"),json_text(&payload,"name").unwrap_or("云端成果"),json_text(&payload,"artifact_type").unwrap_or("other"),json_optional_text(&payload,"mime_type"),json_optional_number(&payload,"size"),json_text(&payload,"summary").unwrap_or(""),json_number(&payload,"created_at").unwrap_or(now),json_number(&payload,"updated_at").unwrap_or(now)])?;
        }
        "task_graph" if sqlite_table_exists(transaction, "task_world_supervisor_snapshots")? => {
            transaction.execute("INSERT INTO task_world_supervisor_snapshots (graph_id,graph_revision,graph_json,node_states_json,updated_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(graph_id) DO UPDATE SET graph_revision=excluded.graph_revision,graph_json=excluded.graph_json,node_states_json=excluded.node_states_json,updated_at=excluded.updated_at WHERE excluded.graph_revision>=task_world_supervisor_snapshots.graph_revision",params![entity_id,json_number(&payload,"graph_revision").unwrap_or(1),json_compact(payload.get("graph")).unwrap_or_else(||"{}".into()),json_compact(payload.get("node_states")).unwrap_or_else(||"[]".into()),json_number(&payload,"updated_at").unwrap_or(now)])?;
        }
        "canvas_view" if sqlite_table_exists(transaction, "task_canvas_views")? => {
            transaction.execute("INSERT INTO task_canvas_views (graph_id,view_revision,graph_revision_seen,viewport_json,node_layouts_json,selection_json,groups_json,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(graph_id) DO UPDATE SET view_revision=excluded.view_revision,graph_revision_seen=excluded.graph_revision_seen,viewport_json=excluded.viewport_json,node_layouts_json=excluded.node_layouts_json,selection_json=excluded.selection_json,groups_json=excluded.groups_json,updated_at=excluded.updated_at WHERE excluded.view_revision>=task_canvas_views.view_revision",params![entity_id,json_number(&payload,"view_revision").unwrap_or(1),json_number(&payload,"graph_revision_seen").unwrap_or(1),json_compact(payload.get("viewport")).unwrap_or_else(||"{\"x\":0,\"y\":0,\"zoom\":1}".into()),json_compact(payload.get("node_layouts")).unwrap_or_else(||"[]".into()),json_compact(payload.get("selection")).unwrap_or_else(||"[]".into()),json_compact(payload.get("groups")).unwrap_or_else(||"[]".into()),json_number(&payload,"updated_at").unwrap_or(now)])?;
        }
        _ => {}
    }
    Ok(())
}

fn json_text<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|item| item.as_str())
}
fn json_optional_text(value: &serde_json::Value, key: &str) -> Option<String> {
    json_text(value, key).map(str::to_string)
}
fn json_number(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|item| item.as_i64())
}
fn json_optional_number(value: &serde_json::Value, key: &str) -> Option<i64> {
    json_number(value, key)
}
fn json_compact(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .filter(|item| !item.is_null())
        .and_then(|item| serde_json::to_string(item).ok())
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
