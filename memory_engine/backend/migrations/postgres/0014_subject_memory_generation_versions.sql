-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE engine_subject_memories
    DROP CONSTRAINT engine_subject_memories_tenant_id_source_id_subject_id_memo_key;

CREATE UNIQUE INDEX engine_subject_memories_legacy_logical_key_idx
    ON engine_subject_memories(tenant_id,source_id,subject_id,memory_key)
    WHERE generation_id IS NULL;

CREATE UNIQUE INDEX engine_subject_memories_one_active_logical_key_idx
    ON engine_subject_memories(tenant_id,source_id,subject_id,memory_key)
    WHERE status='active' AND recall_eligible;
