-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT,
  objective TEXT NOT NULL,
  input_payload_json TEXT NOT NULL DEFAULT 'null',
  status TEXT NOT NULL DEFAULT 'draft',
  priority INTEGER NOT NULL DEFAULT 0,
  tags_json TEXT NOT NULL DEFAULT '[]',
  default_model_config_id TEXT,
  memory_thread_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL DEFAULT 'default_tenant',
  subject_id TEXT NOT NULL DEFAULT 'task_runner_user_default',
  result_summary TEXT,
  last_run_id TEXT,
  mcp_config_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_updated_at ON tasks(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_default_model_config_id ON tasks(default_model_config_id);

CREATE TABLE IF NOT EXISTS model_configs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL DEFAULT 'openai_compatible',
  base_url TEXT NOT NULL,
  api_key TEXT NOT NULL,
  model TEXT NOT NULL,
  temperature REAL,
  max_output_tokens INTEGER,
  thinking_level TEXT,
  supports_responses INTEGER NOT NULL DEFAULT 0,
  instructions TEXT,
  request_cwd TEXT,
  include_prompt_cache_retention INTEGER NOT NULL DEFAULT 0,
  request_body_limit_bytes INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_model_configs_updated_at ON model_configs(updated_at DESC);

CREATE TABLE IF NOT EXISTS task_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  model_config_id TEXT NOT NULL,
  memory_thread_id TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued',
  started_at TEXT,
  finished_at TEXT,
  input_snapshot_json TEXT NOT NULL DEFAULT '{}',
  context_snapshot_json TEXT NOT NULL DEFAULT 'null',
  result_summary TEXT,
  error_message TEXT,
  usage_json TEXT NOT NULL DEFAULT 'null',
  report_json TEXT NOT NULL DEFAULT 'null',
  cancel_requested INTEGER NOT NULL DEFAULT 0,
  summary_job_run_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_runs_task_id ON task_runs(task_id);
CREATE INDEX IF NOT EXISTS idx_task_runs_status ON task_runs(status);
CREATE INDEX IF NOT EXISTS idx_task_runs_created_at ON task_runs(created_at DESC);

CREATE TABLE IF NOT EXISTS task_run_events (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  message TEXT,
  payload_json TEXT NOT NULL DEFAULT 'null',
  created_at TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES task_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_run_events_run_id ON task_run_events(run_id);
CREATE INDEX IF NOT EXISTS idx_task_run_events_created_at ON task_run_events(created_at ASC);
