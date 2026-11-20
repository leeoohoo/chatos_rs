fn validate_rollup_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadRollupGenerationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_rollup" || output.level <= 0 {
        return Err("only a thread rollup generation can finalize source summaries".to_string());
    }
    if finalization.summary_ids.is_empty()
        || finalization.summary_ids.len() != output.source_record_count
    {
        return Err("thread rollup finalization source count does not match output".to_string());
    }
    let unique = finalization
        .summary_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.summary_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err("thread rollup source ids must be unique and non-empty".to_string());
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "thread rollup generation input manifest has no inputs".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "thread rollup generation input manifest has an invalid id".to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let requested_ids = finalization
        .summary_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    if frozen_ids != requested_ids {
        return Err("thread rollup sources do not match frozen input order".to_string());
    }
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "thread rollup finalization requires a successful terminal job result".to_string(),
        );
    }
    Ok(())
}

async fn finalize_thread_l0_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadL0GenerationFinalization<'_>,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND id=ANY($4) AND project_partition_key=$5 AND summary_status='summarizing' \
         AND summary_job_run_id=$6 FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&summary.thread_id)
    .bind(finalization.record_ids)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != finalization.record_ids.len() {
        return Err("thread L0 finalization did not own every frozen source record".to_string());
    }
    let records = rows
        .into_iter()
        .map(decode)
        .collect::<Result<Vec<EngineRecord>, _>>()?;
    let consumed_partition_tokens = records
        .iter()
        .map(crate::repositories::records::estimate_pending_record_tokens)
        .fold(0_i64, i64::saturating_add);
    let now = now_rfc3339();
    for mut record in records {
        record.summary_status = "summarized".to_string();
        record.summary_id = Some(summary.id.clone());
        record.summarized_at = Some(now.clone());
        let updated = sqlx::query(
            "UPDATE engine_records SET summary_status='summarized',summary_id=$4, \
             summary_job_run_id=NULL,summary_started_at=NULL,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_status='summarizing' \
             AND summary_job_run_id=$6",
        )
        .bind(&record.tenant_id)
        .bind(&record.source_id)
        .bind(&record.id)
        .bind(&summary.id)
        .bind(json(&record)?)
        .bind(&generation.job_run_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("thread L0 source record claim changed during commit".to_string());
        }
    }

    lineage::insert_thread_l0_coverage(
        tx,
        generation,
        summary,
        project_partition_key,
        finalization.record_ids,
    )
    .await?;

    let partition = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count-$6,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens-$7,0), \
         summary_status=CASE WHEN GREATEST(pending_record_count-$6,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND ((summary_status='running' AND summary_job_run_id=$5) OR \
           (summary_status<>'running' AND (summary_job_run_id IS NULL OR summary_job_run_id=$5))) \
         AND pending_record_count>=$6",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&summary.thread_id)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .bind(finalization.record_ids.len() as i64)
    .bind(consumed_partition_tokens.max(0))
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if partition.rows_affected() != 1 {
        return Err("thread L0 partition claim was lost before final commit".to_string());
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

async fn finalize_thread_rollup_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadRollupGenerationFinalization<'_>,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND id=ANY($4) AND project_partition_key=$5 AND status='done' AND recall_eligible \
         AND rollup_status='pending' FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.thread_id)
    .bind(finalization.summary_ids)
    .bind(project_partition_key)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != finalization.summary_ids.len() {
        return Err(
            "thread rollup finalization did not own every frozen source summary".to_string(),
        );
    }
    let now = now_rfc3339();
    for row in rows {
        let mut source: EngineSummary = decode(row)?;
        source.rollup_status = "done".to_string();
        source.rollup_summary_id = Some(output.id.clone());
        source.rolled_up_at = Some(now.clone());
        source.updated_at = now.clone();
        let updated = sqlx::query(
            "UPDATE engine_summaries SET rollup_status='done',rollup_dispatch_pending=false, \
             rollup_dispatch_consumed_version=GREATEST(rollup_dispatch_consumed_version, \
             rollup_dispatch_version),rollup_dispatch_consumed_at=$4,updated_at=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND rollup_status='pending'",
        )
        .bind(&source.tenant_id)
        .bind(&source.source_id)
        .bind(&source.id)
        .bind(timestamp(&now)?)
        .bind(json(&source)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("thread rollup source changed during final commit".to_string());
        }
    }

    lineage::insert_thread_rollup_edges(
        tx,
        generation,
        output,
        project_partition_key,
        finalization.summary_ids,
    )
    .await?;

    let released = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_status='pending', \
         rollup_job_run_id=NULL,rollup_locked_at=NULL,rollup_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND ((rollup_status='running' AND rollup_job_run_id=$5) OR \
           (rollup_status<>'running' AND (rollup_job_run_id IS NULL OR rollup_job_run_id=$5)))",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.thread_id)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if released.rows_affected() != 1 {
        return Err("thread rollup partition claim was lost before final commit".to_string());
    }
    finish_generation_job(
        tx,
        generation,
        &output.thread_id,
        &output.id,
        finalization.job_result,
        &now,
    )
    .await
}
