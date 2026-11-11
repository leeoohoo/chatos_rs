-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE engine_records
    ADD COLUMN recall_eligible BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN retired_at TIMESTAMPTZ NULL,
    ADD COLUMN retired_reason TEXT NULL;

ALTER TABLE engine_summaries
    ADD COLUMN retired_at TIMESTAMPTZ NULL;

ALTER TABLE engine_subject_memories
    ADD COLUMN retired_at TIMESTAMPTZ NULL;

ALTER TABLE engine_threads
    ADD COLUMN recall_eligible BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN retired_at TIMESTAMPTZ NULL,
    ADD COLUMN retired_reason TEXT NULL;

CREATE INDEX engine_records_recall_lifecycle_idx
    ON engine_records(tenant_id,source_id,thread_id,recall_eligible,created_at,id);
CREATE INDEX engine_threads_recall_lifecycle_idx
    ON engine_threads(tenant_id,source_id,recall_eligible,updated_at,id);
