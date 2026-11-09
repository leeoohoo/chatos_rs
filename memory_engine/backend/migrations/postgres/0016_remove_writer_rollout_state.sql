-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- The generation/coverage/lineage implementation is the only writer. Transitional
-- state and writer-version switches are intentionally removed.
DROP TRIGGER IF EXISTS engine_thread_partition_writer_transition_guard
    ON engine_thread_summary_partitions;
DROP FUNCTION IF EXISTS engine_guard_partition_writer_transition();

ALTER TABLE engine_thread_summary_partitions
    DROP CONSTRAINT IF EXISTS engine_thread_partition_writer_cutover_check,
    DROP COLUMN migration_state,
    DROP COLUMN authoritative_writer;
