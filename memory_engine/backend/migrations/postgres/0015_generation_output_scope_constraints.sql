-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE OR REPLACE FUNCTION engine_validate_generation_commit()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    job_tenant TEXT;
    job_source TEXT;
    job_thread TEXT;
    job_subject TEXT;
    summary_count BIGINT;
    memory_count BIGINT;
    output_thread TEXT;
    output_subject TEXT;
    output_partition TEXT;
    output_profile TEXT;
    output_pipeline TEXT;
BEGIN
    SELECT tenant_id,source_id,thread_id,subject_id
      INTO job_tenant,job_source,job_thread,job_subject
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
            SELECT thread_id,subject_id,project_partition_key,generation_profile,pipeline_revision
              INTO output_thread,output_subject,output_partition,output_profile,output_pipeline
              FROM engine_summaries
             WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
               AND generation_id=NEW.generation_id AND id=NEW.output_id;
            IF job_thread IS NOT NULL AND output_thread IS DISTINCT FROM job_thread THEN
                RAISE EXCEPTION 'summary generation output thread does not match job scope'
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
            SELECT subject_id,project_partition_key,generation_profile,pipeline_revision
              INTO output_subject,output_partition,output_profile,output_pipeline
              FROM engine_subject_memories
             WHERE tenant_id=NEW.tenant_id AND source_id=NEW.source_id
               AND generation_id=NEW.generation_id AND id=NEW.output_id;
        END IF;

        IF job_subject IS NOT NULL AND output_subject IS DISTINCT FROM job_subject THEN
            RAISE EXCEPTION 'generation output subject does not match job scope'
                USING ERRCODE='check_violation';
        END IF;
        IF output_partition IS DISTINCT FROM NEW.project_partition_key THEN
            RAISE EXCEPTION 'generation output partition does not match registry scope'
                USING ERRCODE='check_violation';
        END IF;
        IF output_profile IS DISTINCT FROM NEW.config_snapshot->>'generation_profile'
           OR output_pipeline IS DISTINCT FROM NEW.config_snapshot->>'pipeline_revision' THEN
            RAISE EXCEPTION 'generation output configuration does not match frozen snapshot'
                USING ERRCODE='check_violation';
        END IF;
    END IF;

    RETURN NEW;
END;
$$;
