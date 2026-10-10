CREATE TABLE IF NOT EXISTS capability_usage (
    execution_id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL DEFAULT 1,
    capability_id TEXT NOT NULL,
    plugin_id TEXT NOT NULL,
    version TEXT NOT NULL,
    used_at INTEGER NOT NULL,
    execution_status TEXT NOT NULL CHECK (execution_status IN ('succeeded', 'failed', 'denied', 'cancelled')),
    latency_ms INTEGER NOT NULL DEFAULT 0,
    approval_required INTEGER NOT NULL DEFAULT 0 CHECK (approval_required IN (0, 1)),
    risk_level TEXT NOT NULL,
    result_digest TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_capability_usage_used_at ON capability_usage(used_at DESC);
CREATE INDEX IF NOT EXISTS idx_capability_usage_capability ON capability_usage(capability_id, plugin_id);
