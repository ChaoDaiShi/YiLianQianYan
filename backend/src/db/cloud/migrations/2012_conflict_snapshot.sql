-- Migration 2012: S4 Conflict Snapshot, Resolution State Machine, and Idempotency Tracking

CREATE TABLE cloud_sync_conflicts_new (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    local_revision INTEGER NOT NULL,
    remote_revision INTEGER NOT NULL,
    local_operation TEXT NOT NULL DEFAULT 'upsert' CHECK (local_operation IN ('upsert','delete')),
    remote_operation TEXT NOT NULL DEFAULT 'upsert' CHECK (remote_operation IN ('upsert','delete')),
    local_payload_json TEXT,
    remote_payload_json TEXT,
    remote_change_id TEXT,
    remote_sequence TEXT,
    conflict_version INTEGER NOT NULL DEFAULT 1,
    local_generation INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('open','resolving','resolved')),
    resolution_request_id TEXT,
    queued_change_id TEXT,
    copied_entity_id TEXT,
    created_at INTEGER NOT NULL,
    resolved_at INTEGER
);

INSERT INTO cloud_sync_conflicts_new (
    id, entity_type, entity_id, local_revision, remote_revision,
    local_operation, remote_operation, local_payload_json, remote_payload_json,
    remote_change_id, remote_sequence, conflict_version, local_generation,
    status, resolution_request_id, queued_change_id, copied_entity_id,
    created_at, resolved_at
)
SELECT
    id, entity_type, entity_id, local_revision, remote_revision,
    'upsert', COALESCE(remote_operation, 'upsert'), local_payload_json, remote_payload_json,
    NULL, NULL, 1, 0,
    CASE WHEN status IN ('open','resolving','resolved') THEN status ELSE 'open' END,
    NULL, NULL, NULL,
    created_at, resolved_at
FROM cloud_sync_conflicts;

DROP TABLE cloud_sync_conflicts;
ALTER TABLE cloud_sync_conflicts_new RENAME TO cloud_sync_conflicts;

CREATE INDEX idx_cloud_sync_conflicts_status ON cloud_sync_conflicts(status, created_at);
CREATE INDEX idx_cloud_sync_conflicts_entity_status ON cloud_sync_conflicts(entity_type, entity_id, status);
CREATE UNIQUE INDEX idx_cloud_sync_conflicts_active ON cloud_sync_conflicts(entity_type, entity_id, remote_change_id)
    WHERE status IN ('open', 'resolving') AND remote_change_id IS NOT NULL;
CREATE INDEX idx_cloud_sync_conflicts_queued ON cloud_sync_conflicts(queued_change_id)
    WHERE queued_change_id IS NOT NULL;
