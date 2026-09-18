-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX engine_records_thread_cursor_idx
    ON engine_records(thread_id,created_at,id);
