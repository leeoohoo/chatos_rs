ALTER TABLE task_board_tasks ADD COLUMN project_work_item_id TEXT;
ALTER TABLE task_board_tasks ADD COLUMN requirement_id TEXT;
ALTER TABLE task_board_tasks ADD COLUMN execution_group_id TEXT;

CREATE INDEX IF NOT EXISTS idx_task_board_tasks_project_execution
ON task_board_tasks(owner_user_id, execution_group_id, project_work_item_id, created_at);
