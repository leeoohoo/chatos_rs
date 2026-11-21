use super::subject_commit::validate_frozen_source_ids;
use super::*;

pub(super) fn validate_l0_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadL0GenerationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_summary" || output.level != 0 {
        return Err("only a thread L0 generation can finalize source records".to_string());
    }
    if finalization.record_ids.is_empty()
        || finalization.record_ids.len() != output.source_record_count
    {
        return Err("thread L0 finalization record count does not match output".to_string());
    }
    let unique = finalization
        .record_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.record_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err("thread L0 finalization record ids must be unique and non-empty".to_string());
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "thread L0 generation input manifest has no inputs".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "thread L0 generation input manifest has an invalid id".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if frozen_ids.as_slice()
        != finalization
            .record_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
    {
        return Err("thread L0 finalization records do not match frozen input order".to_string());
    }
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err("thread L0 finalization requires a successful terminal job result".to_string());
    }
    Ok(())
}

pub(super) fn validate_resummarization_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_summary"
        || output.level != 0
        || output.generation_reason != "resummarize"
    {
        return Err(
            "covered-range resummarization requires a thread L0 resummarize generation".to_string(),
        );
    }
    if finalization.previous_summary_id.trim().is_empty()
        || finalization.record_ids.is_empty()
        || finalization.record_ids.len() != output.source_record_count
    {
        return Err(
            "covered-range resummarization identity or record count is invalid".to_string(),
        );
    }
    let unique = finalization
        .record_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.record_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err(
            "covered-range resummarization record ids must be unique and non-empty".to_string(),
        );
    }
    validate_frozen_source_ids(generation, finalization.record_ids, "resummarization")?;
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "covered-range resummarization requires a successful terminal job result".to_string(),
        );
    }
    Ok(())
}

pub(super) async fn supersede_resummarized_range(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    replacement: &EngineSummary,
    project_partition_key: &str,
    finalization: &ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    let claim = sqlx::query_scalar::<_, String>(
        "SELECT lease_token FROM engine_covered_range_resummarization_claims \
         WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 AND job_run_id=$4 \
           AND status='running' AND lease_expires_at>now() FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&generation.job_run_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if claim.is_none() {
        return Err("covered-range resummarization claim is missing or expired".to_string());
    }
    let previous = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND thread_id=$4 AND project_partition_key=$5 AND level=0 AND status='done' \
         AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&replacement.thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "covered-range source summary is no longer active".to_string())?;
    let mut previous: EngineSummary = decode(previous)?;

    let covered_ids = sqlx::query_scalar::<_, String>(
        "SELECT record_id FROM engine_summary_record_coverage WHERE tenant_id=$1 AND source_id=$2 \
         AND summary_id=$3 AND status='active' ORDER BY ordinal FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if covered_ids != finalization.record_ids {
        return Err(
            "covered-range active coverage changed after generation was frozen".to_string(),
        );
    }

    let owned_records: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND project_partition_key=$4 AND id=ANY($5) AND summary_status='summarized' \
         AND summary_id=$6 AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&replacement.thread_id)
    .bind(project_partition_key)
    .bind(finalization.record_ids)
    .bind(finalization.previous_summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if owned_records.len() != finalization.record_ids.len() {
        return Err("covered-range source records changed after generation was frozen".to_string());
    }

    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_summary_record_coverage SET status='superseded',retired_at=$4, \
         retired_reason='covered_range_resummarized' WHERE tenant_id=$1 AND source_id=$2 \
         AND summary_id=$3 AND status='active'",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;

    previous.status = "superseded".to_string();
    previous.updated_at = now.clone();
    let retired = sqlx::query(
        "UPDATE engine_summaries SET status='superseded',recall_eligible=false,retired_at=$4, \
         retired_reason='covered_range_resummarized',rollup_dispatch_pending=false, \
         subject_memory_source_dispatch_pending=false,updated_at=$4,data=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='done' AND recall_eligible",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .bind(json(&previous)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if retired.rows_affected() != 1 {
        return Err("covered-range source summary changed during replacement".to_string());
    }

    // Every active descendant is invalid once its source range changes. Retire the complete
    // downstream frontier in this transaction; the replacement summary's durable outbox events
    // rebuild it from the new canonical generation.
    sqlx::query(
        "WITH RECURSIVE affected(output_kind,output_id) AS ( \
           SELECT output_kind,output_id FROM engine_memory_derivation_edges \
            WHERE tenant_id=$1 AND source_id=$2 AND input_kind='thread_summary' AND input_id=$3 \
              AND status='active' \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.output_kind AND edge.input_id=input.output_id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_summaries summary SET status='retired',recall_eligible=false, \
             retired_at=$4,retired_reason='upstream_resummarized',updated_at=$4, \
             rollup_dispatch_pending=false,subject_memory_source_dispatch_pending=false, \
             data=jsonb_set(data,'{status}',to_jsonb('retired'::text),true) \
          FROM affected WHERE affected.output_kind='thread_summary' \
            AND summary.tenant_id=$1 AND summary.source_id=$2 AND summary.id=affected.output_id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(output_kind,output_id) AS ( \
           SELECT output_kind,output_id FROM engine_memory_derivation_edges \
            WHERE tenant_id=$1 AND source_id=$2 AND input_kind='thread_summary' AND input_id=$3 \
              AND status='active' \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.output_kind AND edge.input_id=input.output_id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_subject_memories memory SET status='retired',recall_eligible=false, \
             retired_at=$4,retired_reason='upstream_resummarized',updated_at=$4, \
             data=jsonb_set(data,'{status}',to_jsonb('retired'::text),true) \
          FROM affected WHERE affected.output_kind='subject_memory' \
            AND memory.tenant_id=$1 AND memory.source_id=$2 AND memory.id=affected.output_id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(kind,id) AS ( \
           SELECT 'thread_summary'::text,$3::text \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.kind AND edge.input_id=input.id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_memory_derivation_edges edge SET status='superseded',retired_at=$4, \
             retired_reason='upstream_resummarized' FROM affected input \
          WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
            AND edge.input_kind=input.kind AND edge.input_id=input.id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(kind,id) AS ( \
           SELECT 'thread_summary'::text,$3::text \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.kind AND edge.input_id=input.id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 \
              AND edge.status IN ('active','superseded') \
         ) UPDATE engine_memory_fact_projections fact SET status='retired',retired_at=$4 \
           FROM affected WHERE fact.tenant_id=$1 AND fact.source_id=$2 \
             AND fact.status IN ('active','uncertain') \
             AND fact.output_kind=affected.kind AND fact.output_id=affected.id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) async fn finalize_thread_l0_resummarization(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    let now = now_rfc3339();
    for record_id in finalization.record_ids {
        let row = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
             AND thread_id=$4 AND project_partition_key=$5 AND summary_status='summarized' \
             AND summary_id=$6 AND recall_eligible FOR UPDATE",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(record_id)
        .bind(&summary.thread_id)
        .bind(project_partition_key)
        .bind(finalization.previous_summary_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "covered-range source record changed before replacement commit".to_string()
        })?;
        let mut record: EngineRecord = decode(row)?;
        record.summary_id = Some(summary.id.clone());
        record.summarized_at = Some(now.clone());
        sqlx::query(
            "UPDATE engine_records SET summary_id=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_id=$6 \
               AND summary_status='summarized' AND recall_eligible",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(record_id)
        .bind(&summary.id)
        .bind(json(&record)?)
        .bind(finalization.previous_summary_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    lineage::insert_thread_l0_coverage(
        tx,
        generation,
        summary,
        project_partition_key,
        finalization.record_ids,
    )
    .await?;
    let completed = sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET status='completed', \
         completed_at=$5,updated_at=$5 WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 \
         AND job_run_id=$4 AND status='running'",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&generation.job_run_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if completed.rows_affected() != 1 {
        return Err("covered-range resummarization claim changed before commit".to_string());
    }
    finish_generation_job(
        tx,
        generation,
        &summary.thread_id,
        &summary.id,
        finalization.job_result,
        &now,
    )
    .await
}
