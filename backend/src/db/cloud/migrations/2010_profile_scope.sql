-- Migration 2010: S3 Profile Scope Metadata and Generation
CREATE TABLE IF NOT EXISTS cloud_profile_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    scope_id TEXT NOT NULL,
    service_origin TEXT NOT NULL,
    account_id TEXT NOT NULL,
    generation TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
