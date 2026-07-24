ALTER TABLE local_task_runs
ADD COLUMN dispatch_paused INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_local_task_runs_dispatch_queue
ON local_task_runs(status, dispatch_paused, priority DESC, created_at ASC);
