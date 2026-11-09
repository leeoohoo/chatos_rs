-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_reliability_migration_runs (
    id TEXT PRIMARY KEY,
    migration_kind TEXT NOT NULL CHECK (migration_kind IN ('m2a_history_repair','m3b_partition_rebuild')),
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running','completed','failed')),
    cursor JSONB NOT NULL DEFAULT '{}'::JSONB,
    statistics JSONB NOT NULL DEFAULT '{}'::JSONB,
    error_detail JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX engine_reliability_migration_runs_scope_idx
ON engine_reliability_migration_runs(tenant_id, source_id, started_at DESC);

CREATE UNIQUE INDEX engine_reliability_migration_one_running_idx
ON engine_reliability_migration_runs(migration_kind, tenant_id, source_id)
WHERE status='running';

CREATE TABLE engine_reliability_migration_actions (
    migration_run_id TEXT NOT NULL REFERENCES engine_reliability_migration_runs(id),
    sequence BIGSERIAL,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    action_kind TEXT NOT NULL CHECK (action_kind IN (
        'restore_record_pending','retire_record_untrusted_partition',
        'retire_summary','retire_subject_memory',
        'enqueue_partition_rebuild','reconcile_partition_rebuild',
        'complete_partition_rebuild','confirm_coverage','confirm_lineage'
    )),
    object_kind TEXT NOT NULL CHECK (object_kind IN (
        'record','thread_summary','subject_memory','thread_partition'
    )),
    object_id TEXT NOT NULL,
    project_partition_key TEXT NOT NULL,
    before_state JSONB NOT NULL,
    after_state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (migration_run_id, sequence),
    UNIQUE (migration_run_id, action_kind, object_kind, object_id),
    CHECK (project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_reliability_migration_actions_scope_idx
ON engine_reliability_migration_actions(
    tenant_id, source_id, project_partition_key, action_kind, object_kind, object_id
);

CREATE TABLE engine_partition_rebuild_backlog (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    project_partition_key TEXT NOT NULL,
    rebuild_kind TEXT NOT NULL CHECK (rebuild_kind IN (
        'thread_l0','thread_rollup','project_memory','agent_memory'
    )),
    status TEXT NOT NULL CHECK (status IN ('pending','running','completed','failed')),
    migration_run_id TEXT NOT NULL REFERENCES engine_reliability_migration_runs(id),
    reason TEXT NOT NULL,
    attempt_count BIGINT NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TIMESTAMPTZ,
    last_error JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, source_id, thread_id, project_partition_key, rebuild_kind),
    FOREIGN KEY (tenant_id, source_id, thread_id)
        REFERENCES engine_threads(tenant_id, source_id, id),
    CHECK (project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_partition_rebuild_backlog_claim_idx
ON engine_partition_rebuild_backlog(status, lease_expires_at, updated_at)
WHERE status IN ('pending','running');

CREATE TABLE engine_partition_rebuild_attempts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    project_partition_key TEXT NOT NULL,
    rebuild_kind TEXT NOT NULL CHECK (rebuild_kind IN (
        'thread_l0','thread_rollup','project_memory','agent_memory'
    )),
    migration_run_id TEXT NOT NULL REFERENCES engine_reliability_migration_runs(id),
    attempt_number BIGINT NOT NULL CHECK (attempt_number > 0),
    status TEXT NOT NULL CHECK (status IN ('running','waiting','completed','failed')),
    lease_owner TEXT NOT NULL,
    lease_token TEXT NOT NULL,
    cursor_before JSONB NOT NULL DEFAULT '{}'::JSONB,
    cursor_after JSONB NOT NULL DEFAULT '{}'::JSONB,
    generation_ids TEXT[] NOT NULL DEFAULT '{}',
    config_snapshots JSONB NOT NULL DEFAULT '[]'::JSONB,
    error_detail JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    UNIQUE (
        tenant_id,source_id,thread_id,project_partition_key,rebuild_kind,attempt_number
    ),
    CHECK (project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_partition_rebuild_attempts_scope_idx
ON engine_partition_rebuild_attempts(
    tenant_id,source_id,thread_id,project_partition_key,rebuild_kind,started_at DESC
);

CREATE TABLE engine_covered_range_resummarization_claims (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    summary_id TEXT NOT NULL,
    job_run_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('running','completed','failed')),
    lease_token TEXT NOT NULL,
    lease_expires_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id,source_id,summary_id),
    FOREIGN KEY (tenant_id,source_id,summary_id)
        REFERENCES engine_summaries(tenant_id,source_id,id),
    FOREIGN KEY (job_run_id) REFERENCES engine_job_runs(id)
);

CREATE INDEX engine_covered_range_resummarization_running_idx
ON engine_covered_range_resummarization_claims(status,lease_expires_at)
WHERE status='running';

CREATE TABLE engine_reliability_operator_actions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    operator_id TEXT NOT NULL,
    action_kind TEXT NOT NULL CHECK (action_kind IN (
        'retire','restore_pending','rearm','resummarize'
    )),
    object_kind TEXT NOT NULL CHECK (object_kind IN (
        'record','thread_summary','subject_memory','thread_partition','rebuild_backlog'
    )),
    object_id TEXT NOT NULL,
    project_partition_key TEXT,
    reason TEXT NOT NULL,
    before_state JSONB NOT NULL,
    after_state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (project_partition_key IS NULL OR project_partition_key='__none__'
        OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_reliability_operator_actions_scope_idx
ON engine_reliability_operator_actions(tenant_id,source_id,created_at DESC,id DESC);

CREATE TABLE engine_coverage_migration_candidates (
    migration_run_id TEXT NOT NULL REFERENCES engine_reliability_migration_runs(id),
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    summary_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    inference_kind TEXT NOT NULL CHECK (inference_kind IN ('explicit_summary_id','unique_range')),
    validation_status TEXT NOT NULL CHECK (validation_status IN ('candidate','confirmed','rejected')),
    evidence JSONB NOT NULL,
    validated_at TIMESTAMPTZ,
    PRIMARY KEY (migration_run_id, summary_id, record_id)
);

CREATE INDEX engine_coverage_migration_candidates_status_idx
ON engine_coverage_migration_candidates(
    tenant_id, source_id, validation_status, summary_id, record_id
);
