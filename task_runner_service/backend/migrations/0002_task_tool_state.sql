ALTER TABLE tasks ADD COLUMN parent_task_id TEXT;
ALTER TABLE tasks ADD COLUMN source_run_id TEXT;
ALTER TABLE tasks ADD COLUMN task_tool_state_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_tasks_parent_task_id ON tasks(parent_task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_source_run_id ON tasks(source_run_id);
