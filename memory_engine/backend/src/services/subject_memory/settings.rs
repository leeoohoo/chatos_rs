// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::RunSubjectMemoryJobRequest;
use crate::repositories::control_plane as cp_repo;

use super::{
    SubjectMemoryJobSettings, DEFAULT_MAX_LEVEL, DEFAULT_TARGET_SUMMARY_TOKENS, DEFAULT_TOKEN_LIMIT,
};

fn build_settings(req: &RunSubjectMemoryJobRequest) -> Result<SubjectMemoryJobSettings, String> {
    let subject_id = req.subject_id.trim();
    if subject_id.is_empty() {
        return Err("empty subject_id".to_string());
    }
    let memory_type = req.memory_type.trim();
    if memory_type.is_empty() {
        return Err("empty memory_type".to_string());
    }
    let thread_label = req.source_thread_label.trim();
    if thread_label.is_empty() {
        return Err("empty source_thread_label".to_string());
    }

    Ok(SubjectMemoryJobSettings {
        relation_subject_id: req
            .relation_subject_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| subject_id.to_string()),
        source_summary_type: req
            .source_summary_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "thread_incremental".to_string()),
        summary_prompt: req.summary_prompt.clone(),
        rollup_summary_prompt: req.rollup_summary_prompt.clone(),
        prompt_title: req
            .prompt_title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("Subject memory {}", req.subject_id)),
        token_limit: req.token_limit.unwrap_or(DEFAULT_TOKEN_LIMIT).max(500),
        target_summary_tokens: req
            .target_summary_tokens
            .unwrap_or(DEFAULT_TARGET_SUMMARY_TOKENS)
            .max(128),
        count_limit: req.count_limit.unwrap_or(0).max(0),
        keep_level0_count: req.keep_level0_count.unwrap_or(0).max(0),
        max_level: req.max_level.unwrap_or(DEFAULT_MAX_LEVEL).max(1),
        memory_metadata: req.memory_metadata.clone(),
    })
}

pub(crate) async fn build_settings_with_policy(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
) -> Result<SubjectMemoryJobSettings, String> {
    let mut settings = build_settings(req)?;
    let policy = cp_repo::get_effective_job_policy(db, "subject_memory").await?;

    if settings.summary_prompt.is_none() {
        settings.summary_prompt = policy.summary_prompt.clone();
    }
    if settings.rollup_summary_prompt.is_none() {
        settings.rollup_summary_prompt = policy.rollup_summary_prompt.clone();
    }
    if req.token_limit.is_none() {
        settings.token_limit = policy.token_limit.unwrap_or(settings.token_limit).max(500);
    }
    if req.target_summary_tokens.is_none() {
        settings.target_summary_tokens = policy
            .target_summary_tokens
            .unwrap_or(settings.target_summary_tokens)
            .max(128);
    }
    if req.keep_level0_count.is_none() {
        settings.keep_level0_count = policy
            .keep_level0_count
            .unwrap_or(settings.keep_level0_count)
            .max(0);
    }
    if req.count_limit.is_none() {
        settings.count_limit = policy.count_limit.unwrap_or(settings.count_limit).max(0);
    }
    if req.max_level.is_none() {
        settings.max_level = policy.max_level.unwrap_or(settings.max_level).max(1);
    }

    Ok(settings)
}
