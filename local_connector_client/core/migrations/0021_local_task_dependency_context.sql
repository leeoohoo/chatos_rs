ALTER TABLE task_board_tasks
ADD COLUMN execution_client_ref TEXT;

ALTER TABLE task_board_tasks
ADD COLUMN dependency_context_refs_json TEXT NOT NULL DEFAULT '[]';
