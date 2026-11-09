-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- M1a additive foundation. Later M1 stages attach generation/coverage/lineage
-- constraints after the compatible writers have shipped. Every new write path
-- remains behind Configuration Center feature flags.

ALTER TABLE engine_records
    ADD COLUMN project_partition_key TEXT NULL;

ALTER TABLE engine_summaries
    ADD COLUMN generation_id TEXT NULL,
    ADD COLUMN generation_reason TEXT NULL,
    ADD COLUMN generation_profile TEXT NULL,
    ADD COLUMN project_partition_key TEXT NULL,
    ADD COLUMN recall_eligible BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN retired_reason TEXT NULL,
    ADD COLUMN payload_schema TEXT NULL,
    ADD COLUMN structured_payload JSONB NULL,
    ADD COLUMN pipeline_revision TEXT NULL;

ALTER TABLE engine_subject_memories
    ADD COLUMN generation_id TEXT NULL,
    ADD COLUMN generation_profile TEXT NULL,
    ADD COLUMN project_partition_key TEXT NULL,
    ADD COLUMN recall_eligible BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN retired_reason TEXT NULL,
    ADD COLUMN payload_schema TEXT NULL,
    ADD COLUMN structured_payload JSONB NULL,
    ADD COLUMN pipeline_revision TEXT NULL;

UPDATE engine_records
   SET project_partition_key = CASE
       WHEN BTRIM(data #>> '{metadata,memory_project_id}') LIKE 'project:%'
           THEN BTRIM(data #>> '{metadata,memory_project_id}')
       ELSE 'project:' || BTRIM(data #>> '{metadata,memory_project_id}')
   END
 WHERE NULLIF(BTRIM(data #>> '{metadata,memory_project_id}'),'') IS NOT NULL;

UPDATE engine_summaries
   SET project_partition_key = BTRIM(data #>> '{metadata,project_subject_id}')
 WHERE NULLIF(BTRIM(data #>> '{metadata,project_subject_id}'),'') IS NOT NULL;

CREATE INDEX engine_records_partition_pending_idx
    ON engine_records(tenant_id,source_id,thread_id,project_partition_key,summary_status,created_at,id);
CREATE INDEX engine_summaries_partition_recall_idx
    ON engine_summaries(tenant_id,source_id,thread_id,project_partition_key,recall_eligible,level,created_at,id);
CREATE INDEX engine_summaries_generation_profile_idx
    ON engine_summaries(tenant_id,source_id,generation_profile,recall_eligible,created_at,id);
CREATE INDEX engine_subject_memories_generation_profile_idx
    ON engine_subject_memories(tenant_id,source_id,subject_id,generation_profile,recall_eligible,level,created_at,id);

CREATE TABLE engine_thread_summary_partitions (
    tenant_id                           TEXT        NOT NULL,
    source_id                           TEXT        NOT NULL,
    thread_id                           TEXT        NOT NULL,
    project_partition_key               TEXT        NOT NULL,
    summary_status                      TEXT        NOT NULL,
    pending_record_count                BIGINT      NOT NULL DEFAULT 0 CHECK (pending_record_count >= 0),
    pending_summary_tokens              BIGINT      NOT NULL DEFAULT 0 CHECK (pending_summary_tokens >= 0),
    summary_job_run_id                  TEXT        NULL,
    summary_locked_at                   TIMESTAMPTZ NULL,
    summary_lock_expires_at             TIMESTAMPTZ NULL,
    rollup_status                       TEXT        NOT NULL DEFAULT 'pending',
    rollup_job_run_id                   TEXT        NULL,
    rollup_locked_at                    TIMESTAMPTZ NULL,
    rollup_lock_expires_at              TIMESTAMPTZ NULL,
    migration_state                     TEXT        NOT NULL DEFAULT 'backfilling'
        CHECK (migration_state IN ('backfilling','ready','authoritative')),
    authoritative_writer                TEXT        NOT NULL DEFAULT 'v1'
        CHECK (authoritative_writer IN ('v1','v2')),
    summary_dispatch_pending             BOOLEAN     NOT NULL DEFAULT false,
    summary_dispatch_version             BIGINT      NOT NULL DEFAULT 0 CHECK (summary_dispatch_version >= 0),
    summary_dispatch_published_version   BIGINT      NOT NULL DEFAULT 0 CHECK (summary_dispatch_published_version >= 0),
    summary_dispatch_consumed_version    BIGINT      NOT NULL DEFAULT 0 CHECK (summary_dispatch_consumed_version >= 0),
    summary_dispatch_force_version       BIGINT      NOT NULL DEFAULT 0 CHECK (summary_dispatch_force_version >= 0),
    summary_dispatch_requested_at        TIMESTAMPTZ NULL,
    summary_dispatch_published_at        TIMESTAMPTZ NULL,
    summary_dispatch_consumed_at         TIMESTAMPTZ NULL,
    summary_dispatch_last_error          TEXT        NULL,
    summary_dispatch_last_failed_at      TIMESTAMPTZ NULL,
    summary_dispatch_dead_letter_version BIGINT      NULL,
    summary_dispatch_dead_lettered_at    TIMESTAMPTZ NULL,
    summary_dispatch_recovery_count      BIGINT      NOT NULL DEFAULT 0 CHECK (summary_dispatch_recovery_count >= 0),
    summary_dispatch_recovered_at        TIMESTAMPTZ NULL,
    created_at                           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id,source_id,thread_id,project_partition_key),
    CONSTRAINT engine_thread_partition_writer_cutover_check
        CHECK ((migration_state='authoritative') = (authoritative_writer='v2')),
    CONSTRAINT engine_thread_partition_scope_fkey
        FOREIGN KEY (tenant_id,source_id,thread_id)
        REFERENCES engine_threads(tenant_id,source_id,id) ON DELETE CASCADE,
    CONSTRAINT engine_thread_partition_key_check
        CHECK (project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_thread_summary_partitions_claim_idx
    ON engine_thread_summary_partitions(summary_status,summary_lock_expires_at,updated_at);
CREATE INDEX engine_thread_summary_partitions_dispatch_idx
    ON engine_thread_summary_partitions(summary_dispatch_requested_at,updated_at)
    WHERE summary_dispatch_pending;
CREATE INDEX engine_thread_summary_partitions_rollup_claim_idx
    ON engine_thread_summary_partitions(rollup_status,rollup_lock_expires_at,updated_at);

-- Only explicit historical project identities can be backfilled deterministically here.
-- __none__ and missing identity are intentionally not conflated.
INSERT INTO engine_thread_summary_partitions(
    tenant_id,source_id,thread_id,project_partition_key,summary_status,
    pending_record_count,pending_summary_tokens,migration_state,authoritative_writer
)
SELECT tenant_id,source_id,thread_id,project_partition_key,
       CASE WHEN COUNT(*) FILTER (WHERE summary_status='pending')>0 THEN 'pending' ELSE 'idle' END,
       COUNT(*) FILTER (WHERE summary_status='pending'),0,'backfilling','v1'
  FROM engine_records
 WHERE project_partition_key IS NOT NULL
 GROUP BY tenant_id,source_id,thread_id,project_partition_key
ON CONFLICT DO NOTHING;

CREATE TABLE engine_derived_event_outbox (
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    event_id              TEXT        NOT NULL,
    event_kind            TEXT        NOT NULL CHECK (event_kind IN (
        'thread_summary_requested','thread_rollup_requested',
        'subject_memory_source_requested','subject_memory_rollup_requested',
        'derivation_rebuild_requested'
    )),
    aggregate_kind        TEXT        NOT NULL CHECK (aggregate_kind IN (
        'thread_partition','thread_summary','subject_memory','subject_memory_scope'
    )),
    aggregate_id          TEXT        NOT NULL,
    project_partition_key TEXT        NULL,
    dispatch_version      BIGINT      NOT NULL CHECK (dispatch_version > 0),
    generation_id         TEXT        NULL,
    payload               JSONB       NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','claimed','published','dead_letter')),
    attempt_count         BIGINT      NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_owner           TEXT        NULL,
    lease_token           TEXT        NULL,
    lease_expires_at      TIMESTAMPTZ NULL,
    next_attempt_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error_code       TEXT        NULL,
    last_error_detail     JSONB       NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at          TIMESTAMPTZ NULL,
    dead_lettered_at      TIMESTAMPTZ NULL,
    PRIMARY KEY (tenant_id,source_id,event_id),
    UNIQUE (tenant_id,source_id,event_kind,aggregate_kind,aggregate_id,dispatch_version),
    CHECK (project_partition_key IS NULL OR project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE INDEX engine_derived_event_outbox_claim_idx
    ON engine_derived_event_outbox(status,next_attempt_at,lease_expires_at,created_at)
    WHERE status IN ('pending','claimed');
