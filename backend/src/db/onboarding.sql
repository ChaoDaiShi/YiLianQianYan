CREATE TABLE local_onboarding (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    scope TEXT NOT NULL UNIQUE,
    revision INTEGER NOT NULL DEFAULT 0,
    current_step INTEGER NOT NULL DEFAULT 0,
    draft TEXT,
    draft_workspace TEXT,
    profile TEXT NOT NULL DEFAULT '{}',
    field_dates TEXT NOT NULL DEFAULT '{}',
    completed_at INTEGER
);
INSERT INTO local_onboarding(singleton, scope) VALUES(1, lower(hex(randomblob(16))));
