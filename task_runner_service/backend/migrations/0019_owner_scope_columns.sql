-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE tasks ADD COLUMN owner_user_id TEXT;
ALTER TABLE tasks ADD COLUMN owner_username TEXT;
ALTER TABLE tasks ADD COLUMN owner_display_name TEXT;

CREATE INDEX IF NOT EXISTS idx_tasks_owner_user_id ON tasks(owner_user_id);

ALTER TABLE remote_servers ADD COLUMN owner_user_id TEXT;
ALTER TABLE remote_servers ADD COLUMN owner_username TEXT;
ALTER TABLE remote_servers ADD COLUMN owner_display_name TEXT;

CREATE INDEX IF NOT EXISTS idx_remote_servers_owner_user_id
  ON remote_servers(owner_user_id);

ALTER TABLE external_mcp_configs ADD COLUMN owner_user_id TEXT;
ALTER TABLE external_mcp_configs ADD COLUMN owner_username TEXT;
ALTER TABLE external_mcp_configs ADD COLUMN owner_display_name TEXT;

CREATE INDEX IF NOT EXISTS idx_external_mcp_configs_owner_user_id
  ON external_mcp_configs(owner_user_id);
