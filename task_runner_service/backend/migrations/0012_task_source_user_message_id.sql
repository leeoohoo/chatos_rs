-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE tasks ADD COLUMN source_user_message_id TEXT;

CREATE INDEX IF NOT EXISTS idx_tasks_source_user_message_id
ON tasks(source_user_message_id);
