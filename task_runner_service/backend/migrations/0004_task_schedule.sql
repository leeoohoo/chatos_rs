-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE tasks ADD COLUMN schedule_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
