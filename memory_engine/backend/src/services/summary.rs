use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    CreateEngineJobRunRequest, EngineRecord, EngineSummary, FinishEngineJobRunRequest,
    GetThreadRepairScopeStatusRequest,
    GetThreadRepairScopeStatusResponse, RunThreadRepairScopeRequest,
    RunThreadRepairScopeResponse, RunThreadRepairSummaryResponse, RunThreadSummaryResponse,
};
use crate::repositories::{control_plane as cp_repo, records, summaries, threads};
use crate::services::control_plane;

const DEFAULT_MAX_RECORDS: i64 = 20;
const DEFAULT_ROLLUP_TOKEN_LIMIT: i64 = 6000;
const DEFAULT_ROLLUP_ROUND_LIMIT: i64 = 8;
const DEFAULT_ROLLUP_TARGET_TOKENS: i64 = 700;
const DEFAULT_REPAIR_SCOPE_MAX_THREADS: i64 = 5000;
const REPAIR_SUMMARY_PROMPT: &str = r#"You are generating a repair summary for a memory engine.
Your goal is not ordinary compression. Your goal is to restore a trustworthy context state.

Rules:
1. Keep only facts grounded in the conversation records.
2. If assistant claims look speculative, unsupported, or contradicted, mark them as unverified or incorrect.
3. Prefer explicit user corrections and concrete evidence from the conversation.
4. If something is unknown, say it is unknown rather than filling gaps.
5. Highlight constraints the next model should respect.

Output sections with these exact headings:
Confirmed Facts
Incorrect Or Unverified Claims
Still Unclear
Next-Turn Constraints
Current User Goal"#;

#[derive(Debug, Clone)]
pub struct RollupSettings {
    pub summary_prompt: Option<String>,
    pub round_limit: i64,
    pub token_limit: i64,
    pub target_summary_tokens: i64,
    pub keep_level0_count: i64,
    pub max_level: i64,
}

#[derive(Debug, Clone)]
pub struct ThreadRollupResult {
    pub generated: usize,
    pub marked: usize,
}

pub fn default_rollup_settings() -> RollupSettings {
    RollupSettings {
        summary_prompt: None,
        round_limit: DEFAULT_ROLLUP_ROUND_LIMIT,
        token_limit: DEFAULT_ROLLUP_TOKEN_LIMIT,
        target_summary_tokens: DEFAULT_ROLLUP_TARGET_TOKENS,
        keep_level0_count: 5,
        max_level: 4,
    }
}

pub async fn run_thread_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    max_records: Option<usize>,
) -> Result<RunThreadSummaryResponse, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let pending_before_count = records::count_records(
        db,
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        None,
        Some("pending"),
    )
    .await?;
    let job_run = cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "thread_direct".to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(thread.subject_id.clone()),
            thread_label: None,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_l0",
                "compat_trigger_type": "manual_session",
                "pending_before_count": pending_before_count,
            })),
        },
    )
    .await?;

    let pending_records = records::list_pending_records(
        db,
        thread_id,
        max_records.unwrap_or(DEFAULT_MAX_RECORDS as usize).max(1) as i64,
    )
    .await
    .inspect_err(|err| {
        let _ = err;
    })?;

    if pending_records.is_empty() {
        let _ = cp_repo::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: Some(serde_json::json!({
                    "compat_job_type": "summary_l0",
                    "compat_trigger_type": "manual_session",
                    "pending_before_count": pending_before_count,
                    "selected_count": 0,
                    "marked_count": 0,
                    "pending_after_count": pending_before_count,
                })),
                error_message: None,
            },
        )
        .await;
        return Ok(RunThreadSummaryResponse {
            thread_id: thread_id.to_string(),
            generated: false,
            summary_id: None,
            source_record_count: 0,
        });
    }

    let summary_text =
        build_summary_text(config, db, thread.title.as_deref(), &pending_records).await?;
    let summary = summaries::create_thread_summary(
        db,
        tenant_id,
        source_id,
        thread_id,
        thread.subject_id.as_str(),
        summary_text.as_str(),
        pending_records.first().map(|item| item.id.clone()),
        pending_records.last().map(|item| item.id.clone()),
        pending_records.len(),
    )
    .await?;

    let record_ids = pending_records
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    records::mark_records_summarized(db, thread_id, record_ids.as_slice(), summary.id.as_str())
        .await?;
    let pending_after_count = records::count_records(
        db,
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        None,
        Some("pending"),
    )
    .await?;
    let _ = cp_repo::finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: "done".to_string(),
            input_count: pending_records.len() as i64,
            output_count: 1,
            processed_count: pending_records.len() as i64,
            success_count: pending_records.len() as i64,
            error_count: 0,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_l0",
                "compat_trigger_type": "manual_session",
                "pending_before_count": pending_before_count,
                "selected_count": pending_records.len(),
                "marked_count": pending_records.len(),
                "pending_after_count": pending_after_count,
                "generated_summary_id": summary.id,
            })),
            error_message: None,
        },
    )
    .await;

    Ok(RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: true,
        summary_id: Some(summary.id),
        source_record_count: pending_records.len(),
    })
}

pub async fn run_thread_repair_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    max_records: Option<usize>,
) -> Result<RunThreadRepairSummaryResponse, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let pending_before_count = records::count_records(
        db,
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        None,
        Some("pending"),
    )
    .await?;
    let job_run = cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "thread_repair".to_string(),
            trigger_type: "thread_direct".to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(thread.subject_id.clone()),
            thread_label: None,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_review_repair",
                "compat_trigger_type": "manual_review_repair",
                "pending_before_count": pending_before_count,
            })),
        },
    )
    .await?;

    let selected_records = records::list_recent_records(
        db,
        thread_id,
        max_records.unwrap_or(DEFAULT_MAX_RECORDS as usize).max(1) as i64,
    )
    .await?;

    if selected_records.is_empty() {
        let _ = cp_repo::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: Some(serde_json::json!({
                    "compat_job_type": "summary_review_repair",
                    "compat_trigger_type": "manual_review_repair",
                    "pending_before_count": pending_before_count,
                    "selected_count": 0,
                    "marked_count": 0,
                    "pending_after_count": pending_before_count,
                })),
                error_message: None,
            },
        )
        .await;
        return Ok(RunThreadRepairSummaryResponse {
            thread_id: thread_id.to_string(),
            generated: false,
            summary_id: None,
            source_record_count: 0,
        });
    }

    let summary_text =
        build_repair_summary_text(config, db, thread.title.as_deref(), &selected_records).await?;
    let summary = summaries::create_thread_summary_with_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        thread.subject_id.as_str(),
        "thread_repair",
        None,
        summary_text.as_str(),
        selected_records.first().map(|item| item.id.clone()),
        selected_records.last().map(|item| item.id.clone()),
        selected_records.len(),
        Some(serde_json::json!({
            "generator": "memory_engine_repair_v1",
            "summary_role": "repair"
        })),
    )
    .await?;
    let _ = cp_repo::finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: "done".to_string(),
            input_count: selected_records.len() as i64,
            output_count: 1,
            processed_count: selected_records.len() as i64,
            success_count: selected_records.len() as i64,
            error_count: 0,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_review_repair",
                "compat_trigger_type": "manual_review_repair",
                "pending_before_count": pending_before_count,
                "selected_count": selected_records.len(),
                "marked_count": 0,
                "pending_after_count": pending_before_count,
                "generated_summary_id": summary.id,
            })),
            error_message: None,
        },
    )
    .await;

    Ok(RunThreadRepairSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: true,
        summary_id: Some(summary.id),
        source_record_count: selected_records.len(),
    })
}

pub async fn run_thread_repair_scope(
    config: &AppConfig,
    db: &Db,
    req: RunThreadRepairScopeRequest,
) -> Result<RunThreadRepairScopeResponse, String> {
    let normalized_label = req.thread_label.trim();
    if normalized_label.is_empty() {
        return Err("empty thread_label".to_string());
    }

    let normalized_record_type = req
        .pending_record_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let threads_in_scope = threads::list_threads_by_label(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        normalized_label,
        req.thread_status.as_deref(),
        req.max_threads
            .unwrap_or(DEFAULT_REPAIR_SCOPE_MAX_THREADS)
            .max(1)
            .min(DEFAULT_REPAIR_SCOPE_MAX_THREADS),
        0,
    )
    .await?;

    let mut processable_thread_ids = Vec::new();
    let mut pending_record_count = 0_i64;
    for thread in &threads_in_scope {
        let pending = records::count_records(
            db,
            thread.id.as_str(),
            Some(req.tenant_id.as_str()),
            Some(req.source_id.as_str()),
            None,
            normalized_record_type,
            Some("pending"),
        )
        .await?;
        if pending > 0 {
            processable_thread_ids.push(thread.id.clone());
            pending_record_count += pending;
        }
    }

    let mut summarized_threads = 0usize;
    let mut generated_summaries = 0usize;
    for thread_id in &processable_thread_ids {
        match run_thread_repair_summary(
            config,
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread_id.as_str(),
            req.max_records_per_thread,
        )
        .await
        {
            Ok(result) => {
                if result.generated {
                    summarized_threads += 1;
                    generated_summaries += 1;
                }
            }
            Err(err) => {
                return Err(format!(
                    "run repair summary failed for thread_id={}: {}",
                    thread_id, err
                ));
            }
        }
    }

    Ok(RunThreadRepairScopeResponse {
        thread_label: normalized_label.to_string(),
        scope_thread_count: threads_in_scope.len(),
        processed_threads: processable_thread_ids.len(),
        summarized_threads,
        generated_summaries,
        failed_threads: 0,
        pending_record_count,
    })
}

pub async fn get_thread_repair_scope_status(
    db: &Db,
    req: GetThreadRepairScopeStatusRequest,
) -> Result<GetThreadRepairScopeStatusResponse, String> {
    let normalized_label = req.thread_label.trim();
    if normalized_label.is_empty() {
        return Err("empty thread_label".to_string());
    }

    let normalized_record_type = req
        .pending_record_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let threads_in_scope = threads::list_threads_by_label(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        normalized_label,
        req.thread_status.as_deref(),
        req.max_threads
            .unwrap_or(DEFAULT_REPAIR_SCOPE_MAX_THREADS)
            .max(1)
            .min(DEFAULT_REPAIR_SCOPE_MAX_THREADS),
        0,
    )
    .await?;

    let mut pending_record_count = 0_i64;
    for thread in &threads_in_scope {
        pending_record_count += records::count_records(
            db,
            thread.id.as_str(),
            Some(req.tenant_id.as_str()),
            Some(req.source_id.as_str()),
            None,
            normalized_record_type,
            Some("pending"),
        )
        .await?;
    }

    Ok(GetThreadRepairScopeStatusResponse {
        thread_label: normalized_label.to_string(),
        running: false,
        running_job_count: 0,
        pending_record_count,
        scope_thread_count: threads_in_scope.len(),
        job_type: "memory_engine_thread_repair".to_string(),
    })
}

pub async fn run_thread_rollup(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    settings: &RollupSettings,
) -> Result<ThreadRollupResult, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let job_run = cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "rollup".to_string(),
            trigger_type: "thread_direct".to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(thread.subject_id.clone()),
            thread_label: None,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_rollup",
                "compat_trigger_type": "manual_rollup",
            })),
        },
    )
    .await?;

    let selection = select_rollup_batch(
        db,
        thread.id.as_str(),
        settings.round_limit.max(1),
        settings.token_limit.max(500),
        settings.keep_level0_count.max(0),
        settings.max_level.max(1),
    )
    .await?;

    let Some((level, selected, trigger_reason)) = selection else {
        let _ = cp_repo::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: Some(serde_json::json!({
                    "compat_job_type": "summary_rollup",
                    "compat_trigger_type": "manual_rollup",
                    "selected_count": 0,
                    "marked_count": 0,
                    "pending_after_count": 0,
                })),
                error_message: None,
            },
        )
        .await;
        return Ok(ThreadRollupResult {
            generated: 0,
            marked: 0,
        });
    };

    let target_level = level + 1;
    let selected_ids = selected.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
    let source_digest = build_summary_digest(
        thread.id.as_str(),
        level,
        target_level,
        selected_ids.as_slice(),
    );

    if let Some(existing) = summaries::find_summary_by_source_digest(
        db,
        thread.id.as_str(),
        target_level,
        source_digest.as_str(),
    )
    .await?
    {
        let marked = summaries::mark_summaries_rolled_up(
            db,
            thread.id.as_str(),
            selected_ids.as_slice(),
            existing.id.as_str(),
        )
        .await?;
        let _ = cp_repo::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: selected.len() as i64,
                output_count: 0,
                processed_count: selected.len() as i64,
                success_count: marked as i64,
                error_count: 0,
                metadata: Some(serde_json::json!({
                    "compat_job_type": "summary_rollup",
                    "compat_trigger_type": "manual_rollup",
                    "selected_count": selected.len(),
                    "marked_count": marked,
                    "pending_after_count": 0,
                    "rollup_summary_id": existing.id,
                    "trigger_reason": trigger_reason,
                })),
                error_message: None,
            },
        )
        .await;
        return Ok(ThreadRollupResult {
            generated: 0,
            marked,
        });
    }

    let mut summarizable = Vec::new();
    let mut oversized = 0usize;
    for summary in &selected {
        let block = summary_to_rollup_block(summary);
        if estimate_tokens_text(block.as_str()) > settings.token_limit.max(500) {
            oversized += 1;
        } else {
            summarizable.push(block);
        }
    }

    let mut summary_text = if summarizable.is_empty() {
        format!(
            "All {} selected summaries at level {} exceeded token_limit={}, so this rollup only marks the batch as rolled up.",
            selected.len(),
            level,
            settings.token_limit.max(500)
        )
    } else {
        build_rollup_summary_text(
            config,
            db,
            thread.title.as_deref(),
            summarizable.as_slice(),
            settings,
            level,
            target_level,
        )
        .await?
    };

    if oversized > 0 {
        summary_text.push_str(&format!(
            "\n\n[meta] {} oversized summaries were not merged into the rollup body but were marked as rolled up.",
            oversized
        ));
    }

    let created = summaries::create_rollup_summary(
        db,
        tenant_id,
        source_id,
        thread.id.as_str(),
        thread.subject_id.as_str(),
        target_level,
        Some(source_digest),
        summary_text.as_str(),
        selected.first().map(|item| item.id.clone()),
        selected.last().map(|item| item.id.clone()),
        selected.len(),
        Some(serde_json::json!({
            "generator": "memory_engine_rollup_v1",
            "trigger_reason": trigger_reason,
            "source_level": level,
            "target_level": target_level,
        })),
    )
    .await?;

    let marked = summaries::mark_summaries_rolled_up(
        db,
        thread.id.as_str(),
        selected_ids.as_slice(),
        created.id.as_str(),
    )
    .await?;
    let _ = cp_repo::finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: "done".to_string(),
            input_count: selected.len() as i64,
            output_count: 1,
            processed_count: selected.len() as i64,
            success_count: marked as i64,
            error_count: 0,
            metadata: Some(serde_json::json!({
                "compat_job_type": "summary_rollup",
                "compat_trigger_type": "manual_rollup",
                "selected_count": selected.len(),
                "marked_count": marked,
                "pending_after_count": 0,
                "rollup_summary_id": created.id,
                "trigger_reason": trigger_reason,
            })),
            error_message: None,
        },
    )
    .await;

    Ok(ThreadRollupResult {
        generated: 1,
        marked,
    })
}

async fn build_summary_text(
    config: &AppConfig,
    db: &Db,
    title: Option<&str>,
    records: &[EngineRecord],
) -> Result<String, String> {
    let rule_based = build_rule_based_summary(title, records);
    let ai = control_plane::build_ai_client_for_job(config, db, "summary").await?;
    if !ai.is_enabled() {
        return Ok(rule_based);
    }

    let input = records
        .iter()
        .map(|item| format!("[{}][{}] {}", item.created_at, item.role, item.content))
        .collect::<Vec<_>>()
        .join("\n");

    match ai.summarize(title, input.as_str(), Some(500)).await {
        Ok(text) => Ok(text),
        Err(err) if ai.allow_rule_fallback() => Ok(format!(
            "{}\n\n[ai_fallback_reason] {}",
            rule_based, err
        )),
        Err(err) => Err(err),
    }
}

async fn build_repair_summary_text(
    config: &AppConfig,
    db: &Db,
    title: Option<&str>,
    records: &[EngineRecord],
) -> Result<String, String> {
    let rule_based = build_rule_based_repair_summary(title, records);
    let ai = control_plane::build_ai_client_for_job(config, db, "thread_repair").await?;
    if !ai.is_enabled() {
        return Ok(rule_based);
    }

    let input = records
        .iter()
        .map(|item| format!("[{}][{}] {}", item.created_at, item.role, item.content))
        .collect::<Vec<_>>()
        .join("\n");

    match ai
        .summarize(
            title,
            format!("{REPAIR_SUMMARY_PROMPT}\n\nConversation records:\n{input}").as_str(),
            Some(700),
        )
        .await
    {
        Ok(text) => Ok(text),
        Err(err) if ai.allow_rule_fallback() => Ok(format!(
            "{}\n\n[ai_fallback_reason] {}",
            rule_based, err
        )),
        Err(err) => Err(err),
    }
}

async fn build_rollup_summary_text(
    config: &AppConfig,
    db: &Db,
    title: Option<&str>,
    items: &[String],
    settings: &RollupSettings,
    level: i64,
    target_level: i64,
) -> Result<String, String> {
    let rule_based = build_rule_based_rollup_summary(items, level, target_level);
    let ai = control_plane::build_ai_client_for_job(config, db, "rollup").await?;
    if !ai.is_enabled() {
        return Ok(rule_based);
    }

    let prompt_title = title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Thread rollup level {} -> {}", level, target_level));
    let input = items.join("\n\n---\n\n");
    let custom_prefix = settings
        .summary_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n"))
        .unwrap_or_default();

    match ai
        .summarize(
            Some(prompt_title.as_str()),
            format!(
                "{custom_prefix}Merge these prior thread summaries into a higher-level rollup summary. Preserve concrete constraints, current goals, and durable facts.\n\nSource summaries:\n{input}"
            )
            .as_str(),
            Some(settings.target_summary_tokens.max(256)),
        )
        .await
    {
        Ok(text) => Ok(text),
        Err(err) if ai.allow_rule_fallback() => Ok(format!(
            "{}\n\n[ai_fallback_reason] {}",
            rule_based, err
        )),
        Err(err) => Err(err),
    }
}

async fn select_rollup_batch(
    db: &Db,
    thread_id: &str,
    round_limit: i64,
    token_limit: i64,
    keep_level0_count: i64,
    max_level: i64,
) -> Result<Option<(i64, Vec<EngineSummary>, &'static str)>, String> {
    for level in 0..max_level {
        let mut candidates = summaries::list_pending_summaries_by_level(db, thread_id, level).await?;
        if level == 0 && keep_level0_count > 0 {
            let keep = keep_level0_count as usize;
            if candidates.len() > keep {
                let rollup_len = candidates.len().saturating_sub(keep);
                candidates.truncate(rollup_len);
            } else {
                candidates.clear();
            }
        }

        if candidates.is_empty() {
            continue;
        }

        if candidates.len() as i64 >= round_limit {
            return Ok(Some((
                level,
                candidates.into_iter().take(round_limit as usize).collect(),
                "message_count_limit",
            )));
        }

        let token_sum = candidates
            .iter()
            .map(summary_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some((level, candidates, "token_limit")));
        }
    }

    Ok(None)
}

fn build_summary_digest(
    thread_id: &str,
    level: i64,
    target_level: i64,
    summary_ids: &[String],
) -> String {
    format!(
        "thread_rollup:{}:{}:{}:{}",
        thread_id,
        level,
        target_level,
        summary_ids.join(",")
    )
}

fn estimate_tokens_text(text: &str) -> i64 {
    (text.chars().count() as i64 / 4).max(1)
}

fn summary_to_rollup_block(summary: &EngineSummary) -> String {
    format!(
        "[level={}][created_at={}][id={}]\n{}",
        summary.level, summary.created_at, summary.id, summary.summary_text
    )
}

fn build_rule_based_summary(title: Option<&str>, records: &[EngineRecord]) -> String {
    let mut lines = Vec::new();
    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("Thread: {}", title));
    }

    let first_user = records
        .iter()
        .find(|item| item.role == "user")
        .map(|item| normalize_line(item.content.as_str(), 160));
    let last_assistant = records
        .iter()
        .rev()
        .find(|item| item.role == "assistant")
        .map(|item| normalize_line(item.content.as_str(), 160));

    lines.push(format!("New records: {}", records.len()));

    if let Some(text) = first_user {
        lines.push(format!("User asked: {}", text));
    }
    if let Some(text) = last_assistant {
        lines.push(format!("Latest assistant response: {}", text));
    }

    let timeline = records
        .iter()
        .take(8)
        .map(|item| format!("- [{}] {}", item.role, normalize_line(item.content.as_str(), 120)))
        .collect::<Vec<_>>();
    if !timeline.is_empty() {
        lines.push("Timeline:".to_string());
        lines.extend(timeline);
    }

    lines.join("\n")
}

fn build_rule_based_repair_summary(title: Option<&str>, records: &[EngineRecord]) -> String {
    let mut lines = Vec::new();
    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("Thread: {}", title));
    }

    lines.push("Confirmed Facts".to_string());
    if let Some(first_user) = records
        .iter()
        .find(|item| item.role == "user")
        .map(|item| normalize_line(item.content.as_str(), 180))
    {
        lines.push(format!("- Initial user request: {}", first_user));
    }
    if let Some(last_user) = records
        .iter()
        .rev()
        .find(|item| item.role == "user")
        .map(|item| normalize_line(item.content.as_str(), 180))
    {
        lines.push(format!("- Latest user position: {}", last_user));
    }

    lines.push("Incorrect Or Unverified Claims".to_string());
    let assistant_lines = records
        .iter()
        .filter(|item| item.role == "assistant")
        .take(3)
        .map(|item| {
            format!(
                "- Review this assistant claim: {}",
                normalize_line(item.content.as_str(), 180)
            )
        })
        .collect::<Vec<_>>();
    if assistant_lines.is_empty() {
        lines.push("- No assistant claims available in selected records.".to_string());
    } else {
        lines.extend(assistant_lines);
    }

    lines.push("Still Unclear".to_string());
    lines.push(
        "- Verify any file paths, APIs, or conclusions not directly supported by records."
            .to_string(),
    );

    lines.push("Next-Turn Constraints".to_string());
    lines.push("- Prefer tool-backed facts and explicit user corrections.".to_string());
    lines.push("- Do not inherit unsupported assistant assumptions as facts.".to_string());

    lines.push("Current User Goal".to_string());
    if let Some(goal) = records
        .iter()
        .rev()
        .find(|item| item.role == "user")
        .map(|item| normalize_line(item.content.as_str(), 180))
    {
        lines.push(format!("- {}", goal));
    } else {
        lines.push("- Need more user input.".to_string());
    }

    lines.join("\n")
}

fn build_rule_based_rollup_summary(items: &[String], level: i64, target_level: i64) -> String {
    let mut lines = vec![
        format!("Rollup level {} -> {}", level, target_level),
        format!("Merged summaries: {}", items.len()),
        "Highlights:".to_string(),
    ];
    lines.extend(
        items.iter()
            .take(6)
            .map(|item| format!("- {}", normalize_line(item.as_str(), 200))),
    );
    lines.join("\n")
}

fn normalize_line(input: &str, max_chars: usize) -> String {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        let shortened = compact.chars().take(max_chars).collect::<String>();
        format!("{}...", shortened)
    }
}
