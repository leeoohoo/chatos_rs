-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX engine_summaries_thread_cursor_idx
    ON engine_summaries(thread_id,(-level),created_at,id);
