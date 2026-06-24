CREATE INDEX IF NOT EXISTS idx_task_runs_model_config_id_created_at
ON task_runs(model_config_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_status_task_id
ON ask_user_prompts(status, task_id);

CREATE INDEX IF NOT EXISTS idx_tasks_schedule_due_lookup
ON tasks(
  status,
  json_extract(schedule_json, '$.mode'),
  datetime(json_extract(schedule_json, '$.next_run_at'))
);
