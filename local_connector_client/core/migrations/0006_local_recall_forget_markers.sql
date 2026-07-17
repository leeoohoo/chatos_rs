CREATE TABLE IF NOT EXISTS subject_memory_forget_markers (
    owner_user_id TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    recall_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(owner_user_id, subject_type, subject_id, project_id, recall_key)
);

CREATE INDEX IF NOT EXISTS idx_subject_memory_forget_markers_project
ON subject_memory_forget_markers(owner_user_id, project_id);
