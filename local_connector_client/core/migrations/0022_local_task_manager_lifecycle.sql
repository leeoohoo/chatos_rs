ALTER TABLE task_board_tasks
ADD COLUMN manager_scope TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN task_session_id TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN required_for_parent_completion INTEGER NOT NULL DEFAULT 0;

ALTER TABLE task_board_tasks
ADD COLUMN closure_state TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN closure_reason TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN idempotency_key TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN lifecycle_updated_at TEXT;

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_manager_session
ON task_board_tasks(owner_user_id, session_id, task_session_id, manager_scope, closure_state);

CREATE UNIQUE INDEX IF NOT EXISTS idx_task_board_tasks_manager_idempotency
ON task_board_tasks(owner_user_id, session_id, task_session_id, idempotency_key)
WHERE task_session_id IS NOT NULL AND idempotency_key IS NOT NULL;
