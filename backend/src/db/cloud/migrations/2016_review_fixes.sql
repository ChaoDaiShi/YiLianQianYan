CREATE TABLE IF NOT EXISTS cloud_sync_policy (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    policy_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cloud_reliability_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    reconciliation_version INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
INSERT OR IGNORE INTO cloud_reliability_state(singleton, reconciliation_version, updated_at)
VALUES(1, 0, 0);
