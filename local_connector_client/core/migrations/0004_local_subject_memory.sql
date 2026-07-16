CREATE TABLE IF NOT EXISTS subject_memories (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    recall_key TEXT NOT NULL,
    recall_text TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    source_summary_id TEXT NOT NULL,
    level INTEGER NOT NULL DEFAULT 0,
    confidence REAL,
    last_seen_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(source_session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(source_summary_id) REFERENCES memory_summaries(id) ON DELETE CASCADE,
    UNIQUE(owner_user_id, subject_type, subject_id, project_id, recall_key)
);

CREATE INDEX IF NOT EXISTS idx_subject_memories_scope_updated
ON subject_memories(owner_user_id, subject_type, subject_id, project_id, updated_at DESC);
