use super::*;

pub(super) async fn finalize_subject_memory_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    finalization: SubjectMemoryGenerationFinalization<'_>,
) -> Result<(), String> {
    let now = now_rfc3339();
    match finalization.sources {
        SubjectMemoryGenerationSources::Summaries {
            summary_ids,
            scope_key,
        } => {
            finalize_subject_summary_sources(tx, generation, summary_ids, scope_key, &now).await?;
            lineage::insert_subject_summary_edges(tx, generation, output, summary_ids).await?;
        }
        SubjectMemoryGenerationSources::SubjectMemories { memory_ids } => {
            finalize_subject_memory_sources(tx, generation, output, memory_ids, &now).await?;
            lineage::insert_subject_memory_rollup_edges(tx, generation, output, memory_ids).await?;
        }
    }
    if let Some(claim) = finalization.scope_claim {
        let released = sqlx::query(
            "UPDATE engine_subject_memory_scopes SET subject_memory_status='idle',lock_owner=NULL, \
             lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
             AND scope_key=$3 AND status='active' AND ( \
               (subject_memory_status='running' AND lock_owner=$4) OR \
               (subject_memory_status<>'running' AND (lock_owner IS NULL OR lock_owner=$4)))",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(claim.scope_key)
        .bind(claim.lock_owner)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if released.rows_affected() != 1 {
            return Err("subject memory scope claim was lost before final commit".to_string());
        }
    }
    finish_subject_generation_job(
        tx,
        generation,
        &output.subject_id,
        finalization.job_result,
        &now,
    )
    .await
}

async fn finalize_subject_summary_sources(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary_ids: &[String],
    scope_key: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 \
         AND id=ANY($3) AND status='done' AND recall_eligible AND \
         (($4::text IS NULL AND subject_memory_summarized<>1) OR \
          ($4::text IS NOT NULL AND NOT ($4=ANY(subject_memory_scope_keys)))) FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(summary_ids)
    .bind(scope_key)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != summary_ids.len() {
        return Err("subject memory finalization did not own every frozen summary".to_string());
    }
    for row in rows {
        let mut source: EngineSummary = decode(row)?;
        source.subject_memory_summarized = 1;
        source.subject_memory_summarized_at = Some(now.to_string());
        source.updated_at = now.to_string();
        let updated = if let Some(scope_key) = scope_key {
            sqlx::query(
                "UPDATE engine_summaries SET subject_memory_summarized=1, \
                 subject_memory_scope_keys=array_append(subject_memory_scope_keys,$4), \
                 updated_at=$5,data=$6 WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
                 AND NOT ($4=ANY(subject_memory_scope_keys))",
            )
            .bind(&source.tenant_id)
            .bind(&source.source_id)
            .bind(&source.id)
            .bind(scope_key)
            .bind(timestamp(now)?)
            .bind(json(&source)?)
            .execute(&mut **tx)
            .await
        } else {
            sqlx::query(
                "UPDATE engine_summaries SET subject_memory_summarized=1,updated_at=$4,data=$5 \
                 WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
                 AND subject_memory_summarized<>1",
            )
            .bind(&source.tenant_id)
            .bind(&source.source_id)
            .bind(&source.id)
            .bind(timestamp(now)?)
            .bind(json(&source)?)
            .execute(&mut **tx)
            .await
        }
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("subject memory summary source changed during final commit".to_string());
        }
    }

    Ok(())
}

async fn finalize_subject_memory_sources(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    memory_ids: &[String],
    now: &str,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND id=ANY($4) AND status='active' AND recall_eligible \
         AND rollup_status='pending' FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.subject_id)
    .bind(memory_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != memory_ids.len() {
        return Err("subject memory rollup did not own every frozen source memory".to_string());
    }
    for row in rows {
        let mut source: EngineSubjectMemory = decode(row)?;
        source.rollup_status = "done".to_string();
        source.rollup_memory_key = Some(output.memory_key.clone());
        source.rolled_up_at = Some(now.to_string());
        source.updated_at = now.to_string();
        let updated = sqlx::query(
            "UPDATE engine_subject_memories SET rollup_status='done',updated_at=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='active' \
             AND recall_eligible AND rollup_status='pending'",
        )
        .bind(&source.tenant_id)
        .bind(&source.source_id)
        .bind(&source.id)
        .bind(timestamp(now)?)
        .bind(json(&source)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("subject memory rollup source changed during final commit".to_string());
        }
    }
    Ok(())
}

async fn finish_subject_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    subject_id: &str,
    result: FinishEngineJobRunRequest,
    now: &str,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs WHERE id=$1 AND tenant_id=$2 AND source_id=$3 \
         AND subject_id=$4 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(subject_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "subject memory generation job disappeared during final commit".to_string())?;
    let (stored_status, data) = row;
    let mut job: EngineJobRun = decode(data)?;
    if stored_status != "running" || job.status != "running" {
        return Err("subject memory generation job is not running".to_string());
    }
    job.status = result.status;
    job.input_count = result.input_count;
    job.output_count = result.output_count;
    job.processed_count = result.processed_count;
    job.success_count = result.success_count;
    job.error_count = result.error_count;
    job.metadata = result.metadata;
    job.error_message = result.error_message;
    job.finished_at = Some(now.to_string());
    let updated = sqlx::query(
        "UPDATE engine_job_runs SET status=$2,finished_at=$3,data=$4 WHERE id=$1 AND status='running'",
    )
    .bind(&job.id)
    .bind(&job.status)
    .bind(timestamp(now)?)
    .bind(json(&job)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("subject memory generation job changed during final commit".to_string());
    }
    Ok(())
}
