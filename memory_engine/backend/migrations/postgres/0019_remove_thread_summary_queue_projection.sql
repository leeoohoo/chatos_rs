-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

DROP INDEX IF EXISTS engine_threads_summary_claim_idx;
DROP INDEX IF EXISTS engine_threads_pending_summary_idx;
DROP INDEX IF EXISTS engine_threads_dispatch_idx;

UPDATE engine_threads
SET data = data - ARRAY[
    'summary_status',
    'pending_record_count',
    'pending_summary_tokens',
    'summary_job_run_id',
    'summary_locked_at',
    'summary_lock_expires_at'
]::TEXT[];

ALTER TABLE engine_threads
    DROP COLUMN summary_status,
    DROP COLUMN pending_record_count,
    DROP COLUMN pending_summary_tokens,
    DROP COLUMN summary_job_run_id,
    DROP COLUMN summary_locked_at,
    DROP COLUMN summary_lock_expires_at,
    DROP COLUMN rollup_job_run_id,
    DROP COLUMN rollup_locked_at,
    DROP COLUMN rollup_lock_expires_at,
    DROP COLUMN summary_dispatch_pending,
    DROP COLUMN summary_dispatch_version,
    DROP COLUMN summary_dispatch_published_version,
    DROP COLUMN summary_dispatch_consumed_version,
    DROP COLUMN summary_dispatch_force_version,
    DROP COLUMN summary_dispatch_requested_at,
    DROP COLUMN summary_dispatch_published_at,
    DROP COLUMN summary_dispatch_consumed_at,
    DROP COLUMN summary_dispatch_last_error,
    DROP COLUMN summary_dispatch_last_failed_at,
    DROP COLUMN summary_dispatch_dead_letter_version,
    DROP COLUMN summary_dispatch_dead_lettered_at,
    DROP COLUMN summary_dispatch_recovery_count,
    DROP COLUMN summary_dispatch_recovered_at;
