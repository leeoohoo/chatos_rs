pub(crate) async fn execute_prepared_thread_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    ctx: ThreadSummaryExecutionContext,
) -> Result<RunThreadSummaryResponse, String> {
    let ThreadSummaryExecutionContext {
        thread,
        mut settings,
        pending_before_count,
        selection,
    } = ctx;
    settings.cloud_owner_entity_id = Some(job_run_id.to_string());
    let project_partition_key = project_partition_key(&selection).to_string();

    if selection.selected.is_empty() {
        finish_thread_summary_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: Some(noop_metadata(pending_before_count, pending_before_count, 0)),
                error_message: None,
            },
        )
        .await;
        let _ = thread_partitions::release_summary_claim(
            db,
            tenant_id,
            source_id,
            thread_id,
            project_partition_key.as_str(),
            job_run_id,
            0,
            0,
        )
        .await;
        return Ok(noop_response(thread_id));
    }

    let generation_descriptor = thread_summary_generation_descriptor(
        selection.selected.as_slice(),
        settings.token_limit,
        project_partition_key.as_str(),
    )?;
    let summary_build = match build_summary_text(
        config,
        db,
        tenant_id,
        thread.title.as_deref(),
        thread.subject_id.as_str(),
        selection.selected.as_slice(),
        &settings,
        generation_descriptor,
    )
    .await
    {
        Ok(build) => build,
        Err(error) if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED => {
            return Err(error);
        }
        Err(error) => {
            fail_prepared_summary_job(
                db,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
                project_partition_key.as_str(),
                pending_before_count,
                &selection,
                error.clone(),
            )
            .await;
            return Err(error);
        }
    };

    let generation = claim_thread_summary_generation(
        db,
        tenant_id,
        source_id,
        job_run_id,
    )
    .await?;
    let record_ids = selection
        .selected
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let structured_payload = summary_build.structured_payload;
    let summary_text = crate::models::structured_payload::render_payload(&structured_payload)?;
    let pending_after_count = pending_before_count.saturating_sub(record_ids.len() as i64);
    let job_result = FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: record_ids.len() as i64,
        output_count: 1,
        processed_count: record_ids.len() as i64,
        success_count: record_ids.len() as i64,
        error_count: 0,
        metadata: Some(done_metadata(
            pending_before_count,
            record_ids.len(),
            selection.selected_token_count,
            record_ids.len(),
            pending_after_count,
            0,
            "pending_generation_output",
        )),
        error_message: None,
    };
    let summary = generation_output_commits::commit_thread_summary_generation_final(
        db,
        &generation,
        generation_output_commits::ThreadSummaryGenerationOutput {
            thread_id,
            subject_id: thread.subject_id.as_str(),
            summary_type: "thread_incremental",
            level: 0,
            source_digest: None,
            summary_text: summary_text.as_str(),
            source_record_start_id: record_ids.first().cloned(),
            source_record_end_id: record_ids.last().cloned(),
            source_record_count: record_ids.len(),
            metadata: Some(serde_json::json!({
                "project_subject_id": selection.project_subject_id,
                "generation_profile": THREAD_SUMMARY_GENERATION_PROFILE,
            })),
            generation_reason: "pending_records",
            generation_profile: THREAD_SUMMARY_GENERATION_PROFILE,
            project_partition_key: project_partition_key.as_str(),
            payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
            structured_payload: &structured_payload,
            pipeline_revision: THREAD_SUMMARY_PIPELINE_REVISION,
        },
        generation_output_commits::ThreadL0GenerationFinalization {
            record_ids: record_ids.as_slice(),
            consumed_summary_tokens: selection.selected_token_count,
            job_result,
        },
    )
    .await?;

    if let Err(error) = crate::rollup_queue::publish_pending_rollup_for_summary(
        config,
        db,
        tenant_id,
        source_id,
        summary.id.as_str(),
    )
    .await
    {
        tracing::warn!(summary_id = summary.id, error, "thread summary rollup Outbox publish deferred");
    }
    if let Err(error) = crate::subject_memory_queue::publish_pending_source_for_summary(
        config,
        db,
        tenant_id,
        source_id,
        summary.id.as_str(),
    )
    .await
    {
        tracing::warn!(summary_id = summary.id, error, "thread summary subject-memory Outbox publish deferred");
    }

    Ok(RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: true,
        summary_id: Some(summary.id),
        source_record_count: record_ids.len(),
    })
}

fn thread_summary_generation_descriptor(
    records: &[crate::models::EngineRecord],
    token_limit: i64,
    project_partition_key: &str,
) -> Result<serde_json::Value, String> {
    let inputs = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id,
                "thread_id": record.thread_id,
                "role": record.role,
                "record_type": record.record_type,
                "content": record.content,
                "structured_payload": record.structured_payload,
                "metadata": record.metadata,
                "created_at": record.created_at,
            })
        })
        .collect::<Vec<_>>();
    let (manifest, digest) = crate::services::generation_identity::input_manifest(inputs)?;
    Ok(crate::services::generation_identity::descriptor(
        THREAD_SUMMARY_GENERATION_STEP,
        "thread_summary",
        THREAD_SUMMARY_GENERATION_PROFILE,
        THREAD_SUMMARY_PIPELINE_REVISION,
        Some(project_partition_key),
        manifest,
        digest,
        serde_json::json!({"max_model_input_tokens": token_limit}),
    ))
}

async fn claim_thread_summary_generation(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    job_run_id: &str,
) -> Result<generation_commits::GenerationCommit, String> {
    let generation = generation_commits::get_generation_by_job_step(
        db,
        tenant_id,
        source_id,
        job_run_id,
        THREAD_SUMMARY_GENERATION_STEP,
    )
    .await?
    .ok_or_else(|| "thread summary generation was not frozen before model execution".to_string())?;
    if generation.status == "committed" {
        return Ok(generation);
    }
    generation_commits::claim_generation(
        db,
        tenant_id,
        source_id,
        generation.generation_id.as_str(),
        format!("thread-summary-finalizer:{job_run_id}").as_str(),
        GENERATION_FINALIZE_LEASE_SECONDS,
    )
    .await?
    .ok_or_else(|| "thread summary generation finalization lease is occupied".to_string())
}

#[allow(clippy::too_many_arguments)]
async fn fail_prepared_summary_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    project_partition_key: &str,
    pending_before_count: i64,
    selection: &PendingRecordSelection,
    error: String,
) {
    let _ = records::release_records_from_summary(db, tenant_id, source_id, thread_id, job_run_id)
        .await;
    let _ = thread_partitions::release_summary_claim(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        job_run_id,
        0,
        0,
    )
    .await;
    finish_thread_summary_job_run(
        db,
        job_run_id,
        FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: selection.selected.len() as i64,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: selection.selected.len() as i64,
            metadata: Some(failed_metadata(
                pending_before_count,
                Some(selection.selected.len()),
                Some(selection.selected_token_count),
                0,
                Some(pending_before_count),
                0,
                0,
            )),
            error_message: Some(error),
        },
    )
    .await;
}
