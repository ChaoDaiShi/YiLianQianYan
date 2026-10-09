use crate::db::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};

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

const CLOUD_PROFILE_METADATA_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2010,
    "2010_profile_scope",
    MigrationOwner::V11Cloud,
    include_str!("migrations/2010_profile_scope.sql"),
);

const CLOUD_SYNC_CHECKPOINT_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2011,
    "2011_sync_checkpoint",
    MigrationOwner::V11Cloud,
    include_str!("migrations/2011_sync_checkpoint.sql"),
);

const CLOUD_PENDING_REVOCATIONS_SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    2013,
    "2013_pending_revocations",
    MigrationOwner::V11Cloud,
    include_str!("migrations/2013_pending_revocations.sql"),
);

impl Database {
    pub fn initialize_cloud_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migrations(&[
            CLOUD_SCHEMA,
            CLOUD_SEARCH_SCHEMA,
            CLOUD_SYNC_ENTITY_SCHEMA,
            CLOUD_LOCAL_REVISION_SCHEMA,
            CLOUD_CONFLICT_OPERATION_SCHEMA,
            CLOUD_PROFILE_METADATA_SCHEMA,
            CLOUD_SYNC_CHECKPOINT_SCHEMA,
            CLOUD_PENDING_REVOCATIONS_SCHEMA,
        ])
    }
}
