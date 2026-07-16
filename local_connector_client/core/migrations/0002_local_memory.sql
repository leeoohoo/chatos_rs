CREATE TABLE IF NOT EXISTS memory_summaries (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    summary_model TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    source_start_message_id TEXT,
    source_end_message_id TEXT,
    source_message_count INTEGER NOT NULL DEFAULT 0,
    source_estimated_tokens INTEGER NOT NULL DEFAULT 0,
    level INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'completed',
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    UNIQUE(session_id, source_end_message_id)
);

CREATE INDEX IF NOT EXISTS idx_memory_summaries_session_created
ON memory_summaries(session_id, created_at DESC);
