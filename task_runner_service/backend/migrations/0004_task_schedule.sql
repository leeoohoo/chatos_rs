ALTER TABLE tasks ADD COLUMN schedule_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
