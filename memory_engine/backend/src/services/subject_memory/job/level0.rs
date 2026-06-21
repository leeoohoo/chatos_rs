use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{now_rfc3339, RunSubjectMemoryJobRequest, UpsertSubjectMemoryRequest};
use crate::repositories::subject_memories;

use super::super::builders::build_subject_memory_from_summaries;
use super::super::render::{
    build_memory_metadata, decorate_generated_text, digest_from_ids,
    summary_to_subject_memory_block,
};
use super::super::selectors::mark_summary_sources_subject_memory_summarized;
use super::super::{PendingSourceSummary, SubjectMemoryJobSettings};
use super::common::{
    build_failed_job_run, finish_subject_memory_job_run, SubjectMemoryJobProgress,
};

pub(crate) async fn process_level0_selection(
    config: &AppConfig,
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    selected: &[PendingSourceSummary],
    from_scope_runner: bool,
    input_count: usize,
    job_run_id: &str,
    progress: &mut SubjectMemoryJobProgress,
) -> Result<(), String> {
    progress.add_processed(selected.len());
    let selected_ids = selected
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let source_digest = digest_from_ids(
        format!("{}:l0", req.memory_type).as_str(),
        selected_ids.as_slice(),
    )
    .ok_or_else(|| "build subject memory l0 digest failed".to_string())?;

    if let Some(existing) = subject_memories::find_subject_memory_by_source_digest(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.subject_id.as_str(),
        settings.relation_subject_id.as_str(),
        req.memory_type.as_str(),
        0,
        source_digest.as_str(),
    )
    .await?
    {
        progress.marked_source_summaries +=
            mark_summary_sources_subject_memory_summarized(db, selected).await?;
        tracing::info!(
            "[MEMORY-ENGINE-SUBJECT] reused level0 subject_id={} memory_type={} digest={} memory_key={}",
            req.subject_id, req.memory_type, source_digest, existing.memory_key
        );
        return Ok(());
    }

    let selected_texts = selected
        .iter()
        .map(summary_to_subject_memory_block)
        .collect::<Vec<_>>();
    let build = match build_subject_memory_from_summaries(
        config,
        db,
        Some(req.tenant_id.as_str()),
        settings.prompt_title.as_str(),
        settings.summary_prompt.as_deref(),
        selected_texts.as_slice(),
        settings.token_limit,
        settings.target_summary_tokens,
    )
    .await
    {
        Ok(build) => build,
        Err(err) => {
            finish_subject_memory_job_run(
                db,
                job_run_id,
                build_failed_job_run(
                    req,
                    settings.relation_subject_id.as_str(),
                    from_scope_runner,
                    input_count,
                    progress,
                    selected.len(),
                    err.clone(),
                ),
            )
            .await;
            return Err(err);
        }
    };
    let recall_text = decorate_generated_text(
        build,
        None,
        "level0 subject memory",
        settings.keep_level0_count,
    );
    let memory_key = format!("{}:l0:{}", req.memory_type, source_digest);
    let memory_req = UpsertSubjectMemoryRequest {
        id: None,
        tenant_id: req.tenant_id.clone(),
        source_id: req.source_id.clone(),
        memory_type: req.memory_type.clone(),
        text: recall_text,
        level: Some(0),
        source_digest: Some(source_digest.clone()),
        confidence: None,
        last_seen_at: Some(now_rfc3339()),
        metadata: build_memory_metadata(
            settings.memory_metadata.clone(),
            settings.relation_subject_id.as_str(),
            req.source_thread_label.as_str(),
        ),
        rollup_status: Some("pending".to_string()),
        rollup_memory_key: None,
        rolled_up_at: None,
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
    };
    subject_memories::upsert_generated_subject_memory(
        db,
        req.subject_id.as_str(),
        memory_key.as_str(),
        memory_req,
        Some(source_digest.clone()),
        "pending",
    )
    .await?;
    progress.generated_level0 += 1;
    progress.marked_source_summaries +=
        match mark_summary_sources_subject_memory_summarized(db, selected).await {
            Ok(marked) => marked,
            Err(err) => {
                let delete_req = UpsertSubjectMemoryRequest {
                    id: None,
                    tenant_id: req.tenant_id.clone(),
                    source_id: req.source_id.clone(),
                    memory_type: req.memory_type.clone(),
                    text: String::new(),
                    level: Some(0),
                    source_digest: Some(source_digest.clone()),
                    confidence: None,
                    last_seen_at: None,
                    metadata: None,
                    rollup_status: Some("pending".to_string()),
                    rollup_memory_key: None,
                    rolled_up_at: None,
                    status: Some("deleted".to_string()),
                    created_at: None,
                    updated_at: None,
                };
                let _ = subject_memories::upsert_generated_subject_memory(
                    db,
                    req.subject_id.as_str(),
                    memory_key.as_str(),
                    delete_req,
                    Some(source_digest.clone()),
                    "pending",
                )
                .await;
                finish_subject_memory_job_run(
                    db,
                    job_run_id,
                    build_failed_job_run(
                        req,
                        settings.relation_subject_id.as_str(),
                        from_scope_runner,
                        input_count,
                        progress,
                        selected.len(),
                        format!(
                            "mark source summaries subject-memory summarized failed: {}",
                            err
                        ),
                    ),
                )
                .await;
                return Err(err);
            }
        };

    Ok(())
}
