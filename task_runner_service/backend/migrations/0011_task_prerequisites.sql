CREATE TABLE IF NOT EXISTS task_prerequisites (
  task_id TEXT NOT NULL,
  prerequisite_task_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(task_id, prerequisite_task_id),
  FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_prerequisites_task_id
ON task_prerequisites(task_id);

CREATE INDEX IF NOT EXISTS idx_task_prerequisites_prerequisite_task_id
ON task_prerequisites(prerequisite_task_id);
