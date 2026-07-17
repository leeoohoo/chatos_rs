CREATE TABLE IF NOT EXISTS project_runtime_environments (
    project_id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    sandbox_enabled INTEGER NOT NULL DEFAULT 1,
    sandbox_provider TEXT NOT NULL DEFAULT 'local_connector',
    file_provider TEXT NOT NULL DEFAULT 'local_connector',
    analysis_summary TEXT,
    not_runnable_reason TEXT,
    detected_stack_json TEXT NOT NULL DEFAULT '{}',
    required_services_json TEXT NOT NULL DEFAULT '[]',
    env_vars_json TEXT NOT NULL DEFAULT '{}',
    generated_config_files_json TEXT NOT NULL DEFAULT '[]',
    last_agent_run_id TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_runtime_environment_images (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    environment_key TEXT NOT NULL,
    environment_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    image_id TEXT,
    image_ref TEXT,
    image_provider TEXT NOT NULL DEFAULT 'local_connector',
    features_json TEXT NOT NULL DEFAULT '[]',
    ports_json TEXT NOT NULL DEFAULT '[]',
    env_vars_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'planned',
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_runtime_environment_images_project
ON project_runtime_environment_images(project_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS project_runtime_environment_progress (
    project_id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    run_id TEXT,
    phase TEXT NOT NULL DEFAULT 'idle',
    status TEXT NOT NULL DEFAULT 'idle',
    progress_percent INTEGER,
    provider TEXT NOT NULL DEFAULT 'local_connector',
    started_at TEXT,
    updated_at TEXT NOT NULL,
    finished_at TEXT,
    logs TEXT NOT NULL DEFAULT '',
    error TEXT,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE
);
