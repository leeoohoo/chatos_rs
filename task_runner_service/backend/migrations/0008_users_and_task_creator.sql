-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_login_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_enabled ON users(enabled);

ALTER TABLE tasks ADD COLUMN creator_user_id TEXT;
ALTER TABLE tasks ADD COLUMN creator_username TEXT;
ALTER TABLE tasks ADD COLUMN creator_display_name TEXT;

CREATE INDEX IF NOT EXISTS idx_tasks_creator_user_id ON tasks(creator_user_id);
