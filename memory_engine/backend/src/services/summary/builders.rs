use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineRecord, DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE};
use crate::repositories::control_plane as cp_repo;
use crate::services::ai_pipeline::{
    self, ContinueCheck, SummarizeTextsOptions, SummaryBuildResult, MIN_TOKEN_LIMIT,
};
use crate::services::control_plane as control_plane_service;

use super::render::record_to_summary_block;
use super::{RollupSettings, SummaryJobSettings};

pub(crate) async fn build_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    title: Option<&str>,
    records: &[EngineRecord],
    settings: &SummaryJobSettings,
) -> Result<SummaryBuildResult, String> {
    let ai =
        control_plane_service::build_ai_client_for_job(config, db, "summary", owner_user_id)
            .await?;
    if !ai.is_enabled() {
        return Err("summary model is not configured or enabled".to_string());
    }

    let items = records
        .iter()
        .map(record_to_summary_block)
        .collect::<Vec<_>>();
    ai_pipeline::summarize_texts_with_split(
        &ai,
        items.as_slice(),
        &SummarizeTextsOptions {
            prompt_title: title.unwrap_or("Thread summary"),
            summary_prompt: settings.summary_prompt.as_deref(),
            leaf_directive: "Summarize these conversation records into a concise, high-signal continuation summary. Preserve what has already been done, what is in progress, the most likely next steps, and concrete constraints, files, commands, risks, and user requirements.",
            merge_directive: "Merge these partial conversation summaries into one coherent continuation summary. Preserve chronology, current state, next actions, and user-grounded constraints.",
            token_limit: settings.token_limit,
            target_tokens: settings.target_summary_tokens,
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "summary",
            continue_check: None,
        },
    )
    .await
}

pub(crate) async fn build_repair_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    title: Option<&str>,
    records: &[EngineRecord],
    settings: &SummaryJobSettings,
    job_run_id: Option<&str>,
) -> Result<SummaryBuildResult, String> {
    let ai = control_plane_service::build_ai_client_for_job(
        config,
        db,
        "thread_repair",
        owner_user_id,
    )
    .await?;
    if !ai.is_enabled() {
        return Err("thread repair model is not configured or enabled".to_string());
    }

    let db_for_check = db.clone();
    let repair_job_run_id = job_run_id.map(str::to_string);
    let continue_check = move || {
        let db = db_for_check.clone();
        let job_run_id = repair_job_run_id.clone();
        Box::pin(async move {
            let Some(job_run_id) = job_run_id else {
                return Ok(());
            };
            let job = cp_repo::get_job_run_by_id(&db, job_run_id.as_str()).await?;
            match job {
                Some(job) if job.status == "running" => Ok(()),
                Some(job) => Err(format!(
                    "thread repair job_run_id={} stopped before model request because status={}",
                    job.id, job.status
                )),
                None => Err(format!(
                    "thread repair job_run_id={} stopped before model request because job run is missing",
                    job_run_id
                )),
            }
        })
            as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
    };
    let continue_check_ref = if job_run_id.is_some() {
        Some(&continue_check as &ContinueCheck<'_>)
    } else {
        None
    };

    let items = records
        .iter()
        .map(record_to_summary_block)
        .collect::<Vec<_>>();
    ai_pipeline::summarize_texts_with_split(
        &ai,
        items.as_slice(),
        &SummarizeTextsOptions {
            prompt_title: title.unwrap_or("Thread repair summary"),
            summary_prompt: settings
                .summary_prompt
                .as_deref()
                .or(Some(DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE)),
            leaf_directive: "Generate a repair-oriented summary from these conversation records. Use the user's messages as the primary factual source, correct assistant drift, mark unsupported claims as unverified, and state the next-turn constraints clearly.",
            merge_directive: "Merge these partial repair summaries into one corrected context summary. Preserve only user-grounded facts, explicitly call out incorrect or unverified claims, and keep the next-turn constraints actionable.",
            token_limit: settings.token_limit.max(MIN_TOKEN_LIMIT),
            target_tokens: None,
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "thread_repair",
            continue_check: continue_check_ref,
        },
    )
    .await
}

pub(crate) async fn build_rollup_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    title: Option<&str>,
    items: &[String],
    settings: &RollupSettings,
    level: i64,
    target_level: i64,
) -> Result<SummaryBuildResult, String> {
    let ai =
        control_plane_service::build_ai_client_for_job(config, db, "rollup", owner_user_id)
            .await?;
    if !ai.is_enabled() {
        return Err("rollup model is not configured or enabled".to_string());
    }
    let prompt_title = title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Thread rollup level {} -> {}", level, target_level));
    let leaf_directive = format!(
        "Roll up these prior thread summaries from level {} to level {}. Preserve durable facts, current goals, active work, constraints, and risks.",
        level, target_level
    );
    let merge_directive = format!(
        "Merge these partial rollup summaries for level {} to level {} into one coherent higher-level summary. Preserve chronology, durable facts, current state, next actions, and constraints.",
        level, target_level
    );
    ai_pipeline::summarize_texts_with_split(
        &ai,
        items,
        &SummarizeTextsOptions {
            prompt_title: prompt_title.as_str(),
            summary_prompt: settings.summary_prompt.as_deref(),
            leaf_directive: leaf_directive.as_str(),
            merge_directive: merge_directive.as_str(),
            token_limit: settings.token_limit,
            target_tokens: Some(settings.target_summary_tokens.max(256)),
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "rollup",
            continue_check: None,
        },
    )
    .await
}
