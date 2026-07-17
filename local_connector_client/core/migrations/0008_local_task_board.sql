CREATE TABLE IF NOT EXISTS task_board_tasks (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'medium',
    status TEXT NOT NULL DEFAULT 'todo',
    tags_json TEXT NOT NULL DEFAULT '[]',
    prerequisite_task_ids_json TEXT NOT NULL DEFAULT '[]',
    due_at TEXT,
    outcome_summary TEXT NOT NULL DEFAULT '',
    outcome_items_json TEXT NOT NULL DEFAULT '[]',
    resume_hint TEXT NOT NULL DEFAULT '',
    blocker_reason TEXT NOT NULL DEFAULT '',
    blocker_needs_json TEXT NOT NULL DEFAULT '[]',
    blocker_kind TEXT NOT NULL DEFAULT '',
    completed_at TEXT,
    last_outcome_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(turn_id) REFERENCES turns(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_session_created
ON task_board_tasks(session_id, created_at, id);

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_turn_created
ON task_board_tasks(turn_id, created_at, id);

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_session_status
ON task_board_tasks(session_id, status, updated_at DESC);
