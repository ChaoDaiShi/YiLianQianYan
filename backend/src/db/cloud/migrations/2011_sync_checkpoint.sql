-- Migration 2011: S2 Client Sync Checkpoint, Outbox States, Authoritative Revision, and Pending Projections

ALTER TABLE cloud_sync_cursor ADD COLUMN download_cursor TEXT NOT NULL DEFAULT '0';
ALTER TABLE cloud_sync_cursor ADD COLUMN sync_epoch TEXT;
INSERT OR IGNORE INTO cloud_sync_cursor (singleton, cursor, download_cursor, sync_epoch, updated_at) VALUES (1, '0', '0', NULL, 0);
UPDATE cloud_sync_cursor SET download_cursor = COALESCE(cursor, '0') WHERE download_cursor = '0' AND cursor IS NOT NULL;

CREATE TABLE cloud_sync_outbox_new (
    change_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
    revision INTEGER NOT NULL CHECK (revision > 0),
    operation TEXT NOT NULL CHECK (operation IN ('upsert','delete')),
    payload_json TEXT,
    canonical_digest TEXT,
    device_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending','in_flight','retry_wait','conflicted','rejected')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_retry_at INTEGER,
    last_error_code TEXT,
    local_generation INTEGER NOT NULL DEFAULT 1 CHECK (local_generation >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO cloud_sync_outbox_new (
    change_id, entity_type, entity_id, base_revision, revision, operation,
    payload_json, canonical_digest, device_id, state, attempts, next_retry_at,
    last_error_code, local_generation, created_at, updated_at
)
SELECT
    change_id, entity_type, entity_id, base_revision, revision, operation,
    payload_json, NULL, device_id,
    CASE WHEN state = 'error' THEN 'retry_wait' ELSE state END,
    attempts, next_retry_at, last_error_code, 1, created_at, updated_at
FROM cloud_sync_outbox;

DROP TABLE cloud_sync_outbox;
ALTER TABLE cloud_sync_outbox_new RENAME TO cloud_sync_outbox;
CREATE INDEX idx_cloud_sync_outbox_state_retry ON cloud_sync_outbox(state, next_retry_at, created_at);
CREATE INDEX idx_cloud_sync_outbox_entity ON cloud_sync_outbox(entity_type, entity_id, state);

CREATE TABLE cloud_entity_revisions_new (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    authoritative_revision INTEGER NOT NULL DEFAULT 0 CHECK (authoritative_revision >= 0),
    local_generation INTEGER NOT NULL DEFAULT 0 CHECK (local_generation >= 0),
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (entity_type, entity_id)
);

INSERT INTO cloud_entity_revisions_new (
    entity_type, entity_id, fingerprint, revision, authoritative_revision, local_generation, deleted, updated_at
)
SELECT
    r.entity_type,
    r.entity_id,
    r.fingerprint,
    r.revision,
    COALESCE(s.revision, 0),
    r.revision,
    r.deleted,
    r.updated_at
FROM cloud_entity_revisions r
LEFT JOIN cloud_synced_entities s ON r.entity_type = s.entity_type AND r.entity_id = s.entity_id;

DROP TABLE cloud_entity_revisions;
ALTER TABLE cloud_entity_revisions_new RENAME TO cloud_entity_revisions;

ALTER TABLE cloud_synced_entities ADD COLUMN canonical_digest TEXT;

CREATE TABLE IF NOT EXISTS cloud_pending_projections (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    dependency_type TEXT NOT NULL,
    dependency_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    payload_json TEXT,
    sequence INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cloud_pending_projections_dep ON cloud_pending_projections(dependency_type, dependency_id);
