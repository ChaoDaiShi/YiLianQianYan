-- Migration 2015: S5 Lifecycle Reliability, Deletion Status Credentials, and Export Metadata

CREATE TABLE cloud_lifecycle_jobs_new (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('export','deletion')),
    status TEXT NOT NULL CHECK (status IN (
        'queued',
        'running',
        'waiting_for_producers',
        'waiting_backup_expiry',
        'completed',
        'failed',
        'retry_wait'
    )),
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    result_ref TEXT,
    error_code TEXT,
    live_data_removed INTEGER NOT NULL DEFAULT 0 CHECK (live_data_removed IN (0, 1)),
    backups_cleared INTEGER NOT NULL DEFAULT 0 CHECK (backups_cleared IN (0, 1)),
    backup_purge_deadline INTEGER,
    status_credential_ref_json TEXT,
    object_key TEXT,
    size INTEGER,
    sha256 TEXT,
    expires_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO cloud_lifecycle_jobs_new (
    id, kind, status, progress, result_ref, error_code,
    live_data_removed, backups_cleared, backup_purge_deadline,
    status_credential_ref_json, object_key, size, sha256, expires_at,
    created_at, updated_at
)
SELECT
    id, kind, status, progress, result_ref, error_code,
    0, 0, NULL,
    NULL, NULL, NULL, NULL, NULL,
    created_at, updated_at
FROM cloud_lifecycle_jobs;

DROP TABLE cloud_lifecycle_jobs;
ALTER TABLE cloud_lifecycle_jobs_new RENAME TO cloud_lifecycle_jobs;

CREATE INDEX IF NOT EXISTS idx_cloud_lifecycle_jobs_status ON cloud_lifecycle_jobs(status, created_at);
