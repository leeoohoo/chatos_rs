-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE tasks ADD COLUMN task_profile TEXT NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_tasks_task_profile ON tasks(task_profile);

CREATE INDEX IF NOT EXISTS idx_tasks_chatos_source_profile
ON tasks(source_session_id, source_user_message_id, task_profile);
