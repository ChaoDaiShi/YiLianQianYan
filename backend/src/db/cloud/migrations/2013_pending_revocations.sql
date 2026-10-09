-- Migration 2013: S3 Pending Revocation Tasks Metadata
CREATE TABLE IF NOT EXISTS cloud_pending_revocations (
    session_id TEXT PRIMARY KEY,
    scope_id TEXT NOT NULL,
    service_origin TEXT NOT NULL,
    account_id TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_retry_at INTEGER NOT NULL,
    last_error_code TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cloud_pending_revocations_retry ON cloud_pending_revocations(next_retry_at);
