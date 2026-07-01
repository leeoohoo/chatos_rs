-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE IF NOT EXISTS ask_user_prompts (
  id TEXT PRIMARY KEY,
  task_id TEXT,
  run_id TEXT,
  conversation_id TEXT NOT NULL,
  conversation_turn_id TEXT NOT NULL,
  tool_call_id TEXT,
  kind TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  message TEXT NOT NULL DEFAULT '',
  allow_cancel INTEGER NOT NULL DEFAULT 1,
  timeout_ms INTEGER NOT NULL DEFAULT 86400000,
  payload_json TEXT NOT NULL DEFAULT '{}',
  response_json TEXT NOT NULL DEFAULT 'null',
  status TEXT NOT NULL DEFAULT 'pending',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_run_id ON ask_user_prompts(run_id);
CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_task_id ON ask_user_prompts(task_id);
CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_status ON ask_user_prompts(status);
CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_updated_at ON ask_user_prompts(updated_at DESC);
