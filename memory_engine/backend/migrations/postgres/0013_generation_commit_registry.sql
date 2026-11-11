-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_generation_commits (
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    generation_id         TEXT        NOT NULL,
    job_run_id            TEXT        NOT NULL REFERENCES engine_job_runs(id),
    generation_step_key   TEXT        NOT NULL,
    generation_kind       TEXT        NOT NULL CHECK (generation_kind IN (
        'thread_summary','thread_rollup','project_memory',
        'agent_memory','subject_memory_rollup'
    )),
    input_digest          TEXT        NOT NULL,
    input_manifest        JSONB       NOT NULL,
    config_snapshot       JSONB       NOT NULL,
    project_partition_key TEXT        NULL,
    status                TEXT        NOT NULL CHECK (status IN ('prepared','running','committed','abandoned')),
    attempt_count         BIGINT      NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_owner           TEXT        NULL,
    lease_token           TEXT        NULL,
    lease_acquired_at     TIMESTAMPTZ NULL,
    lease_expires_at      TIMESTAMPTZ NULL,
    last_error_code       TEXT        NULL,
    last_error_detail     JSONB       NULL,
    last_failed_at        TIMESTAMPTZ NULL,
    output_kind           TEXT        NULL CHECK (
        output_kind IS NULL OR output_kind IN ('thread_summary','subject_memory')
    ),
    output_id             TEXT        NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    committed_at          TIMESTAMPTZ NULL,
    abandoned_at          TIMESTAMPTZ NULL,
    abandoned_reason      TEXT        NULL,
    CONSTRAINT engine_generation_output_state_check CHECK (
        (status='committed' AND output_kind IS NOT NULL AND output_id IS NOT NULL)
        OR (status<>'committed' AND output_kind IS NULL AND output_id IS NULL)
    ),
    CONSTRAINT engine_generation_partition_check CHECK (
        project_partition_key IS NULL OR project_partition_key='__none__'
        OR project_partition_key LIKE 'project:%'
    ),
    PRIMARY KEY (tenant_id,source_id,generation_id),
    UNIQUE (tenant_id,source_id,job_run_id,generation_step_key)
);

CREATE INDEX engine_generation_commits_claim_idx
    ON engine_generation_commits(status,lease_expires_at,created_at)
    WHERE status IN ('prepared','running');

CREATE FUNCTION engine_guard_generation_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF (NEW.job_run_id,NEW.generation_step_key,NEW.generation_kind,NEW.input_digest,
        NEW.input_manifest,NEW.config_snapshot,NEW.project_partition_key)
       IS DISTINCT FROM
       (OLD.job_run_id,OLD.generation_step_key,OLD.generation_kind,OLD.input_digest,
        OLD.input_manifest,OLD.config_snapshot,OLD.project_partition_key) THEN
        RAISE EXCEPTION 'generation frozen input and configuration are immutable'
            USING ERRCODE='check_violation';
    END IF;

    IF OLD.status='committed' OR OLD.status='abandoned' THEN
        IF NEW IS DISTINCT FROM OLD THEN
            RAISE EXCEPTION 'terminal generation state is immutable'
                USING ERRCODE='check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status=NEW.status THEN
        IF OLD.status='running'
           AND NEW.lease_token IS DISTINCT FROM OLD.lease_token
           AND (OLD.lease_expires_at IS NULL OR OLD.lease_expires_at>now()) THEN
            RAISE EXCEPTION 'active generation lease cannot be replaced'
                USING ERRCODE='check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF (OLD.status='prepared' AND NEW.status='running')
       OR (OLD.status='running' AND NEW.status IN ('prepared','committed','abandoned'))
       OR (OLD.status='prepared' AND NEW.status='abandoned') THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'invalid generation state transition: % -> %',OLD.status,NEW.status
        USING ERRCODE='check_violation';
END;
$$;

CREATE TRIGGER engine_generation_transition_guard
BEFORE UPDATE ON engine_generation_commits
FOR EACH ROW
EXECUTE FUNCTION engine_guard_generation_transition();

ALTER TABLE engine_summaries
    ADD CONSTRAINT engine_summaries_generation_fkey
    FOREIGN KEY (tenant_id,source_id,generation_id)
    REFERENCES engine_generation_commits(tenant_id,source_id,generation_id)
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE engine_subject_memories
    ADD CONSTRAINT engine_subject_memories_generation_fkey
    FOREIGN KEY (tenant_id,source_id,generation_id)
    REFERENCES engine_generation_commits(tenant_id,source_id,generation_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE UNIQUE INDEX engine_summaries_generation_unique_idx
    ON engine_summaries(tenant_id,source_id,generation_id)
    WHERE generation_id IS NOT NULL;

CREATE UNIQUE INDEX engine_subject_memories_generation_unique_idx
    ON engine_subject_memories(tenant_id,source_id,generation_id)
    WHERE generation_id IS NOT NULL;

CREATE FUNCTION engine_validate_generation_commit()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    job_tenant TEXT;
    job_source TEXT;
    summary_count BIGINT;
    memory_count BIGINT;
BEGIN
    SELECT tenant_id,source_id INTO job_tenant,job_source
      FROM engine_job_runs
     WHERE id=NEW.job_run_id;

    IF NOT FOUND OR job_tenant IS DISTINCT FROM NEW.tenant_id
                 OR job_source IS DISTINCT FROM NEW.source_id THEN
        RAISE EXCEPTION 'generation job scope does not match tenant/source'
            USING ERRCODE='foreign_key_violation';
    END IF;

    IF NEW.status='committed' THEN
        SELECT COUNT(*) INTO summary_count
          FROM engine_summaries
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
           AND generation_id=NEW.generation_id;
        SELECT COUNT(*) INTO memory_count
          FROM engine_subject_memories
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
           AND generation_id=NEW.generation_id;

        IF NEW.output_kind='thread_summary' THEN
            IF summary_count<>1 OR memory_count<>0 OR NOT EXISTS (
                SELECT 1 FROM engine_summaries
                 WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
                   AND generation_id=NEW.generation_id AND id=NEW.output_id
            ) THEN
                RAISE EXCEPTION 'committed generation must reference exactly one thread summary output'
                    USING ERRCODE='check_violation';
            END IF;
        ELSIF NEW.output_kind='subject_memory' THEN
            IF memory_count<>1 OR summary_count<>0 OR NOT EXISTS (
                SELECT 1 FROM engine_subject_memories
                 WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
                   AND generation_id=NEW.generation_id AND id=NEW.output_id
            ) THEN
                RAISE EXCEPTION 'committed generation must reference exactly one subject memory output'
                    USING ERRCODE='check_violation';
            END IF;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER engine_generation_commit_validation
AFTER INSERT OR UPDATE ON engine_generation_commits
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION engine_validate_generation_commit();

CREATE FUNCTION engine_validate_generation_output()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    registry_status TEXT;
    registry_output_kind TEXT;
    registry_output_id TEXT;
    expected_kind TEXT;
BEGIN
    IF NEW.generation_id IS NULL THEN
        RETURN NEW;
    END IF;

    expected_kind := CASE TG_TABLE_NAME
        WHEN 'engine_summaries' THEN 'thread_summary'
        WHEN 'engine_subject_memories' THEN 'subject_memory'
        ELSE NULL
    END;
    SELECT status,output_kind,output_id
      INTO registry_status,registry_output_kind,registry_output_id
      FROM engine_generation_commits
     WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
       AND generation_id=NEW.generation_id;

    IF NOT FOUND OR registry_status<>'committed'
       OR registry_output_kind<>expected_kind OR registry_output_id<>NEW.id THEN
        RAISE EXCEPTION 'generated output must match a committed generation registry row'
            USING ERRCODE='check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER engine_summary_generation_output_validation
AFTER INSERT OR UPDATE OF generation_id ON engine_summaries
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION engine_validate_generation_output();

CREATE CONSTRAINT TRIGGER engine_subject_memory_generation_output_validation
AFTER INSERT OR UPDATE OF generation_id ON engine_subject_memories
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION engine_validate_generation_output();
