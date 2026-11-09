-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_summary_record_coverage (
    id                    TEXT        NOT NULL,
    tenant_id             TEXT        NOT NULL,
    source_id             TEXT        NOT NULL,
    thread_id             TEXT        NOT NULL,
    project_partition_key TEXT        NOT NULL,
    summary_id            TEXT        NOT NULL,
    record_id             TEXT        NOT NULL,
    ordinal               BIGINT      NOT NULL CHECK (ordinal >= 0),
    source_ref            TEXT        NOT NULL,
    coverage_kind         TEXT        NOT NULL CHECK (coverage_kind = 'full'),
    status                TEXT        NOT NULL CHECK (status IN ('active','superseded','retired')),
    generation_id         TEXT        NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at            TIMESTAMPTZ NULL,
    retired_reason        TEXT        NULL,
    PRIMARY KEY (tenant_id,source_id,id),
    UNIQUE (tenant_id,source_id,summary_id,record_id),
    UNIQUE (tenant_id,source_id,summary_id,ordinal),
    UNIQUE (tenant_id,source_id,summary_id,source_ref),
    FOREIGN KEY (tenant_id,source_id,summary_id)
        REFERENCES engine_summaries(tenant_id,source_id,id),
    FOREIGN KEY (tenant_id,source_id,record_id)
        REFERENCES engine_records(tenant_id,source_id,id),
    FOREIGN KEY (tenant_id,source_id,generation_id)
        REFERENCES engine_generation_commits(tenant_id,source_id,generation_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (project_partition_key='__none__' OR project_partition_key LIKE 'project:%')
);

CREATE UNIQUE INDEX engine_summary_record_one_active_idx
    ON engine_summary_record_coverage(tenant_id,source_id,record_id)
    WHERE status='active';
CREATE INDEX engine_summary_record_coverage_summary_idx
    ON engine_summary_record_coverage(tenant_id,source_id,summary_id,status,ordinal);

CREATE OR REPLACE FUNCTION engine_validate_summary_record_coverage()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    summary_thread TEXT;
    summary_partition TEXT;
    summary_generation TEXT;
    summary_recall BOOLEAN;
    record_thread TEXT;
    record_partition TEXT;
BEGIN
    SELECT thread_id,project_partition_key,generation_id,recall_eligible
      INTO summary_thread,summary_partition,summary_generation,summary_recall
      FROM engine_summaries
     WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.summary_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'coverage summary does not exist in tenant/source scope'
            USING ERRCODE='foreign_key_violation';
    END IF;

    SELECT thread_id,project_partition_key
      INTO record_thread,record_partition
      FROM engine_records
     WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.record_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'coverage record does not exist in tenant/source scope'
            USING ERRCODE='foreign_key_violation';
    END IF;

    IF summary_thread IS DISTINCT FROM NEW.thread_id
       OR record_thread IS DISTINCT FROM NEW.thread_id
       OR summary_partition IS DISTINCT FROM NEW.project_partition_key
       OR record_partition IS DISTINCT FROM NEW.project_partition_key
       OR summary_generation IS DISTINCT FROM NEW.generation_id THEN
        RAISE EXCEPTION 'coverage thread, partition, or generation does not match source/output'
            USING ERRCODE='check_violation';
    END IF;
    IF NEW.status='active' AND NOT summary_recall THEN
        RAISE EXCEPTION 'active coverage cannot reference an ineligible summary'
            USING ERRCODE='check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER engine_summary_record_coverage_validation
AFTER INSERT OR UPDATE ON engine_summary_record_coverage
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION engine_validate_summary_record_coverage();

CREATE TABLE engine_memory_derivation_edges (
    id                          TEXT        NOT NULL,
    tenant_id                   TEXT        NOT NULL,
    source_id                   TEXT        NOT NULL,
    derivation_kind             TEXT        NOT NULL CHECK (derivation_kind IN (
        'thread_rollup','project_memory','agent_memory','subject_memory_rollup'
    )),
    output_scope_key            TEXT        NOT NULL,
    input_project_partition_key TEXT        NOT NULL,
    output_kind                 TEXT        NOT NULL CHECK (output_kind IN (
        'thread_summary','subject_memory'
    )),
    output_id                   TEXT        NOT NULL,
    input_kind                  TEXT        NOT NULL CHECK (input_kind IN (
        'thread_summary','subject_memory'
    )),
    input_id                    TEXT        NOT NULL,
    ordinal                     BIGINT      NOT NULL CHECK (ordinal >= 0),
    status                      TEXT        NOT NULL CHECK (status IN ('active','superseded','retired')),
    generation_id               TEXT        NOT NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at                  TIMESTAMPTZ NULL,
    retired_reason              TEXT        NULL,
    PRIMARY KEY (tenant_id,source_id,id),
    UNIQUE (tenant_id,source_id,output_kind,output_id,input_kind,input_id),
    UNIQUE (tenant_id,source_id,output_kind,output_id,ordinal),
    FOREIGN KEY (tenant_id,source_id,generation_id)
        REFERENCES engine_generation_commits(tenant_id,source_id,generation_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (input_project_partition_key='__none__' OR input_project_partition_key LIKE 'project:%')
);

CREATE UNIQUE INDEX engine_memory_derivation_one_active_idx
    ON engine_memory_derivation_edges(
        tenant_id,source_id,input_kind,input_id,derivation_kind,output_scope_key
    ) WHERE status='active';
CREATE INDEX engine_memory_derivation_output_idx
    ON engine_memory_derivation_edges(tenant_id,source_id,output_kind,output_id,status,ordinal);

CREATE OR REPLACE FUNCTION engine_validate_memory_derivation_edge()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    input_thread TEXT;
    input_subject TEXT;
    input_partition TEXT;
    input_profile TEXT;
    input_memory_type TEXT;
    input_level BIGINT;
    output_thread TEXT;
    output_subject TEXT;
    output_partition TEXT;
    output_profile TEXT;
    output_memory_type TEXT;
    output_level BIGINT;
    output_generation TEXT;
    output_recall BOOLEAN;
    output_status TEXT;
BEGIN
    IF NEW.input_kind='thread_summary' THEN
        SELECT thread_id,subject_id,project_partition_key,generation_profile,level
          INTO input_thread,input_subject,input_partition,input_profile,input_level
          FROM engine_summaries
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.input_id;
    ELSE
        SELECT subject_id,project_partition_key,generation_profile,memory_type,level
          INTO input_subject,input_partition,input_profile,input_memory_type,input_level
          FROM engine_subject_memories
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.input_id;
    END IF;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'derivation input does not exist in declared kind and scope'
            USING ERRCODE='foreign_key_violation';
    END IF;

    IF NEW.output_kind='thread_summary' THEN
        SELECT thread_id,subject_id,project_partition_key,generation_profile,level,
               generation_id,recall_eligible,status
          INTO output_thread,output_subject,output_partition,output_profile,output_level,
               output_generation,output_recall,output_status
          FROM engine_summaries
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.output_id;
    ELSE
        SELECT subject_id,project_partition_key,generation_profile,memory_type,level,
               generation_id,recall_eligible,status
          INTO output_subject,output_partition,output_profile,output_memory_type,output_level,
               output_generation,output_recall,output_status
          FROM engine_subject_memories
         WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id AND id=NEW.output_id;
    END IF;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'derivation output does not exist in declared kind and scope'
            USING ERRCODE='foreign_key_violation';
    END IF;

    IF input_partition IS DISTINCT FROM NEW.input_project_partition_key
       OR output_generation IS DISTINCT FROM NEW.generation_id THEN
        RAISE EXCEPTION 'derivation partition or generation identity mismatch'
            USING ERRCODE='check_violation';
    END IF;
    IF NEW.status='active' AND (NOT output_recall OR output_status IN ('retired','deleted')) THEN
        RAISE EXCEPTION 'active derivation cannot reference an ineligible output'
            USING ERRCODE='check_violation';
    END IF;

    IF NEW.derivation_kind='thread_rollup' THEN
        IF NEW.input_kind<>'thread_summary' OR NEW.output_kind<>'thread_summary'
           OR input_thread IS DISTINCT FROM output_thread
           OR input_partition IS DISTINCT FROM output_partition
           OR output_level<=input_level THEN
            RAISE EXCEPTION 'invalid thread rollup derivation'
                USING ERRCODE='check_violation';
        END IF;
    ELSIF NEW.derivation_kind='project_memory' THEN
        IF NEW.input_kind<>'thread_summary' OR NEW.output_kind<>'subject_memory'
           OR output_memory_type<>'project_summary'
           OR input_partition NOT LIKE 'project:%'
           OR output_subject IS DISTINCT FROM input_partition
           OR output_partition IS DISTINCT FROM input_partition THEN
            RAISE EXCEPTION 'invalid project memory derivation'
                USING ERRCODE='check_violation';
        END IF;
    ELSIF NEW.derivation_kind='agent_memory' THEN
        IF NEW.input_kind<>'thread_summary' OR NEW.output_kind<>'subject_memory'
           OR output_memory_type<>'agent_memory'
           OR output_subject IS DISTINCT FROM input_subject THEN
            RAISE EXCEPTION 'invalid agent memory derivation'
                USING ERRCODE='check_violation';
        END IF;
    ELSIF NEW.derivation_kind='subject_memory_rollup' THEN
        IF NEW.input_kind<>'subject_memory' OR NEW.output_kind<>'subject_memory'
           OR output_subject IS DISTINCT FROM input_subject
           OR output_memory_type IS DISTINCT FROM input_memory_type
           OR output_level<=input_level THEN
            RAISE EXCEPTION 'invalid subject memory rollup derivation'
                USING ERRCODE='check_violation';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER engine_memory_derivation_edge_validation
AFTER INSERT OR UPDATE ON engine_memory_derivation_edges
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION engine_validate_memory_derivation_edge();
