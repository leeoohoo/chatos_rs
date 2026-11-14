// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod blocks;
mod policy;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::db::Db;
use crate::models::{
    ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse,
    ComposeTokenBudget,
};
use crate::repositories::{memory_facts, records, threads};

use self::blocks::{
    block_token_count, build_context_blocks, count_blocks_by_pool,
    thread_allows_dynamic_project_recall, thread_identity_subject_ids,
};
use self::policy::{ResolvedComposeContextPolicy, CONTEXT_POLICY_REVISION};

pub async fn compose_context(
    db: &Db,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    let thread = threads::get_thread_by_id(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.thread_id.as_str(),
    )
    .await?
    .ok_or_else(|| "thread not found".to_string())?;

    if req.model_capability_revision.trim().is_empty()
        || req.model_capability_revision.chars().any(char::is_control)
    {
        return Err("model_capability_revision is required".to_string());
    }
    let policy = ResolvedComposeContextPolicy::from_request(&req.policy)?;
    memory_facts::verify_lexical_index_ready(db).await?;
    let bound_identity_subject_ids = thread_identity_subject_ids(&thread);
    if bound_identity_subject_ids.is_empty() {
        return Err("thread is not bound to a canonical agent identity".to_string());
    }
    let allow_dynamic_project = thread_allows_dynamic_project_recall(&thread);
    validate_requested_subject(&bound_identity_subject_ids, req.subject_id.as_deref())?;
    let mut subject_memory_subject_ids = bound_identity_subject_ids;
    let context_project_subject_id = req
        .context_project_subject_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(project) = context_project_subject_id {
        validate_requested_project(&subject_memory_subject_ids, allow_dynamic_project, project)?;
        if !subject_memory_subject_ids
            .iter()
            .any(|value| value == project)
        {
            subject_memory_subject_ids.push(project.to_string());
        }
    }
    let context_blocks = build_context_blocks(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        &thread,
        &policy,
        subject_memory_subject_ids.as_slice(),
        context_project_subject_id,
        req.query_text.as_deref(),
    )
    .await?;

    let mut recent_records = if allow_dynamic_project {
        records::list_context_records_for_project(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread.id.as_str(),
            context_project_subject_id,
            None,
        )
        .await?
    } else {
        records::list_context_records(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread.id.as_str(),
            None,
        )
        .await?
    };
    if let Some(current_turn_id) = req
        .current_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        recent_records.retain(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("conversation_turn_id"))
                .and_then(serde_json::Value::as_str)
                != Some(current_turn_id)
        });
    }
    let recent_candidate_count = recent_records.len();
    let oldest_pending_record_at = recent_records
        .first()
        .map(|record| record.created_at.clone());
    let block_tokens = block_token_count(context_blocks.blocks.as_slice());
    let recent_budget = policy
        .max_recent_record_tokens
        .min(policy.max_memory_tokens.saturating_sub(block_tokens));
    let retained_records = retain_newest_records_within_budget(recent_records, recent_budget);
    let recent_records = retained_records.records;

    let mut candidate_count_by_pool = context_blocks.candidate_count_by_pool;
    candidate_count_by_pool.insert("raw_tail".to_string(), recent_candidate_count);
    candidate_count_by_pool
        .entry("relevant_history".to_string())
        .or_insert(0);
    let mut selected_count_by_pool = count_blocks_by_pool(context_blocks.blocks.as_slice());
    selected_count_by_pool.insert("raw_tail".to_string(), recent_records.len());
    selected_count_by_pool
        .entry("relevant_history".to_string())
        .or_insert(0);
    let mut dropped_count_by_reason = context_blocks.dropped_count_by_reason;
    if retained_records.dropped > 0 {
        *dropped_count_by_reason
            .entry("raw_tail_budget".to_string())
            .or_insert(0) += retained_records.dropped;
    }
    let budget_requested = requested_budget(&req.policy);
    let budget_effective = effective_budget(&policy);
    let budget_used = used_budget(
        context_blocks.blocks.as_slice(),
        retained_records.used_tokens,
    );

    Ok(ComposeContextResponse {
        thread_id: thread.id,
        blocks: context_blocks.blocks,
        recent_records: recent_records.clone(),
        meta: ComposeContextMeta {
            summary_count: context_blocks.summary_count,
            recent_record_count: recent_records.len(),
            policy_revision: CONTEXT_POLICY_REVISION.to_string(),
            model_capability_revision: req.model_capability_revision,
            budget_requested,
            budget_effective,
            budget_used,
            estimator_id: memory_engine_sdk::CONSERVATIVE_TOKEN_ESTIMATOR_ID.to_string(),
            tokenizer_id: None,
            candidate_count_by_pool,
            selected_count_by_pool,
            dropped_count_by_reason,
            deduplicated_count: context_blocks.deduplicated_count,
            needs_compaction: retained_records.dropped > 0,
            oldest_pending_record_at,
            project_context_verified: true,
        },
    })
}

struct RetainedRecords {
    records: Vec<crate::models::EngineRecord>,
    used_tokens: usize,
    dropped: usize,
}

fn retain_newest_records_within_budget(
    records: Vec<crate::models::EngineRecord>,
    token_budget: usize,
) -> RetainedRecords {
    let candidate_count = records.len();
    let mut units: Vec<Vec<crate::models::EngineRecord>> = Vec::new();
    for record in records {
        let turn_id = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("conversation_turn_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let joins_previous = turn_id.is_some()
            && units
                .last()
                .and_then(|unit| unit.last())
                .and_then(|previous| {
                    previous
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("conversation_turn_id"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                })
                == turn_id;
        if joins_previous {
            units.last_mut().expect("record unit exists").push(record);
        } else {
            units.push(vec![record]);
        }
    }
    let mut selected = Vec::new();
    let mut used = 0usize;
    for unit in units.into_iter().rev() {
        let estimated = unit.iter().map(estimate_record_tokens).sum::<usize>();
        if used.saturating_add(estimated) > token_budget {
            break;
        }
        used = used.saturating_add(estimated);
        selected.push(unit);
    }
    selected.reverse();
    let selected = selected.into_iter().flatten().collect::<Vec<_>>();
    RetainedRecords {
        dropped: candidate_count.saturating_sub(selected.len()),
        records: selected,
        used_tokens: used,
    }
}

fn estimate_record_tokens(record: &crate::models::EngineRecord) -> usize {
    estimate_tokens(record.content.as_str())
        .saturating_add(
            record
                .structured_payload
                .as_ref()
                .map_or(0, |value| estimate_tokens(value.to_string().as_str())),
        )
        .saturating_add(12)
}

fn requested_budget(policy: &ComposeContextPolicy) -> ComposeTokenBudget {
    ComposeTokenBudget {
        total: policy.max_memory_tokens,
        recent_records: policy.max_recent_record_tokens,
        thread_continuity: policy.max_thread_summary_tokens,
        project_state: policy.max_project_memory_tokens,
        agent_stable_memory: policy.max_agent_memory_tokens,
        relevant_history: 0,
    }
}

fn effective_budget(policy: &ResolvedComposeContextPolicy) -> ComposeTokenBudget {
    ComposeTokenBudget {
        total: policy.max_memory_tokens,
        recent_records: policy.max_recent_record_tokens,
        thread_continuity: policy.max_thread_summary_tokens,
        project_state: policy.max_project_memory_tokens,
        agent_stable_memory: policy.max_agent_memory_tokens,
        relevant_history: policy.max_relevant_history_tokens,
    }
}

fn used_budget(
    blocks: &[crate::models::ComposeContextBlock],
    recent_record_tokens: usize,
) -> ComposeTokenBudget {
    let mut by_pool = BTreeMap::<&str, usize>::new();
    for block in blocks {
        *by_pool.entry(block.pool.as_str()).or_insert(0) += block.estimated_tokens;
    }
    let thread_continuity = by_pool.get("thread_continuity").copied().unwrap_or(0);
    let project_state = by_pool.get("project_state").copied().unwrap_or(0);
    let agent_stable_memory = by_pool.get("agent_stable_memory").copied().unwrap_or(0);
    let relevant_history = by_pool.get("relevant_history").copied().unwrap_or(0);
    ComposeTokenBudget {
        total: recent_record_tokens
            .saturating_add(thread_continuity)
            .saturating_add(project_state)
            .saturating_add(agent_stable_memory)
            .saturating_add(relevant_history),
        recent_records: recent_record_tokens,
        thread_continuity,
        project_state,
        agent_stable_memory,
        relevant_history,
    }
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    memory_engine_sdk::estimate_text_tokens(text).max(1)
}

fn validate_requested_project(
    bound: &[String],
    allow_dynamic: bool,
    project: &str,
) -> Result<(), String> {
    if !project.starts_with("project:") || project.len() == "project:".len() {
        return Err("context project subject identity is invalid".to_string());
    }
    if !allow_dynamic && !bound.iter().any(|subject| subject == project) {
        return Err("thread does not allow dynamic project recall".to_string());
    }
    Ok(())
}

fn validate_requested_subject(bound: &[String], requested: Option<&str>) -> Result<(), String> {
    if let Some(subject) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if !bound.iter().any(|value| value == subject) {
            return Err("requested subject is not bound to this thread".to_string());
        }
    }
    Ok(())
}
