CREATE TABLE IF NOT EXISTS external_mcp_configs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  transport TEXT NOT NULL DEFAULT 'stdio',
  command TEXT,
  args_json TEXT NOT NULL DEFAULT '[]',
  url TEXT,
  headers_json TEXT NOT NULL DEFAULT '{}',
  env_json TEXT NOT NULL DEFAULT '{}',
  cwd TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_external_mcp_configs_updated_at
  ON external_mcp_configs(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_external_mcp_configs_enabled
  ON external_mcp_configs(enabled);

CREATE INDEX IF NOT EXISTS idx_external_mcp_configs_creator_user_id
  ON external_mcp_configs(creator_user_id);
