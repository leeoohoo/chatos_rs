ALTER TABLE model_configs ADD COLUMN usage_scenario TEXT;

CREATE TABLE IF NOT EXISTS runtime_settings (
  id TEXT PRIMARY KEY,
  task_execution_max_iterations INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
