CREATE TABLE IF NOT EXISTS local_task_runs (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    requirement_id TEXT,
    task_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL UNIQUE,
    execution_group_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    priority INTEGER NOT NULL DEFAULT 0,
    prompt TEXT NOT NULL,
    model_config_id TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 2,
    worker_id TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    result_content TEXT,
    result_reasoning TEXT,
    tool_calls_json TEXT,
    finish_reason TEXT,
    usage_json TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE,
    FOREIGN KEY(requirement_id) REFERENCES project_requirements(id) ON DELETE SET NULL,
    FOREIGN KEY(task_id) REFERENCES project_work_items(id) ON DELETE CASCADE,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_local_task_runs_queue
ON local_task_runs(status, priority DESC, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_local_task_runs_requirement
ON local_task_runs(requirement_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_task_runs_active_task
ON local_task_runs(task_id)
WHERE status IN ('queued', 'running');

CREATE TABLE IF NOT EXISTS local_task_run_events (
    event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES local_task_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_local_task_run_events_run_seq
ON local_task_run_events(run_id, event_seq ASC);
