-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE model_configs ADD COLUMN usage_scenario TEXT;

CREATE TABLE IF NOT EXISTS runtime_settings (
  id TEXT PRIMARY KEY,
  task_execution_max_iterations INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
