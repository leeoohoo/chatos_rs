CREATE TABLE IF NOT EXISTS task_projects (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  name TEXT NOT NULL,
  root_path TEXT,
  git_url TEXT,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_projects_owner_user_id
ON task_projects(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_task_projects_status
ON task_projects(status);

ALTER TABLE tasks ADD COLUMN project_id TEXT NOT NULL DEFAULT '-1';

CREATE INDEX IF NOT EXISTS idx_tasks_project_id
ON tasks(project_id);

CREATE INDEX IF NOT EXISTS idx_tasks_owner_project
ON tasks(owner_user_id, project_id);
