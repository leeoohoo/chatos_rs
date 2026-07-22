ALTER TABLE task_board_tasks ADD COLUMN task_kind TEXT NOT NULL DEFAULT 'task_manager';
ALTER TABLE task_board_tasks ADD COLUMN objective TEXT NOT NULL DEFAULT '';
ALTER TABLE task_board_tasks ADD COLUMN model_config_id TEXT;
ALTER TABLE task_board_tasks ADD COLUMN is_planning_task INTEGER NOT NULL DEFAULT 0;
ALTER TABLE task_board_tasks ADD COLUMN enabled_builtin_kinds_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE task_board_tasks ADD COLUMN external_mcp_config_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE task_board_tasks ADD COLUMN selected_skill_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE task_board_tasks ADD COLUMN last_run_id TEXT;

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_kind_source
ON task_board_tasks(owner_user_id, session_id, task_kind, turn_id, created_at);

CREATE TABLE local_task_runs_v2 (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    requirement_id TEXT,
    task_kind TEXT NOT NULL DEFAULT 'project_work_item',
    task_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL UNIQUE,
    execution_group_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    priority INTEGER NOT NULL DEFAULT 0,
    prompt TEXT NOT NULL,
    model_config_id TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
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
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    CHECK(task_kind IN ('project_work_item', 'conversation_task'))
);

INSERT INTO local_task_runs_v2 (
    id, owner_user_id, project_id, requirement_id, task_kind, task_id,
    session_id, turn_id, execution_group_id, status, priority, prompt,
    model_config_id, attempt, max_attempts, worker_id, lease_expires_at,
    heartbeat_at, cancel_requested, result_content, result_reasoning,
    tool_calls_json, finish_reason, usage_json, error, created_at,
    started_at, finished_at, updated_at
)
SELECT
    id, owner_user_id, project_id, requirement_id, 'project_work_item', task_id,
    session_id, turn_id, execution_group_id, status, priority, prompt,
    model_config_id, attempt, MAX(max_attempts, 5), worker_id, lease_expires_at,
    heartbeat_at, cancel_requested, result_content, result_reasoning,
    tool_calls_json, finish_reason, usage_json, error, created_at,
    started_at, finished_at, updated_at
FROM local_task_runs;

CREATE TABLE local_task_run_events_v2 (
    event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES local_task_runs_v2(id) ON DELETE CASCADE
);

INSERT INTO local_task_run_events_v2 (
    event_seq, run_id, owner_user_id, event_name, payload_json, created_at
)
SELECT event_seq, run_id, owner_user_id, event_name, payload_json, created_at
FROM local_task_run_events;

DROP TABLE local_task_run_events;
DROP TABLE local_task_runs;
ALTER TABLE local_task_runs_v2 RENAME TO local_task_runs;
ALTER TABLE local_task_run_events_v2 RENAME TO local_task_run_events;

CREATE INDEX IF NOT EXISTS idx_local_task_runs_queue
ON local_task_runs(status, priority DESC, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_local_task_runs_requirement
ON local_task_runs(requirement_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_task_runs_active_task
ON local_task_runs(task_kind, task_id)
WHERE status IN ('queued', 'running');

CREATE INDEX IF NOT EXISTS idx_local_task_run_events_run_seq
ON local_task_run_events(run_id, event_seq ASC);
