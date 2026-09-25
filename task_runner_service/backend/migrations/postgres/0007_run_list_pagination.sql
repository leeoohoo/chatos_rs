-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX task_runs_created_cursor_idx
    ON task_runs(created_at DESC,id);
