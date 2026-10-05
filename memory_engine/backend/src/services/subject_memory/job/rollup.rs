// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::RunSubjectMemoryJobRequest;
use crate::repositories::subject_memories;
use crate::services::ai_pipeline::{estimate_tokens_text, SummaryBuildResult};

use super::super::builders::build_subject_memory_rollup;
use super::super::render::{
    decorate_generated_text, digest_from_ids, subject_memory_to_rollup_block,
};
use super::super::{RollupSelection, SubjectMemoryJobSettings};
use super::common::{
    finish_subject_memory_build, generated_subject_memory_request,
    rollback_generated_subject_memory_after_mark_failure, SubjectMemoryJobProgress,
};

pub(crate) async fn process_rollup_selection(
    config: &AppConfig,
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    selection: &RollupSelection,
    from_scope_runner: bool,
    input_count: usize,
    job_run_id: &str,
    scope_lock_owner: Option<&str>,
    progress: &mut SubjectMemoryJobProgress,
) -> Result<(), String> {
    progress.add_processed(selection.selected.len());
    let level = selection.level;
    let target_level = level + 1;
    let selected_ids = selection
        .selected
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let source_digest = digest_from_ids(
        format!("{}:rollup:l{}->{}", req.memory_type, level, target_level).as_str(),
        selected_ids.as_slice(),
    )
    .ok_or_else(|| "build subject memory rollup digest failed".to_string())?;

    if let Some(existing) = subject_memories::find_subject_memory_by_source_digest(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.subject_id.as_str(),
        settings.relation_subject_id.as_str(),
        req.memory_type.as_str(),
        target_level,
        source_digest.as_str(),
    )
    .await?
    {
        progress.marked_source_memories += subject_memories::mark_subject_memories_rolled_up(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            req.subject_id.as_str(),
            selected_ids.as_slice(),
            existing.memory_key.as_str(),
        )
        .await?;
        tracing::info!(
            "[MEMORY-ENGINE-SUBJECT] reused rollup subject_id={} memory_type={} level={}->{} digest={} memory_key={}",
            req.subject_id, req.memory_type, level, target_level, source_digest, existing.memory_key
        );
        return Ok(());
    }

    let mut summarizable = Vec::new();
    let mut oversized = 0usize;
    for memory in &selection.selected {
        let block = subject_memory_to_rollup_block(memory);
        if estimate_tokens_text(block.as_str()) > settings.token_limit.max(500) {
            oversized += 1;
        } else {
            summarizable.push(block);
        }
    }

    let build = if summarizable.is_empty() {
        SummaryBuildResult {
            text: format!(
                "All {} selected {} memories at level {} exceeded token_limit={}, so this rollup only marks the batch as rolled up.",
                selection.selected.len(),
                req.memory_type,
                level,
                settings.token_limit.max(500)
            ),
            chunk_count: 1,
            overflow_retry_count: 0,
        }
    } else {
        finish_subject_memory_build(
            db,
            req,
            settings,
            from_scope_runner,
            input_count,
            progress,
            selection.selected.len(),
            job_run_id,
            build_subject_memory_rollup(
                config,
                db,
                req.tenant_id.as_str(),
                settings.prompt_title.as_str(),
                summarizable.as_slice(),
                settings.token_limit,
                settings.target_summary_tokens,
                level,
                target_level,
                job_run_id,
                serde_json::json!({
                    "resume_kind": "subject_memory_job",
                    "job_run_id": job_run_id,
                    "request": req,
                    "from_scope_runner": from_scope_runner,
                    "scope_lock_owner": scope_lock_owner,
                    "scope_key": req.scope_key,
                }),
            )
            .await,
        )
        .await?
    };

    let memory_text = decorate_generated_text(
        build,
        Some(oversized),
        "subject memory rollup",
        settings.keep_level0_count,
    );
    let memory_key = format!("{}:l{}:{}", req.memory_type, target_level, source_digest);
    let memory_req = generated_subject_memory_request(
        req,
        settings,
        memory_text,
        target_level,
        source_digest.as_str(),
    );
    subject_memories::upsert_generated_subject_memory(
        db,
        req.subject_id.as_str(),
        memory_key.as_str(),
        memory_req,
        Some(source_digest.clone()),
        "pending",
    )
    .await?;
    progress.generated_rollups += 1;
    progress.marked_source_memories += match subject_memories::mark_subject_memories_rolled_up(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.subject_id.as_str(),
        selected_ids.as_slice(),
        memory_key.as_str(),
    )
    .await
    {
        Ok(marked) => marked,
        Err(err) => {
            rollback_generated_subject_memory_after_mark_failure(
                db,
                req,
                settings,
                from_scope_runner,
                input_count,
                progress,
                selection.selected.len(),
                job_run_id,
                memory_key.as_str(),
                source_digest.as_str(),
                target_level,
                format!("mark subject memories rolled up failed: {}", err),
            )
            .await;
            return Err(err);
        }
    };

    Ok(())
}
