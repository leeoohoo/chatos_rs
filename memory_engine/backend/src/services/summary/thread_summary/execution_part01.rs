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

    let mut processed_count = 0_i64;
    let output_count = 0_i64;

    let result: Result<RunThreadSummaryResponse, String> = async {
        let selected_pending_tokens = selection.selected_token_count.max(0);
        let skipped_pending_tokens = selection.oversized_token_count.max(0);
        let skipped_count = mark_oversized_records_as_summarized(
            db,
            tenant_id,
            source_id,
            thread_id,
            selection.oversized.as_slice(),
            job_run_id,
            "skipped_single_record_token_limit",
        )
        .await?;
        let skipped_count_i64 = skipped_count as i64;

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
                    metadata: Some(noop_metadata(
                        pending_before_count,
                        pending_before_count.saturating_sub(skipped_count_i64),
                        skipped_count,
                    )),
                    error_message: None,
                },
            )
            .await;
            let _ = threads::release_summary_slot(
                db,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
                skipped_count_i64,
                skipped_pending_tokens,
            )
            .await;
            return Ok(noop_response(thread_id));
        }

        let summary_build = match build_summary_text(
            config,
            db,
            tenant_id,
            thread.title.as_deref(),
            selection.selected.as_slice(),
            &settings,
        )
        .await
        {
            Ok(build) => build,
            Err(err) if err == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED => {
                return Err(err);
            }
            Err(err) => {
                finish_thread_summary_job_run(
                    db,
                    job_run_id,
                    FinishEngineJobRunRequest {
                        status: "failed".to_string(),
                        input_count: selection.selected.len() as i64,
                        output_count: 0,
                        processed_count: selection.selected.len() as i64 + skipped_count as i64,
                        success_count: 0,
                        error_count: selection.selected.len() as i64,
                        metadata: Some(failed_metadata(
                            pending_before_count,
                            Some(selection.selected.len()),
                            Some(selection.selected_token_count),
                            skipped_count,
                            Some(pending_before_count.saturating_sub(skipped_count_i64)),
                            selection.selected.len() as i64 + skipped_count as i64,
                            0,
                        )),
                        error_message: Some(err.clone()),
                    },
                )
                .await;
                let _ = threads::release_summary_slot(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    skipped_count_i64,
                    skipped_pending_tokens,
                )
                .await;
                let _ = records::release_records_from_summary(
                    db, tenant_id, source_id, thread_id, job_run_id,
                )
                .await;
                return Err(err);
            }
        };
        let summary_text =
            decorate_generated_text(summary_build, Some(skipped_count), "message summary");
        let summary = summaries::create_thread_summary(
            db,
            tenant_id,
            source_id,
            thread_id,
            thread.subject_id.as_str(),
            summary_text.as_str(),
            selection.selected.first().map(|item| item.id.clone()),
            selection.selected.last().map(|item| item.id.clone()),
            selection.selected.len(),
        )
        .await?;
        processed_count = selection.selected.len() as i64 + skipped_count as i64;

        let record_ids = selection
            .selected
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let marked_messages = match records::mark_claimed_records_summarized(
            db,
            tenant_id,
            source_id,
            thread_id,
            record_ids.as_slice(),
            job_run_id,
            summary.id.as_str(),
        )
        .await
        {
            Ok(marked) => marked,
            Err(err) => {
                let _ = summaries::delete_thread_summary(
                    db,
                    thread_id,
                    summary.id.as_str(),
                    Some(tenant_id),
                    Some(source_id),
                )
                .await;
                finish_thread_summary_job_run(
                    db,
                    job_run_id,
                    FinishEngineJobRunRequest {
                        status: "failed".to_string(),
                        input_count: selection.selected.len() as i64,
                        output_count: 0,
                        processed_count: selection.selected.len() as i64 + skipped_count as i64,
                        success_count: 0,
                        error_count: selection.selected.len() as i64,
                        metadata: Some(failed_metadata(
                            pending_before_count,
                            Some(selection.selected.len()),
                            Some(selection.selected_token_count),
                            skipped_count,
                            Some(pending_before_count.saturating_sub(skipped_count_i64)),
                            selection.selected.len() as i64 + skipped_count as i64,
                            0,
                        )),
                        error_message: Some(format!("mark records summarized failed: {}", err)),
                    },
                )
                .await;
                let _ = threads::release_summary_slot(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    skipped_count_i64,
                    skipped_pending_tokens,
                )
                .await;
                let _ = records::release_records_from_summary(
                    db, tenant_id, source_id, thread_id, job_run_id,
                )
                .await;
                return Err(err);
            }
        };
        let pending_after_count = pending_before_count
            .saturating_sub(skipped_count_i64)
            .saturating_sub(marked_messages as i64);
        finish_thread_summary_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: selection.selected.len() as i64,
                output_count: 1,
                processed_count: selection.selected.len() as i64 + skipped_count as i64,
                success_count: selection.selected.len() as i64 + skipped_count as i64,
                error_count: 0,
                metadata: Some(done_metadata(
                    pending_before_count,
                    selection.selected.len(),
                    selection.selected_token_count,
                    marked_messages + skipped_count,
                    pending_after_count,
                    skipped_count,
                    summary.id.as_str(),
                )),
                error_message: None,
            },
        )
        .await;
        let _ = threads::release_summary_slot(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run_id,
            skipped_count_i64.saturating_add(marked_messages as i64),
            skipped_pending_tokens.saturating_add(selected_pending_tokens),
        )
        .await;
        if let Err(err) = crate::rollup_queue::publish_pending_rollup_for_summary(
            config,
            db,
            tenant_id,
            source_id,
            summary.id.as_str(),
        )
        .await
        {
            tracing::warn!(
                summary_id = summary.id.as_str(),
                error = err.as_str(),
                "Memory Engine left thread summary rollup event in Outbox for recovery"
            );
        }
        if let Err(err) = crate::subject_memory_queue::publish_pending_source_for_summary(
            config,
            db,
            tenant_id,
            source_id,
            summary.id.as_str(),
        )
        .await
        {
            tracing::warn!(
                summary_id = summary.id.as_str(),
                error = err.as_str(),
                "Memory Engine left thread summary subject-memory event in Outbox for recovery"
            );
        }

        Ok(RunThreadSummaryResponse {
            thread_id: thread_id.to_string(),
            generated: true,
            summary_id: Some(summary.id),
            source_record_count: selection.selected.len(),
        })
    }
    .await;

    if let Err(err) = &result {
        if err == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED {
            return result;
        }
        finish_thread_summary_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: pending_before_count.max(0),
                output_count,
                processed_count,
                success_count: output_count,
                error_count: 1,
                metadata: Some(failed_metadata(
                    pending_before_count,
                    None,
                    None,
                    0,
                    None,
                    processed_count,
                    output_count,
                )),
                error_message: Some(err.clone()),
            },
        )
        .await;
        let _ =
            threads::release_summary_slot(db, tenant_id, source_id, thread_id, job_run_id, 0, 0)
                .await;
        let _ =
            records::release_records_from_summary(db, tenant_id, source_id, thread_id, job_run_id)
                .await;
    }

    result
}

fn noop_response(thread_id: &str) -> RunThreadSummaryResponse {
    RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: false,
        summary_id: None,
        source_record_count: 0,
    }
}

fn validate_summary_job_scope(
    job_run: &EngineJobRun,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    if job_run.job_type != "summary"
        || job_run.tenant_id.as_deref() != Some(tenant_id)
        || job_run.source_id.as_deref() != Some(source_id)
        || job_run.thread_id.as_deref() != Some(thread_id)
    {
        return Err("summary job scope does not match its Cloud Agent callback".to_string());
    }
    Ok(())
}

fn completed_job_response(thread_id: &str, job_run: &EngineJobRun) -> RunThreadSummaryResponse {
    let summary_id = job_run
        .metadata
        .as_ref()
        .and_then(|value| value.get("generated_summary_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: summary_id.is_some(),
        summary_id,
        source_record_count: metadata_i64(job_run.metadata.as_ref(), "selected_count")
            .unwrap_or(0)
            .max(0) as usize,
    }
}

fn metadata_i64(metadata: Option<&serde_json::Value>, key: &str) -> Option<i64> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_i64())
}
