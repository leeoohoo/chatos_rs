// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use crate::db::Db;
use crate::models::{ComposeContextBlock, EngineSubjectMemory, EngineSummary, EngineThread};
use crate::repositories::{memory_facts, subject_memories, subject_memory_scopes, summaries};

use super::policy::ResolvedComposeContextPolicy;

const RECENT_SUBJECT_MEMORY_LIMIT: i64 = 3;

pub(crate) struct BuiltContextBlocks {
    pub(crate) blocks: Vec<ComposeContextBlock>,
    pub(crate) summary_count: usize,
    pub(crate) candidate_count_by_pool: BTreeMap<String, usize>,
    pub(crate) dropped_count_by_reason: BTreeMap<String, usize>,
    pub(crate) deduplicated_count: usize,
}

pub(crate) async fn build_context_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread: &EngineThread,
    policy: &ResolvedComposeContextPolicy,
    subject_memory_subject_ids: &[String],
    context_project_subject_id: Option<&str>,
    query_text: Option<&str>,
) -> Result<BuiltContextBlocks, String> {
    let mut blocks = Vec::new();
    let summary_blocks = load_thread_summary_blocks(
        db,
        tenant_id,
        source_id,
        thread.id.as_str(),
        200,
        thread_allows_dynamic_project_recall(thread),
        context_project_subject_id,
    )
    .await?;
    let summary_count = summary_blocks.1;
    blocks.extend(summary_blocks.0);

    blocks.extend(
        load_subject_memory_blocks(db, tenant_id, source_id, subject_memory_subject_ids).await?,
    );
    let mut relevant_candidates = load_relevant_history_blocks(
        db,
        tenant_id,
        source_id,
        thread,
        context_project_subject_id,
        subject_memory_subject_ids,
        query_text,
    )
    .await?;
    blocks.append(&mut relevant_candidates);

    let candidate_count_by_pool = count_blocks_by_pool(blocks.as_slice());
    let candidate_count = blocks.len();
    let (deduplicated, deduplicated_count) = deduplicate_blocks(blocks);
    blocks = deduplicated;
    blocks = retain_blocks_within_budgets(blocks, policy);
    let mut dropped_count_by_reason = BTreeMap::new();
    if deduplicated_count > 0 {
        dropped_count_by_reason.insert("exact_duplicate".to_string(), deduplicated_count);
    }
    let dropped = candidate_count
        .saturating_sub(deduplicated_count)
        .saturating_sub(blocks.len());
    if dropped > 0 {
        dropped_count_by_reason.insert("pool_or_total_budget".to_string(), dropped);
    }

    Ok(BuiltContextBlocks {
        blocks,
        summary_count,
        candidate_count_by_pool,
        dropped_count_by_reason,
        deduplicated_count,
    })
}

pub(crate) fn count_blocks_by_pool(blocks: &[ComposeContextBlock]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for block in blocks {
        *counts.entry(block.pool.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn block_token_count(blocks: &[ComposeContextBlock]) -> usize {
    blocks.iter().map(|block| block.estimated_tokens).sum()
}

fn retain_blocks_within_budgets(
    blocks: Vec<ComposeContextBlock>,
    policy: &ResolvedComposeContextPolicy,
) -> Vec<ComposeContextBlock> {
    let mut total = 0usize;
    let mut thread = 0usize;
    let mut project = 0usize;
    let mut agent = 0usize;
    let mut relevant = 0usize;
    blocks
        .into_iter()
        .filter(|block| {
            let tokens = block.estimated_tokens;
            let pool = if block.pool == "thread_continuity" {
                &mut thread
            } else if block.pool == "project_state" {
                &mut project
            } else if block.pool == "agent_stable_memory" {
                &mut agent
            } else {
                &mut relevant
            };
            let pool_limit = if block.pool == "thread_continuity" {
                policy.max_thread_summary_tokens
            } else if block.pool == "project_state" {
                policy.max_project_memory_tokens
            } else if block.pool == "agent_stable_memory" {
                policy.max_agent_memory_tokens
            } else {
                policy.max_relevant_history_tokens
            };
            if total.saturating_add(tokens) > policy.max_memory_tokens
                || pool.saturating_add(tokens) > pool_limit
            {
                return false;
            }
            total = total.saturating_add(tokens);
            *pool = pool.saturating_add(tokens);
            true
        })
        .collect()
}

async fn load_relevant_history_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread: &EngineThread,
    context_project_subject_id: Option<&str>,
    subject_memory_subject_ids: &[String],
    query_text: Option<&str>,
) -> Result<Vec<ComposeContextBlock>, String> {
    let Some(query_text) = query_text.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    let partition = context_project_subject_id.unwrap_or("__none__");
    let agent_scope_keys = subject_memory_subject_ids
        .iter()
        .filter(|subject| subject.starts_with("agent:"))
        .cloned()
        .collect::<Vec<_>>();
    let facts = memory_facts::search_relevant(
        db,
        tenant_id,
        source_id,
        thread.id.as_str(),
        partition,
        agent_scope_keys.as_slice(),
        query_text,
        100,
    )
    .await?;
    let now = chrono::Utc::now();
    let mut blocks = facts
        .into_iter()
        .map(|fact| {
            let scope_score = match fact.scope_kind.as_str() {
                "thread" => 1.0,
                "project" => 0.9,
                "agent" => 0.7,
                _ => 0.0,
            };
            let age_days = now.signed_duration_since(fact.created_at).num_days().max(0) as f64;
            let recency_score = 1.0 / (1.0 + age_days / 30.0);
            let open_item_score = fact
                .searchable_text
                .starts_with("open_items ")
                .then_some(0.25)
                .unwrap_or(0.0);
            let uncertainty_penalty = (fact.status == "uncertain").then_some(0.2).unwrap_or(0.0);
            let total_score =
                fact.lexical_score * 2.0 + scope_score + recency_score + open_item_score
                    - uncertainty_penalty;
            let mut score_components = BTreeMap::new();
            score_components.insert("lexical_match".to_string(), fact.lexical_score);
            score_components.insert("scope_match".to_string(), scope_score);
            score_components.insert("recency".to_string(), recency_score);
            score_components.insert("open_item".to_string(), open_item_score);
            score_components.insert("uncertainty_penalty".to_string(), -uncertainty_penalty);
            score_components.insert("total".to_string(), total_score);
            let mut source_refs = fact.source_refs;
            source_refs.push(fact.output_id.clone());
            source_refs.push(fact.fact_id.clone());
            source_refs.sort();
            source_refs.dedup();
            let project_partition_key = fact
                .project_partition_key
                .unwrap_or_else(|| "__none__".to_string());
            let text = fact.searchable_text;
            ComposeContextBlock {
                block_id: format!(
                    "memory-fact:{}:{}",
                    fact.fact_key, fact.normalized_value_hash
                ),
                block_type: format!("relevant_{}_fact", fact.output_kind),
                pool: "relevant_history".to_string(),
                subject_id: (fact.scope_kind != "thread").then_some(fact.scope_key),
                project_partition_key,
                level: None,
                source_refs,
                estimated_tokens: super::estimate_tokens(text.as_str()).saturating_add(12),
                score_components,
                selection_reason: format!("hybrid_fact_key:{}", fact.fact_key),
                truncated: false,
                text,
            }
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|left, right| {
        right
            .score_components
            .get("total")
            .partial_cmp(&left.score_components.get("total"))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.block_id.cmp(&right.block_id))
    });
    Ok(blocks)
}

fn deduplicate_blocks(blocks: Vec<ComposeContextBlock>) -> (Vec<ComposeContextBlock>, usize) {
    let mut seen_block_ids = std::collections::BTreeSet::new();
    let mut seen_source_frontiers = std::collections::BTreeSet::new();
    let mut seen_content = std::collections::BTreeSet::new();
    let mut selected = Vec::new();
    let mut dropped = 0usize;
    for block in blocks {
        let mut refs = block.source_refs.clone();
        refs.sort();
        let normalized_content = block
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let duplicate = !seen_block_ids.insert(block.block_id.clone())
            || (!refs.is_empty() && !seen_source_frontiers.insert(refs))
            || !seen_content.insert(normalized_content);
        if duplicate {
            dropped += 1;
        } else {
            selected.push(block);
        }
    }
    (selected, dropped)
}

pub(crate) fn thread_agent_subject_id(thread: &EngineThread) -> Option<String> {
    let subject_id = thread.subject_id.trim();
    let labels = thread.labels.as_ref()?;
    let is_v2 = labels
        .iter()
        .any(|label| label.trim() == "memory-schema:v2");
    let has_matching_agent_label = labels.iter().any(|label| label.trim() == subject_id);
    (is_v2
        && subject_memory_scopes::is_canonical_agent_subject_id(subject_id)
        && has_matching_agent_label)
        .then(|| subject_id.to_string())
}

pub(crate) fn thread_identity_subject_ids(thread: &EngineThread) -> Vec<String> {
    let mut subjects = Vec::new();
    let Some(agent) = thread_agent_subject_id(thread) else {
        return subjects;
    };
    subjects.push(agent);
    for label in thread.labels.as_ref().into_iter().flatten() {
        let label = label.trim();
        if label.starts_with("project:")
            && label.len() > "project:".len()
            && !subjects.iter().any(|value| value == label)
        {
            subjects.push(label.to_string());
        }
    }
    subjects
}

pub(crate) fn thread_allows_dynamic_project_recall(thread: &EngineThread) -> bool {
    thread.labels.as_ref().into_iter().flatten().any(|label| {
        matches!(
            label.trim(),
            "thread-kind:main_chat"
                | "thread-kind:agent_conversation"
                | "thread-kind:story_agent"
                | "thread-kind:approval_agent"
                | "thread-kind:agent_team_run"
        )
    })
}

pub(crate) fn build_thread_summary_level0_text(rows: &[EngineSummary]) -> String {
    let mut ordered = rows.to_vec();
    ordered.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    ordered
        .into_iter()
        .map(|item| item.summary_text)
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub(crate) fn build_thread_summary_blocks_from_frontier(
    mut frontier: Vec<EngineSummary>,
) -> (Vec<ComposeContextBlock>, usize) {
    let mut blocks = Vec::new();
    let summary_count = frontier.len();
    frontier.sort_by(|left, right| {
        right
            .level
            .cmp(&left.level)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    for top_summary in frontier.iter().filter(|summary| summary.level > 0) {
        let source_ref = top_summary.id.clone();
        blocks.push(context_block(
            source_ref.clone(),
            "thread_summary_top_level",
            "thread_continuity",
            Some(top_summary.subject_id.clone()),
            "__none__".to_string(),
            Some(top_summary.level),
            vec![source_ref],
            top_summary.summary_text.clone(),
            "canonical_thread_frontier",
        ));
    }

    let level0_rows = frontier
        .into_iter()
        .filter(|summary| summary.level == 0)
        .collect::<Vec<_>>();
    if !level0_rows.is_empty() {
        let source_refs = level0_rows
            .iter()
            .map(|summary| summary.id.clone())
            .collect::<Vec<_>>();
        let block_id = format!("thread-summary-frontier:{}", source_refs.join(":"));
        let subject_id = level0_rows
            .first()
            .map(|summary| summary.subject_id.clone());
        blocks.push(context_block(
            block_id,
            "thread_summary_level0",
            "thread_continuity",
            subject_id,
            "__none__".to_string(),
            Some(0),
            source_refs,
            build_thread_summary_level0_text(level0_rows.as_slice()),
            "canonical_thread_frontier",
        ));
    }

    (blocks, summary_count)
}

fn context_block(
    block_id: String,
    block_type: &str,
    pool: &str,
    subject_id: Option<String>,
    project_partition_key: String,
    level: Option<i64>,
    source_refs: Vec<String>,
    text: String,
    selection_reason: &str,
) -> ComposeContextBlock {
    let estimated_tokens = super::estimate_tokens(text.as_str()).saturating_add(12);
    let mut score_components = BTreeMap::new();
    score_components.insert("scope_match".to_string(), 1.0);
    score_components.insert(
        if pool == "thread_continuity" {
            "thread_continuity"
        } else {
            "durable_frontier"
        }
        .to_string(),
        1.0,
    );
    ComposeContextBlock {
        block_id,
        block_type: block_type.to_string(),
        pool: pool.to_string(),
        subject_id,
        project_partition_key,
        level,
        source_refs,
        estimated_tokens,
        score_components,
        selection_reason: selection_reason.to_string(),
        truncated: false,
        text,
    }
}

pub(crate) fn format_subject_memory(item: EngineSubjectMemory) -> String {
    format!(
        "[subject_id={}][memory_type={}][level={}][memory_key={}]\n{}",
        item.subject_id, item.memory_type, item.level, item.memory_key, item.text
    )
}

async fn load_thread_summary_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_limit: i64,
    isolate_dynamic_project: bool,
    context_project_subject_id: Option<&str>,
) -> Result<(Vec<ComposeContextBlock>, usize), String> {
    // Stable Agent threads may contain several projects. Their existing rollups can combine
    // those projects, so only project-scoped level-0 summaries are safe as Thread context.
    // Cross-project durable knowledge is already supplied by Agent Memory, while the current
    // project's complete rollup is supplied by Project Summary.
    let frontier = if isolate_dynamic_project {
        summaries::list_latest_thread_summaries_at_level_for_project(
            db,
            tenant_id,
            source_id,
            thread_id,
            "thread_incremental",
            0,
            context_project_subject_id,
            summary_limit,
        )
        .await?
    } else {
        summaries::list_latest_thread_summary_frontier(
            db,
            tenant_id,
            source_id,
            thread_id,
            "thread_incremental",
            summary_limit,
        )
        .await?
    };

    let (mut blocks, count) = build_thread_summary_blocks_from_frontier(frontier);
    let partition = context_project_subject_id.unwrap_or("__none__");
    for block in &mut blocks {
        block.project_partition_key = partition.to_string();
    }
    Ok((blocks, count))
}

async fn load_subject_memory_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_ids: &[String],
) -> Result<Vec<ComposeContextBlock>, String> {
    let mut blocks = Vec::new();

    for subject_id in subject_ids {
        let (memory_type, rollup_block_type, recent_block_type) =
            if subject_id.starts_with("agent:") {
                ("agent_memory", "agent_memory_rollup", "agent_memory_recent")
            } else if subject_id.starts_with("project:") {
                (
                    "project_summary",
                    "project_summary_rollup",
                    "project_summary_recent",
                )
            } else {
                continue;
            };
        let required_generation_profile = subject_id
            .starts_with("agent:")
            .then_some(subject_memory_scopes::AGENT_STABLE_MEMORY_PROFILE);

        let rollup = subject_memories::list_subject_memories_for_context(
            db,
            tenant_id,
            source_id,
            subject_id,
            memory_type,
            None,
            None,
            None,
            required_generation_profile,
            1,
        )
        .await?
        .into_iter()
        .find(|memory| memory.level > 0);
        let has_rollup = rollup.is_some();
        if let Some(memory) = rollup {
            let block_id = memory.id.clone();
            let subject = memory.subject_id.clone();
            let level = memory.level;
            blocks.push(context_block(
                block_id.clone(),
                rollup_block_type,
                if subject.starts_with("project:") {
                    "project_state"
                } else {
                    "agent_stable_memory"
                },
                Some(subject.clone()),
                if subject.starts_with("project:") {
                    subject
                } else {
                    "__none__".to_string()
                },
                Some(level),
                vec![block_id],
                format_subject_memory(memory),
                "canonical_subject_frontier",
            ));
        }

        let mut recent = subject_memories::list_subject_memories_for_context(
            db,
            tenant_id,
            source_id,
            subject_id,
            memory_type,
            Some(0),
            Some("pending"),
            Some(subject_id),
            required_generation_profile,
            RECENT_SUBJECT_MEMORY_LIMIT,
        )
        .await?;
        if recent.is_empty() && !has_rollup {
            recent = subject_memories::list_subject_memories_for_context(
                db,
                tenant_id,
                source_id,
                subject_id,
                memory_type,
                Some(0),
                None,
                None,
                required_generation_profile,
                1,
            )
            .await?;
        }
        if !recent.is_empty() {
            let source_refs = recent
                .iter()
                .map(|memory| memory.id.clone())
                .collect::<Vec<_>>();
            let level = recent.first().map(|memory| memory.level);
            let block_id = format!("subject-memory-frontier:{}", source_refs.join(":"));
            blocks.push(context_block(
                block_id,
                recent_block_type,
                if subject_id.starts_with("project:") {
                    "project_state"
                } else {
                    "agent_stable_memory"
                },
                Some(subject_id.clone()),
                if subject_id.starts_with("project:") {
                    subject_id.clone()
                } else {
                    "__none__".to_string()
                },
                level,
                source_refs,
                recent
                    .into_iter()
                    .map(format_subject_memory)
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n"),
                "recent_unrolled_subject_frontier",
            ));
        }
    }

    Ok(blocks)
}
