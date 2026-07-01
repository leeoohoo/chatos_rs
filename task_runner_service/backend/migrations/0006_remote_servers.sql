-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE IF NOT EXISTS remote_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22,
  username TEXT NOT NULL,
  auth_type TEXT NOT NULL DEFAULT 'password',
  password TEXT,
  private_key_path TEXT,
  certificate_path TEXT,
  default_remote_path TEXT,
  host_key_policy TEXT NOT NULL DEFAULT 'accept_new',
  enabled INTEGER NOT NULL DEFAULT 1,
  last_tested_at TEXT,
  last_test_status TEXT,
  last_test_message TEXT,
  last_active_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_remote_servers_updated_at
  ON remote_servers(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_remote_servers_enabled
  ON remote_servers(enabled);
