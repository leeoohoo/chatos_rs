#[cfg(test)]
pub async fn commit_thread_summary_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, None, None).await
}

pub async fn commit_thread_summary_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadL0GenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, Some(finalization), None, None)
        .await
}

pub async fn commit_thread_summary_resummarization_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadL0ResummarizationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, None, Some(finalization))
        .await
}

pub async fn commit_thread_rollup_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadRollupGenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, Some(finalization), None)
        .await
}

async fn commit_thread_summary_generation_inner(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    l0_finalization: Option<ThreadL0GenerationFinalization<'_>>,
    rollup_finalization: Option<ThreadRollupGenerationFinalization<'_>>,
    resummarization_finalization: Option<ThreadL0ResummarizationFinalization<'_>>,
) -> Result<EngineSummary, String> {
    let current = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "thread summary generation registry row is missing".to_string())?;
    validate_summary_output(&current, &output)?;
    if current.status == "committed" {
        return load_committed_thread_summary(db, &current).await;
    }
    if current.status != "running" || current.lease_token != generation.lease_token {
        return Err("thread summary generation commit lease is no longer current".to_string());
    }
    if let Some(finalization) = l0_finalization.as_ref() {
        validate_l0_finalization(&current, &output, finalization)?;
    }
    if let Some(finalization) = rollup_finalization.as_ref() {
        validate_rollup_finalization(&current, &output, finalization)?;
    }
    if let Some(finalization) = resummarization_finalization.as_ref() {
        validate_resummarization_finalization(&current, &output, finalization)?;
    }

    let project_partition_key = output.project_partition_key;
    let is_noop = output.summary_type == "thread_noop";
    let mut summary = new_summary(
        &current.tenant_id,
        &current.source_id,
        output.thread_id,
        output.subject_id,
        output.summary_type,
        output.level,
        output.source_digest,
        output.summary_text,
        output.source_record_start_id,
        output.source_record_end_id,
        output.source_record_count,
        output.metadata,
    );
    summary.payload_schema = Some(output.payload_schema.to_string());
    summary.structured_payload = Some(output.structured_payload.clone());
    if is_noop {
        summary.rollup_status = "done".to_string();
        summary.subject_memory_summarized = 1;
        summary.subject_memory_summarized_at = Some(summary.created_at.clone());
    }
    let downstream_pending = !is_noop;
    let downstream_version = i64::from(downstream_pending);
    let downstream_requested_at = downstream_pending
        .then(|| timestamp(&summary.created_at))
        .transpose()?;
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    if l0_finalization.is_some()
        || rollup_finalization.is_some()
        || resummarization_finalization.is_some()
    {
        lock_running_generation_job(&mut tx, &current, Some(output.thread_id), None).await?;
    }
    if let Some(finalization) = resummarization_finalization.as_ref() {
        supersede_resummarized_range(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type, \
         level,source_digest,status,rollup_status,subject_memory_summarized,rollup_dispatch_pending, \
         rollup_dispatch_version,rollup_dispatch_requested_at,subject_memory_source_dispatch_pending, \
         subject_memory_source_dispatch_version,subject_memory_source_dispatch_requested_at, \
         created_at,updated_at,data,generation_id,generation_reason,generation_profile, \
         project_partition_key,recall_eligible,payload_schema,structured_payload,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$12,$13,$14,$15,$16,$17, \
         $18,$19,$20,$21,true,$22,$23,$24)",
    )
    .bind(&summary.id)
    .bind(&summary.tenant_id)
    .bind(&summary.source_id)
    .bind(&summary.thread_id)
    .bind(&summary.subject_id)
    .bind(&summary.summary_type)
    .bind(summary.level)
    .bind(&summary.source_digest)
    .bind(&summary.status)
    .bind(&summary.rollup_status)
    .bind(summary.subject_memory_summarized)
    .bind(downstream_pending)
    .bind(downstream_version)
    .bind(downstream_requested_at)
    .bind(timestamp(&summary.created_at)?)
    .bind(timestamp(&summary.updated_at)?)
    .bind(json(&summary)?)
    .bind(&current.generation_id)
    .bind(output.generation_reason)
    .bind(output.generation_profile)
    .bind(output.project_partition_key)
    .bind(output.payload_schema)
    .bind(Json(output.structured_payload))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if !is_noop {
        memory_facts::project_payload(
            &mut tx,
            &current,
            FactOutputScope {
                output_kind: "thread_summary",
                output_id: summary.id.as_str(),
                scope_kind: "thread",
                scope_key: summary.thread_id.as_str(),
                subject_id: summary.subject_id.as_str(),
                project_partition_key: Some(project_partition_key),
            },
            output.payload_schema,
            output.structured_payload,
        )
        .await?;
    }
    if !is_noop {
        derived_events::enqueue_summary_downstream_requests(
            &mut tx,
            &current.tenant_id,
            &current.source_id,
            &summary.id,
            &summary.thread_id,
            project_partition_key,
            &current.generation_id,
        )
        .await?;
    }
    if let Some(finalization) = l0_finalization {
        finalize_thread_l0_generation(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if let Some(finalization) = rollup_finalization {
        finalize_thread_rollup_generation(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if let Some(finalization) = resummarization_finalization {
        finalize_thread_l0_resummarization(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if !generation_commits::commit_generation(
        &mut tx,
        generation,
        "thread_summary",
        summary.id.as_str(),
    )
    .await?
    {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return load_committed_thread_summary(db, &current).await;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(summary)
}
