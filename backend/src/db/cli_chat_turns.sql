CREATE TABLE cli_chat_turns (
    turn_id TEXT PRIMARY KEY REFERENCES conversation_turns(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    session_id TEXT,
    context_digest TEXT NOT NULL,
    context_chars INTEGER NOT NULL,
    history_entries INTEGER NOT NULL,
    state TEXT NOT NULL,
    usage_json TEXT
);
