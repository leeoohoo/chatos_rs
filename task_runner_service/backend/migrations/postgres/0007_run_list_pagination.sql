-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- Forward-compatible additive index: old and new application versions keep
-- the same query contract, and correctness never depends on this index being present.

CREATE INDEX task_runs_created_cursor_idx
    ON task_runs(created_at DESC,id);
