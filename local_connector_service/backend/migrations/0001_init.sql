-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE IF NOT EXISTS local_connector_devices (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  public_key TEXT NOT NULL,
  client_version TEXT,
  os TEXT,
  status TEXT NOT NULL DEFAULT 'registered',
  last_seen_at TEXT,
  revoked_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_local_connector_devices_owner_user
ON local_connector_devices(owner_user_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_local_connector_devices_status
ON local_connector_devices(status);

CREATE TABLE IF NOT EXISTS local_connector_workspaces (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  local_path_alias TEXT NOT NULL,
  local_path_fingerprint TEXT NOT NULL,
  capabilities_json TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_local_connector_workspaces_owner_user
ON local_connector_workspaces(owner_user_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_local_connector_workspaces_device
ON local_connector_workspaces(device_id);

CREATE TABLE IF NOT EXISTS local_connector_project_bindings (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  mode TEXT NOT NULL DEFAULT 'local_mcp',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(owner_user_id, project_id, mode)
);

CREATE INDEX IF NOT EXISTS idx_local_connector_project_bindings_owner_project
ON local_connector_project_bindings(owner_user_id, project_id);

CREATE INDEX IF NOT EXISTS idx_local_connector_project_bindings_workspace
ON local_connector_project_bindings(workspace_id);

CREATE TABLE IF NOT EXISTS local_connector_sandbox_pairings (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  sandbox_mode TEXT NOT NULL DEFAULT 'docker',
  facade_base_url TEXT,
  access_client_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(owner_user_id, device_id, workspace_id)
);

CREATE INDEX IF NOT EXISTS idx_local_connector_sandbox_pairings_owner
ON local_connector_sandbox_pairings(owner_user_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_local_connector_sandbox_pairings_workspace
ON local_connector_sandbox_pairings(workspace_id);

CREATE TABLE IF NOT EXISTS local_connector_sessions (
  id TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'connected',
  connected_at TEXT NOT NULL,
  last_heartbeat_at TEXT NOT NULL,
  disconnected_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_local_connector_sessions_device_status
ON local_connector_sessions(device_id, status);

CREATE INDEX IF NOT EXISTS idx_local_connector_sessions_owner_updated
ON local_connector_sessions(owner_user_id, updated_at);
