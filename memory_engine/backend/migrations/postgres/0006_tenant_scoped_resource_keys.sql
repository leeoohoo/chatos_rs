-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- Tenant-owned resource identifiers are only unique inside a tenant/source
-- boundary. Child rows repeat that boundary in their foreign keys so a row
-- can never reference a thread owned by another tenant or source.

ALTER TABLE engine_records DROP CONSTRAINT engine_records_thread_id_fkey;
ALTER TABLE engine_compact_turns DROP CONSTRAINT engine_compact_turns_thread_id_fkey;
ALTER TABLE engine_summaries DROP CONSTRAINT engine_summaries_thread_id_fkey;
ALTER TABLE engine_thread_snapshots DROP CONSTRAINT engine_thread_snapshots_thread_id_fkey;

ALTER TABLE engine_subjects DROP CONSTRAINT engine_subjects_pkey;
ALTER TABLE engine_subject_memory_scopes DROP CONSTRAINT engine_subject_memory_scopes_pkey;
ALTER TABLE engine_subject_memories DROP CONSTRAINT engine_subject_memories_pkey;
ALTER TABLE engine_threads DROP CONSTRAINT engine_threads_pkey;
ALTER TABLE engine_records DROP CONSTRAINT engine_records_pkey;
ALTER TABLE engine_compact_turns DROP CONSTRAINT engine_compact_turns_pkey;
ALTER TABLE engine_summaries DROP CONSTRAINT engine_summaries_pkey;
ALTER TABLE engine_thread_snapshots DROP CONSTRAINT engine_thread_snapshots_pkey;

ALTER TABLE engine_subjects
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_subject_memory_scopes
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_subject_memories
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_threads
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_records
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_compact_turns
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_summaries
    ADD PRIMARY KEY (tenant_id, source_id, id);
ALTER TABLE engine_thread_snapshots
    ADD PRIMARY KEY (tenant_id, source_id, id);

ALTER TABLE engine_records
    ADD CONSTRAINT engine_records_thread_scope_fkey
        FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE;
ALTER TABLE engine_compact_turns
    ADD CONSTRAINT engine_compact_turns_thread_scope_fkey
        FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE;
ALTER TABLE engine_summaries
    ADD CONSTRAINT engine_summaries_thread_scope_fkey
        FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE;
ALTER TABLE engine_thread_snapshots
    ADD CONSTRAINT engine_thread_snapshots_thread_scope_fkey
        FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE;

DROP INDEX IF EXISTS engine_records_thread_cursor_idx;
CREATE INDEX engine_records_thread_cursor_idx
    ON engine_records (tenant_id, source_id, thread_id, created_at, id);

DROP INDEX IF EXISTS engine_summaries_thread_cursor_idx;
CREATE INDEX engine_summaries_thread_cursor_idx
    ON engine_summaries (tenant_id, source_id, thread_id, (-level), created_at, id);
