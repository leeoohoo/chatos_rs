ALTER TABLE remote_servers ADD COLUMN creator_user_id TEXT;
ALTER TABLE remote_servers ADD COLUMN creator_username TEXT;
ALTER TABLE remote_servers ADD COLUMN creator_display_name TEXT;
ALTER TABLE remote_servers ADD COLUMN task_id TEXT;

CREATE INDEX IF NOT EXISTS idx_remote_servers_creator_user_id
  ON remote_servers(creator_user_id);

CREATE INDEX IF NOT EXISTS idx_remote_servers_task_id
  ON remote_servers(task_id);
