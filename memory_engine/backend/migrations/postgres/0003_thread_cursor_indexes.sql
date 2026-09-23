-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

DROP INDEX IF EXISTS engine_threads_listing_idx;
DROP INDEX IF EXISTS engine_threads_subject_idx;

CREATE INDEX engine_threads_updated_cursor_idx
    ON engine_threads(updated_at DESC,created_at DESC,id DESC);
CREATE INDEX engine_threads_status_updated_cursor_idx
    ON engine_threads(status,updated_at DESC,created_at DESC,id DESC);
CREATE INDEX engine_threads_listing_idx
    ON engine_threads(tenant_id,source_id,status,updated_at DESC,created_at DESC,id DESC);
CREATE INDEX engine_threads_subject_idx
    ON engine_threads(tenant_id,source_id,subject_id,updated_at DESC,created_at DESC,id DESC);
