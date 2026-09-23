-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_job_policies (
    job_type TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX engine_job_policies_enabled_idx ON engine_job_policies(enabled, job_type);

CREATE TABLE engine_job_runs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    tenant_id TEXT NULL,
    source_id TEXT NULL,
    thread_id TEXT NULL,
    subject_id TEXT NULL,
    thread_label TEXT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NULL,
    lock_owner TEXT NULL,
    lock_expires_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX engine_job_runs_type_started_idx ON engine_job_runs(job_type, started_at DESC);
CREATE INDEX engine_job_runs_status_started_idx ON engine_job_runs(status, started_at DESC);
CREATE INDEX engine_job_runs_tenant_source_idx ON engine_job_runs(tenant_id, source_id, started_at DESC);
CREATE INDEX engine_job_runs_thread_idx ON engine_job_runs(thread_id, started_at DESC) WHERE thread_id IS NOT NULL;
CREATE INDEX engine_job_runs_stale_lock_idx ON engine_job_runs(status, lock_expires_at) WHERE status='running';

CREATE TABLE engine_sources (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NULL,
    source_id TEXT NOT NULL UNIQUE,
    source_type TEXT NOT NULL,
    status TEXT NOT NULL,
    sdk_enabled BOOLEAN NOT NULL,
    secret_key_hash TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX engine_sources_listing_idx ON engine_sources(tenant_id, source_type, status, updated_at DESC);
CREATE INDEX engine_sources_sdk_idx ON engine_sources(sdk_enabled, status, source_id);

CREATE TABLE engine_subjects (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, subject_id)
);
CREATE INDEX engine_subjects_type_idx ON engine_subjects(tenant_id, source_id, subject_type, status, updated_at DESC);

CREATE TABLE engine_subject_memory_scopes (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    memory_type TEXT NOT NULL,
    source_thread_label TEXT NOT NULL,
    relation_subject_id TEXT NULL,
    source_summary_type TEXT NULL,
    status TEXT NOT NULL,
    subject_memory_status TEXT NOT NULL DEFAULT 'idle',
    subject_memory_dispatch_pending BOOLEAN NOT NULL DEFAULT false,
    subject_memory_dispatch_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_dispatch_published_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_dispatch_consumed_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_dispatch_requested_at TIMESTAMPTZ NULL,
    subject_memory_dispatch_published_at TIMESTAMPTZ NULL,
    subject_memory_dispatch_consumed_at TIMESTAMPTZ NULL,
    subject_memory_dispatch_last_error TEXT NULL,
    subject_memory_dispatch_last_failed_at TIMESTAMPTZ NULL,
    subject_memory_dispatch_dead_letter_version BIGINT NULL,
    subject_memory_dispatch_dead_lettered_at TIMESTAMPTZ NULL,
    lock_owner TEXT NULL,
    lock_expires_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, scope_key)
);
CREATE INDEX engine_subject_memory_scopes_subject_idx ON engine_subject_memory_scopes(tenant_id, source_id, subject_id, status, updated_at DESC);
CREATE INDEX engine_subject_memory_scopes_dispatch_idx ON engine_subject_memory_scopes(subject_memory_dispatch_pending, subject_memory_dispatch_requested_at, updated_at) WHERE subject_memory_dispatch_pending;
CREATE INDEX engine_subject_memory_scopes_claim_idx ON engine_subject_memory_scopes(subject_memory_status, lock_expires_at, updated_at);

CREATE TABLE engine_subject_memories (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    memory_key TEXT NOT NULL,
    memory_type TEXT NOT NULL,
    level BIGINT NOT NULL,
    source_digest TEXT NULL,
    relation_subject_id TEXT NULL,
    status TEXT NOT NULL,
    rollup_status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, subject_id, memory_key)
);
CREATE INDEX engine_subject_memories_query_idx ON engine_subject_memories(tenant_id, source_id, subject_id, memory_type, level, status, updated_at DESC);
CREATE INDEX engine_subject_memories_rollup_idx ON engine_subject_memories(tenant_id, source_id, subject_id, memory_type, rollup_status, level, created_at);
CREATE INDEX engine_subject_memories_digest_idx ON engine_subject_memories(tenant_id, source_id, subject_id, source_digest) WHERE source_digest IS NOT NULL;

CREATE TABLE engine_threads (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    thread_type TEXT NOT NULL,
    external_thread_id TEXT NULL,
    status TEXT NOT NULL,
    summary_status TEXT NOT NULL,
    pending_record_count BIGINT NOT NULL DEFAULT 0,
    pending_summary_tokens BIGINT NOT NULL DEFAULT 0,
    summary_job_run_id TEXT NULL,
    summary_locked_at TIMESTAMPTZ NULL,
    summary_lock_expires_at TIMESTAMPTZ NULL,
    record_sync_inflight BIGINT NOT NULL DEFAULT 0,
    record_sync_lease_expires_at TIMESTAMPTZ NULL,
    rollup_job_run_id TEXT NULL,
    rollup_locked_at TIMESTAMPTZ NULL,
    rollup_lock_expires_at TIMESTAMPTZ NULL,
    summary_dispatch_pending BOOLEAN NOT NULL DEFAULT false,
    summary_dispatch_version BIGINT NOT NULL DEFAULT 0,
    summary_dispatch_published_version BIGINT NOT NULL DEFAULT 0,
    summary_dispatch_consumed_version BIGINT NOT NULL DEFAULT 0,
    summary_dispatch_requested_at TIMESTAMPTZ NULL,
    summary_dispatch_published_at TIMESTAMPTZ NULL,
    summary_dispatch_consumed_at TIMESTAMPTZ NULL,
    summary_dispatch_last_error TEXT NULL,
    summary_dispatch_last_failed_at TIMESTAMPTZ NULL,
    summary_dispatch_dead_letter_version BIGINT NULL,
    summary_dispatch_dead_lettered_at TIMESTAMPTZ NULL,
    summary_dispatch_recovery_count BIGINT NOT NULL DEFAULT 0,
    summary_dispatch_recovered_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, external_thread_id)
);
CREATE INDEX engine_threads_listing_idx ON engine_threads(tenant_id, source_id, status, updated_at DESC, id);
CREATE INDEX engine_threads_subject_idx ON engine_threads(tenant_id, source_id, subject_id, updated_at DESC);
CREATE INDEX engine_threads_summary_claim_idx ON engine_threads(summary_status, summary_lock_expires_at, updated_at);
CREATE INDEX engine_threads_pending_summary_idx ON engine_threads(summary_status, pending_summary_tokens, updated_at) WHERE summary_status='pending';
CREATE INDEX engine_threads_dispatch_idx ON engine_threads(summary_dispatch_pending, summary_dispatch_requested_at, updated_at) WHERE summary_dispatch_pending;
CREATE INDEX engine_threads_labels_gin_idx ON engine_threads USING GIN ((data->'labels'));

CREATE TABLE engine_records (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES engine_threads(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    external_record_id TEXT NULL,
    role TEXT NOT NULL,
    record_type TEXT NOT NULL,
    summary_status TEXT NOT NULL,
    summary_id TEXT NULL,
    summary_job_run_id TEXT NULL,
    summary_started_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, thread_id, external_record_id)
);
CREATE INDEX engine_records_thread_order_idx ON engine_records(tenant_id, source_id, thread_id, created_at, id);
CREATE INDEX engine_records_summary_idx ON engine_records(tenant_id, source_id, thread_id, summary_status, created_at, id);
CREATE INDEX engine_records_type_idx ON engine_records(tenant_id, source_id, thread_id, record_type, created_at, id);

CREATE TABLE engine_compact_turns (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES engine_threads(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    record_type TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    user_record_id TEXT NOT NULL,
    user_created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, thread_id, record_type, turn_id)
);
CREATE INDEX engine_compact_turns_page_idx ON engine_compact_turns(tenant_id, source_id, thread_id, record_type, user_created_at DESC, turn_id DESC);

CREATE TABLE engine_summaries (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    thread_id TEXT NOT NULL REFERENCES engine_threads(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    summary_type TEXT NOT NULL,
    level BIGINT NOT NULL,
    source_digest TEXT NULL,
    status TEXT NOT NULL,
    rollup_status TEXT NOT NULL,
    subject_memory_summarized BIGINT NOT NULL DEFAULT 0,
    subject_memory_scope_keys TEXT[] NOT NULL DEFAULT '{}',
    rollup_dispatch_pending BOOLEAN NOT NULL DEFAULT false,
    rollup_dispatch_version BIGINT NOT NULL DEFAULT 0,
    rollup_dispatch_published_version BIGINT NOT NULL DEFAULT 0,
    rollup_dispatch_consumed_version BIGINT NOT NULL DEFAULT 0,
    rollup_dispatch_requested_at TIMESTAMPTZ NULL,
    rollup_dispatch_published_at TIMESTAMPTZ NULL,
    rollup_dispatch_consumed_at TIMESTAMPTZ NULL,
    rollup_dispatch_last_error TEXT NULL,
    rollup_dispatch_last_failed_at TIMESTAMPTZ NULL,
    rollup_dispatch_dead_letter_version BIGINT NULL,
    rollup_dispatch_dead_lettered_at TIMESTAMPTZ NULL,
    subject_memory_source_dispatch_pending BOOLEAN NOT NULL DEFAULT false,
    subject_memory_source_dispatch_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_source_dispatch_published_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_source_dispatch_consumed_version BIGINT NOT NULL DEFAULT 0,
    subject_memory_source_dispatch_requested_at TIMESTAMPTZ NULL,
    subject_memory_source_dispatch_published_at TIMESTAMPTZ NULL,
    subject_memory_source_dispatch_consumed_at TIMESTAMPTZ NULL,
    subject_memory_source_dispatch_last_error TEXT NULL,
    subject_memory_source_dispatch_last_failed_at TIMESTAMPTZ NULL,
    subject_memory_source_dispatch_dead_letter_version BIGINT NULL,
    subject_memory_source_dispatch_dead_lettered_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX engine_summaries_thread_idx ON engine_summaries(tenant_id, source_id, thread_id, summary_type, level, created_at DESC);
CREATE INDEX engine_summaries_rollup_idx ON engine_summaries(tenant_id, source_id, thread_id, rollup_status, level, created_at);
CREATE INDEX engine_summaries_subject_idx ON engine_summaries(tenant_id, source_id, subject_id, summary_type, subject_memory_summarized, created_at);
CREATE INDEX engine_summaries_rollup_dispatch_idx ON engine_summaries(rollup_dispatch_pending, rollup_dispatch_requested_at, updated_at) WHERE rollup_dispatch_pending;
CREATE INDEX engine_summaries_subject_dispatch_idx ON engine_summaries(subject_memory_source_dispatch_pending, subject_memory_source_dispatch_requested_at, updated_at) WHERE subject_memory_source_dispatch_pending;

CREATE TABLE engine_thread_snapshots (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    thread_id TEXT NOT NULL REFERENCES engine_threads(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL,
    snapshot_type TEXT NOT NULL,
    status TEXT NOT NULL,
    snapshot_version BIGINT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(tenant_id, source_id, thread_id, turn_id, snapshot_type)
);
CREATE INDEX engine_thread_snapshots_latest_idx ON engine_thread_snapshots(tenant_id, source_id, thread_id, snapshot_type, captured_at DESC, snapshot_version DESC);

CREATE TABLE cloud_agent_lanes (
    ordering_lane_key TEXT PRIMARY KEY,
    next_lane_seq BIGINT NOT NULL,
    active_lane_seq BIGINT NOT NULL,
    version BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE cloud_agent_runs (
    agent_run_id TEXT PRIMARY KEY,
    ordering_lane_key TEXT NOT NULL REFERENCES cloud_agent_lanes(ordering_lane_key),
    lane_seq BIGINT NOT NULL,
    generation BIGINT NOT NULL,
    step_seq BIGINT NOT NULL,
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    version BIGINT NOT NULL,
    claim_token TEXT NULL,
    claim_until TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(ordering_lane_key,lane_seq)
);
CREATE INDEX cloud_agent_runs_claim_idx ON cloud_agent_runs(status,claim_until);
CREATE TABLE engine_cloud_agent_run_states (
    agent_run_id TEXT PRIMARY KEY REFERENCES cloud_agent_runs(agent_run_id) ON DELETE CASCADE,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE TABLE cloud_agent_outbox (
    event_id TEXT PRIMARY KEY,
    agent_run_id TEXT NOT NULL REFERENCES cloud_agent_runs(agent_run_id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    publish_attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX cloud_agent_outbox_ready_idx ON cloud_agent_outbox(status,available_at,event_id);
