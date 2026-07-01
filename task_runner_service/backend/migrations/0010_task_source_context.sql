-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE tasks ADD COLUMN source_session_id TEXT;
ALTER TABLE tasks ADD COLUMN source_turn_id TEXT;

CREATE INDEX IF NOT EXISTS idx_tasks_source_session_id ON tasks(source_session_id);
CREATE INDEX IF NOT EXISTS idx_tasks_source_turn_id ON tasks(source_turn_id);
