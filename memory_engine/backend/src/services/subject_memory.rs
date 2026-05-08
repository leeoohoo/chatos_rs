use std::collections::{BTreeMap, VecDeque};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineSubjectMemory, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    UpsertSubjectMemoryRequest,
};
use crate::repositories::{subject_memories, summaries};

const DEFAULT_TOKEN_LIMIT: i64 = 6000;
const DEFAULT_ROUND_LIMIT: i64 = 20;
const DEFAULT_TARGET_SUMMARY_TOKENS: i64 = 700;
const DEFAULT_MAX_LEVEL: i64 = 4;
const DEFAULT_MAX_SOURCE_SUMMARIES: i64 = 5000;
const MIN_TOKEN_LIMIT: i64 = 128;
const MAX_OVERFLOW_RETRIES: usize = 4;
const MAX_MERGE_ROUNDS: usize = 16;

#[derive(Debug, Clone)]
struct SubjectMemoryJobSettings {
    relation_subject_id: String,
    source_summary_type: String,
    summary_prompt: Option<String>,
    prompt_title: String,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_level0_count: i64,
    max_level: i64,
    memory_metadata: Option<Value>,
}

#[derive(Debug, Clone)]
struct PendingSourceSummary {
    id: String,
    thread_id: String,
    summary_type: String,
    summary_text: String,
    created_at: String,
    metadata: Option<Value>,
}

#[derive(Debug, Clone)]
struct RollupSelection {
    level: i64,
    selected: Vec<EngineSubjectMemory>,
}

#[derive(Debug, Clone)]
struct SummaryBuildResult {
    text: String,
    chunk_count: usize,
    overflow_retry_count: usize,
    forced_truncated: bool,
}

pub async fn run_subject_memory_job(
    config: &AppConfig,
    db: &Db,
    req: RunSubjectMemoryJobRequest,
) -> Result<RunSubjectMemoryJobResponse, String> {
    let settings = build_settings(&req)?;

    let pending_summaries = summaries::list_summaries_by_thread_label(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.source_thread_label.as_str(),
        Some(settings.source_summary_type.as_str()),
        Some("done"),
        None,
        Some(0),
        DEFAULT_MAX_SOURCE_SUMMARIES,
        0,
    )
    .await?
    .into_iter()
    .map(|item| PendingSourceSummary {
        id: item.id,
        thread_id: item.thread_id,
        summary_type: item.summary_type,
        summary_text: item.summary_text,
        created_at: item.created_at,
        metadata: item.metadata,
    })
    .collect::<Vec<_>>();

    let mut generated_level0 = 0usize;
    let mut generated_rollups = 0usize;
    let mut marked_source_summaries = 0usize;
    let mut marked_source_memories = 0usize;

    if let Some(selected) = select_summary_batch(
        pending_summaries.as_slice(),
        settings.round_limit,
        settings.token_limit,
    ) {
        let selected_ids = selected.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
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
            marked_source_summaries +=
                mark_summary_sources_subject_memory_summarized(db, selected.as_slice()).await?;
            info!(
                "[MEMORY-ENGINE-SUBJECT] reused level0 subject_id={} memory_type={} digest={} memory_key={}",
                req.subject_id, req.memory_type, source_digest, existing.memory_key
            );
        } else {
            let selected_texts = selected
                .iter()
                .map(summary_to_subject_memory_block)
                .collect::<Vec<_>>();
            let build = build_subject_memory_from_summaries(
                config,
                settings.prompt_title.as_str(),
                settings.summary_prompt.as_deref(),
                selected_texts.as_slice(),
                settings.token_limit,
                settings.target_summary_tokens,
            )
            .await?;
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
                Some(source_digest),
                "pending",
            )
            .await?;
            generated_level0 = 1;
            marked_source_summaries +=
                mark_summary_sources_subject_memory_summarized(db, selected.as_slice()).await?;
        }
    }

    if let Some(selection) = select_rollup_batch(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.subject_id.as_str(),
        settings.relation_subject_id.as_str(),
        req.memory_type.as_str(),
        settings.round_limit,
        settings.token_limit,
        settings.keep_level0_count,
        settings.max_level,
    )
    .await?
    {
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
            marked_source_memories += subject_memories::mark_subject_memories_rolled_up(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                req.subject_id.as_str(),
                selected_ids.as_slice(),
                existing.memory_key.as_str(),
            )
            .await?;
            info!(
                "[MEMORY-ENGINE-SUBJECT] reused rollup subject_id={} memory_type={} level={}->{} digest={} memory_key={}",
                req.subject_id, req.memory_type, level, target_level, source_digest, existing.memory_key
            );
        } else {
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
                    forced_truncated: false,
                }
            } else {
                build_subject_memory_rollup(
                    config,
                    settings.prompt_title.as_str(),
                    settings.summary_prompt.as_deref(),
                    summarizable.as_slice(),
                    settings.token_limit,
                    settings.target_summary_tokens,
                    level,
                    target_level,
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
            let memory_req = UpsertSubjectMemoryRequest {
                id: None,
                tenant_id: req.tenant_id.clone(),
                source_id: req.source_id.clone(),
                memory_type: req.memory_type.clone(),
                text: memory_text,
                level: Some(target_level),
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
                Some(source_digest),
                "pending",
            )
            .await?;
            generated_rollups = 1;
            marked_source_memories += subject_memories::mark_subject_memories_rolled_up(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                req.subject_id.as_str(),
                selected_ids.as_slice(),
                memory_key.as_str(),
            )
            .await?;
        }
    }

    Ok(RunSubjectMemoryJobResponse {
        subject_id: req.subject_id,
        generated_level0,
        generated_rollups,
        generated_memories: generated_level0 + generated_rollups,
        marked_source_summaries,
        marked_source_memories,
    })
}

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
        prompt_title: req
            .prompt_title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("Subject memory {}", req.subject_id)),
        round_limit: req.round_limit.unwrap_or(DEFAULT_ROUND_LIMIT).max(1),
        token_limit: req.token_limit.unwrap_or(DEFAULT_TOKEN_LIMIT).max(500),
        target_summary_tokens: req
            .target_summary_tokens
            .unwrap_or(DEFAULT_TARGET_SUMMARY_TOKENS)
            .max(128),
        keep_level0_count: req.keep_level0_count.unwrap_or(0).max(0),
        max_level: req.max_level.unwrap_or(DEFAULT_MAX_LEVEL).max(1),
        memory_metadata: req.memory_metadata.clone(),
    })
}

fn select_summary_batch(
    candidates: &[PendingSourceSummary],
    round_limit: i64,
    token_limit: i64,
) -> Option<Vec<PendingSourceSummary>> {
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() as i64 >= round_limit {
        return Some(
            candidates
                .iter()
                .take(round_limit as usize)
                .cloned()
                .collect(),
        );
    }

    let token_sum = candidates
        .iter()
        .map(summary_to_subject_memory_block)
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();
    if token_sum >= token_limit {
        return Some(candidates.to_vec());
    }

    None
}

async fn select_rollup_batch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    relation_subject_id: &str,
    memory_type: &str,
    round_limit: i64,
    token_limit: i64,
    keep_level0_count: i64,
    max_level: i64,
) -> Result<Option<RollupSelection>, String> {
    for level in 0..max_level {
        let mut candidates = subject_memories::list_pending_subject_memories_by_level(
            db,
            tenant_id,
            source_id,
            subject_id,
            relation_subject_id,
            memory_type,
            level,
        )
        .await?;

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
            return Ok(Some(RollupSelection {
                level,
                selected: candidates.into_iter().take(round_limit as usize).collect(),
            }));
        }

        let token_sum = candidates
            .iter()
            .map(subject_memory_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some(RollupSelection {
                level,
                selected: candidates,
            }));
        }
    }

    Ok(None)
}

async fn mark_summary_sources_subject_memory_summarized(
    db: &Db,
    selected: &[PendingSourceSummary],
) -> Result<usize, String> {
    if selected.is_empty() {
        return Ok(0);
    }

    let mut grouped = BTreeMap::<String, Vec<String>>::new();
    for item in selected {
        grouped
            .entry(item.thread_id.clone())
            .or_default()
            .push(item.id.clone());
    }

    let mut marked = 0usize;
    for (thread_id, summary_ids) in grouped {
        marked += summaries::mark_summaries_subject_memory_summarized(
            db,
            thread_id.as_str(),
            summary_ids.as_slice(),
        )
        .await?;
    }

    Ok(marked)
}

async fn build_subject_memory_from_summaries(
    config: &AppConfig,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<SummaryBuildResult, String> {
    let ai = AiClient::new(config)?;
    if !ai.is_enabled() {
        return Ok(SummaryBuildResult {
            text: build_rule_based_subject_memory(prompt_title, items, target_summary_tokens),
            chunk_count: 1,
            overflow_retry_count: 0,
            forced_truncated: false,
        });
    }

    summarize_texts_with_split(
        &ai,
        prompt_title,
        summary_prompt,
        "Build a durable subject memory from these conversation summaries. Preserve concrete facts, current goals, constraints, risks, and decisions.",
        "Merge these partial subject-memory summaries into one durable memory. Preserve facts, goals, constraints, risks, and decisions.",
        items,
        token_limit,
        target_summary_tokens,
    )
    .await
}

async fn build_subject_memory_rollup(
    config: &AppConfig,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    level: i64,
    target_level: i64,
) -> Result<SummaryBuildResult, String> {
    let ai = AiClient::new(config)?;
    if !ai.is_enabled() {
        return Ok(SummaryBuildResult {
            text: build_rule_based_rollup_memory(prompt_title, items, level, target_level),
            chunk_count: 1,
            overflow_retry_count: 0,
            forced_truncated: false,
        });
    }

    summarize_texts_with_split(
        &ai,
        prompt_title,
        summary_prompt,
        format!(
            "Roll up these prior subject memories from level {} to level {}. Preserve durable facts, active goals, constraints, and risks.",
            level, target_level
        )
        .as_str(),
        format!(
            "Merge these partial subject-memory rollups for level {} to level {} into one durable memory.",
            level, target_level
        )
        .as_str(),
        items,
        token_limit,
        target_summary_tokens,
    )
    .await
}

async fn summarize_texts_with_split(
    ai: &AiClient,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    leaf_directive: &str,
    merge_directive: &str,
    items: &[String],
    token_limit: i64,
    target_tokens: i64,
) -> Result<SummaryBuildResult, String> {
    if items.is_empty() {
        return Err("empty summarize items".to_string());
    }

    let mut overflow_retry_count = 0usize;
    let mut effective_token_limit = token_limit.max(500);

    loop {
        match summarize_texts_once(
            ai,
            prompt_title,
            summary_prompt,
            leaf_directive,
            merge_directive,
            items,
            effective_token_limit,
            target_tokens,
        )
        .await
        {
            Ok((text, chunk_count)) => {
                return Ok(SummaryBuildResult {
                    text,
                    chunk_count,
                    overflow_retry_count,
                    forced_truncated: false,
                });
            }
            Err(err) if is_context_overflow_error(err.as_str()) => {
                overflow_retry_count += 1;
                if overflow_retry_count > MAX_OVERFLOW_RETRIES {
                    break;
                }
                let next = (effective_token_limit / 2).max(MIN_TOKEN_LIMIT);
                if next >= effective_token_limit {
                    break;
                }
                effective_token_limit = next;
            }
            Err(err) => return Err(err),
        }
    }

    Ok(SummaryBuildResult {
        text: force_truncated_summary(items, target_tokens, prompt_title, overflow_retry_count),
        chunk_count: 1,
        overflow_retry_count,
        forced_truncated: true,
    })
}

async fn summarize_texts_once(
    ai: &AiClient,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    leaf_directive: &str,
    merge_directive: &str,
    items: &[String],
    token_limit: i64,
    target_tokens: i64,
) -> Result<(String, usize), String> {
    let chunks = split_chunks_by_token_limit(items, token_limit.max(MIN_TOKEN_LIMIT));
    if chunks.is_empty() {
        return Err("no chunks".to_string());
    }

    let mut chunk_summaries = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let input = build_ai_input(summary_prompt, leaf_directive, chunk.as_slice());
        let text = ai
            .summarize(Some(prompt_title), input.as_str(), Some(target_tokens))
            .await?;
        chunk_summaries.push(text);
    }

    let merged = merge_chunk_summaries(
        ai,
        prompt_title,
        summary_prompt,
        merge_directive,
        chunk_summaries,
        token_limit,
        target_tokens,
    )
    .await?;

    Ok((merged, chunks.len()))
}

async fn merge_chunk_summaries(
    ai: &AiClient,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    merge_directive: &str,
    summaries: Vec<String>,
    token_limit: i64,
    target_tokens: i64,
) -> Result<String, String> {
    if summaries.is_empty() {
        return Err("empty summaries for merge".to_string());
    }
    if summaries.len() == 1 {
        return Ok(summaries[0].clone());
    }

    let mut round = 1usize;
    let mut current = summaries;

    while current.len() > 1 {
        if round > MAX_MERGE_ROUNDS {
            return Err("context_length_exceeded: merge rounds exceeded".to_string());
        }

        let groups = split_chunks_by_token_limit(current.as_slice(), token_limit.max(MIN_TOKEN_LIMIT));
        let mut next = Vec::with_capacity(groups.len());
        let mut progressed = false;

        for group in groups {
            if group.len() <= 1 {
                next.extend(group.into_iter());
                continue;
            }

            progressed = true;
            let input = build_ai_input(summary_prompt, merge_directive, group.as_slice());
            let text = ai
                .summarize(
                    Some(format!("{} merge round {}", prompt_title, round).as_str()),
                    input.as_str(),
                    Some(target_tokens.max(256)),
                )
                .await?;
            next.push(text);
        }

        if !progressed {
            return Err("context_length_exceeded: merge chunks are individually oversized".to_string());
        }

        current = next;
        round += 1;
    }

    current
        .into_iter()
        .next()
        .ok_or_else(|| "empty merged summary".to_string())
}

fn build_ai_input(summary_prompt: Option<&str>, directive: &str, items: &[String]) -> String {
    let custom_prefix = summary_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n"))
        .unwrap_or_default();
    let body = items.join("\n\n---\n\n");
    format!("{custom_prefix}{directive}\n\nSource items:\n{body}")
}

fn build_rule_based_subject_memory(prompt_title: &str, items: &[String], target_tokens: i64) -> String {
    let mut lines = vec![
        format!("Subject memory: {}", prompt_title),
        format!("Source items: {}", items.len()),
        "Key points:".to_string(),
    ];

    for item in items.iter().take(12) {
        let short = normalize_line(item.as_str(), 240);
        if !short.is_empty() {
            lines.push(format!("- {}", short));
        }
    }

    let mut out = lines.join("\n");
    let max_chars = (target_tokens.max(128) as usize).saturating_mul(4);
    if out.chars().count() > max_chars {
        out = out.chars().take(max_chars).collect::<String>();
        out.push_str("\n...[truncated]");
    }
    out
}

fn build_rule_based_rollup_memory(
    prompt_title: &str,
    items: &[String],
    level: i64,
    target_level: i64,
) -> String {
    let mut lines = vec![
        format!("Subject memory rollup: {}", prompt_title),
        format!("Source level: {} -> {}", level, target_level),
        format!("Merged items: {}", items.len()),
        "Highlights:".to_string(),
    ];

    for item in items.iter().take(10) {
        let short = normalize_line(item.as_str(), 220);
        if !short.is_empty() {
            lines.push(format!("- {}", short));
        }
    }

    lines.join("\n")
}

fn summary_to_subject_memory_block(item: &PendingSourceSummary) -> String {
    let project_prefix = project_id_from_summary_metadata(item.metadata.as_ref())
        .map(|project_id| format!("[project_id={}]", project_id))
        .unwrap_or_default();
    format!(
        "{}[summary_id={}][thread_id={}][created_at={}][summary_type={}]\n{}",
        project_prefix,
        item.id,
        item.thread_id,
        item.created_at,
        item.summary_type,
        item.summary_text
    )
}

fn subject_memory_to_rollup_block(item: &EngineSubjectMemory) -> String {
    format!(
        "[level={}][memory_key={}][updated_at={}]\n{}",
        item.level, item.memory_key, item.updated_at, item.text
    )
}

fn project_id_from_summary_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|value| value.get("legacy_session_mapping"))
        .and_then(|mapping| mapping.get("project_id"))
        .and_then(Value::as_str)
        .or_else(|| metadata.and_then(|value| value.get("project_id")).and_then(Value::as_str))
        .or_else(|| metadata.and_then(|value| value.get("projectId")).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn build_memory_metadata(
    memory_metadata: Option<Value>,
    relation_subject_id: &str,
    source_thread_label: &str,
) -> Option<Value> {
    let mut map = match memory_metadata {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    map.insert(
        "relation_subject_id".to_string(),
        Value::String(relation_subject_id.to_string()),
    );
    map.insert(
        "source_thread_label".to_string(),
        Value::String(source_thread_label.to_string()),
    );
    Some(Value::Object(map))
}

fn decorate_generated_text(
    build: SummaryBuildResult,
    oversized_count: Option<usize>,
    label: &str,
    keep_level0_count: i64,
) -> String {
    let mut text = build.text;
    if build.chunk_count > 1 {
        text.push_str(&format!(
            "\n\n[meta] This {} was merged from {} chunks.",
            label, build.chunk_count
        ));
    }
    if build.overflow_retry_count > 0 {
        text.push_str(&format!(
            "\n\n[meta] Context overflow retry count: {}.",
            build.overflow_retry_count
        ));
    }
    if build.forced_truncated {
        text.push_str("\n\n[meta] Forced truncation fallback was used.");
    }
    if let Some(count) = oversized_count.filter(|value| *value > 0) {
        text.push_str(&format!(
            "\n\n[meta] {} oversized source memories were marked rolled up without being merged into the body.",
            count
        ));
    }
    if keep_level0_count > 0 {
        let _ = keep_level0_count;
    }
    text
}

fn digest_from_ids(namespace: &str, ids: &[String]) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(namespace.trim().as_bytes());
    hasher.update(b"\n");

    let mut count = 0usize;
    for id in ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        hasher.update(normalized.as_bytes());
        hasher.update(b"\n");
        count += 1;
    }

    if count == 0 {
        return None;
    }

    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn estimate_tokens_text(text: &str) -> i64 {
    (text.chars().count() as i64 / 4).max(1)
}

fn split_chunks_by_token_limit(items: &[String], token_limit: i64) -> Vec<Vec<String>> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    let mut leaves = Vec::new();
    queue.push_back(items.to_vec());

    while let Some(chunk) = queue.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        let chunk_tokens = chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>();
        if chunk_tokens > token_limit && chunk.len() > 1 {
            let mid = chunk.len() / 2;
            queue.push_back(chunk[..mid].to_vec());
            queue.push_back(chunk[mid..].to_vec());
            continue;
        }

        leaves.push(chunk);
    }

    leaves
}

fn force_truncated_summary(
    items: &[String],
    target_tokens: i64,
    prompt_title: &str,
    retry_count: usize,
) -> String {
    let mut lines = vec![
        "[forced-truncated-summary] Context overflow fallback used.".to_string(),
        format!("Task: {}", prompt_title),
        format!("Overflow retry count: {}", retry_count),
        "Fallback highlights:".to_string(),
    ];

    for item in items.iter().take(12) {
        let short = item
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(240)
            .collect::<String>();
        lines.push(format!("- {}", short));
    }

    let mut out = lines.join("\n");
    let max_chars = (target_tokens.max(128) as usize).saturating_mul(4);
    if out.chars().count() > max_chars {
        out = out.chars().take(max_chars).collect::<String>();
        out.push_str("\n...[truncated]");
    }
    out
}

fn is_context_overflow_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || message.contains("context window") && message.contains("exceed")
        || message.contains("context length")
        || message.contains("token limit")
        || message.contains("prompt is too long")
        || message.contains("too many tokens")
        || message.contains("max context")
}

fn normalize_line(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect::<String>()
}
