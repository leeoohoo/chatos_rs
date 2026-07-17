CREATE TABLE IF NOT EXISTS local_projects (
    project_id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    project_name TEXT NOT NULL,
    root_relative_path TEXT,
    execution_plane TEXT NOT NULL DEFAULT 'local_connector',
    runtime_schema_version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_local_projects_owner_updated
ON local_projects(owner_user_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    selected_model_id TEXT,
    selected_agent_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    message_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id)
);

CREATE INDEX IF NOT EXISTS idx_sessions_project_updated
ON sessions(project_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS session_runtime_settings (
    session_id TEXT PRIMARY KEY,
    selected_model_id TEXT,
    selected_model_name TEXT,
    selected_thinking_level TEXT,
    workspace_root TEXT,
    reasoning_enabled INTEGER NOT NULL DEFAULT 0,
    plan_mode_enabled INTEGER NOT NULL DEFAULT 0,
    mcp_enabled INTEGER NOT NULL DEFAULT 1,
    enabled_mcp_ids_json TEXT NOT NULL DEFAULT '[]',
    selected_skill_ids_json TEXT NOT NULL DEFAULT '[]',
    auto_create_task INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    user_message_id TEXT,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    error_message TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    UNIQUE(session_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_turns_session_created
ON turns(session_id, created_at DESC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    sequence_no INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    reasoning TEXT,
    tool_calls_json TEXT,
    tool_call_id TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(turn_id) REFERENCES turns(id) ON DELETE SET NULL,
    UNIQUE(session_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_messages_session_sequence
ON messages(session_id, sequence_no);

CREATE TABLE IF NOT EXISTS runtime_events (
    event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    owner_user_id TEXT NOT NULL,
    project_id TEXT,
    session_id TEXT,
    turn_id TEXT,
    event_name TEXT NOT NULL,
    stream_type TEXT,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_events_session_seq
ON runtime_events(session_id, event_seq);
