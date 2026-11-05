// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{ComposeContextBlock, EngineSubjectMemory, EngineSummary, EngineThread};
use crate::repositories::{subject_memories, subject_memory_scopes, summaries};

use super::policy::ResolvedComposeContextPolicy;

const RECENT_SUBJECT_MEMORY_LIMIT: i64 = 3;

pub(crate) struct BuiltContextBlocks {
    pub(crate) blocks: Vec<ComposeContextBlock>,
    pub(crate) summary_count: usize,
}

pub(crate) async fn build_context_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread: &EngineThread,
    policy: &ResolvedComposeContextPolicy,
    subject_memory_subject_ids: &[String],
    context_project_subject_id: Option<&str>,
) -> Result<BuiltContextBlocks, String> {
    let mut blocks = Vec::new();
    let mut summary_count = 0usize;

    if policy.include_thread_summary {
        let summary_blocks = load_thread_summary_blocks(
            db,
            tenant_id,
            source_id,
            thread.id.as_str(),
            policy.summary_limit,
            thread_allows_dynamic_project_recall(thread),
            context_project_subject_id,
        )
        .await?;
        summary_count = summary_blocks.1;
        blocks.extend(summary_blocks.0);
    }

    if policy.include_subject_memory {
        blocks.extend(
            load_subject_memory_blocks(db, tenant_id, source_id, subject_memory_subject_ids)
                .await?,
        );
    }

    Ok(BuiltContextBlocks {
        blocks,
        summary_count,
    })
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

pub(crate) fn build_thread_summary_blocks_from_rows(
    level0_rows: Vec<EngineSummary>,
    top_summary: Option<EngineSummary>,
) -> (Vec<ComposeContextBlock>, usize) {
    let mut blocks = Vec::new();
    let mut summary_count = 0usize;

    if let Some(top_summary) = top_summary.filter(|item| item.level > 0) {
        summary_count += 1;
        blocks.push(ComposeContextBlock {
            block_type: "thread_summary_top_level".to_string(),
            text: top_summary.summary_text,
        });
    }

    if !level0_rows.is_empty() {
        summary_count += level0_rows.len();
        blocks.push(ComposeContextBlock {
            block_type: "thread_summary_level0".to_string(),
            text: build_thread_summary_level0_text(level0_rows.as_slice()),
        });
    }

    (blocks, summary_count)
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
    let level0_rows = if isolate_dynamic_project {
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
        summaries::list_latest_thread_summaries_at_level(
            db,
            tenant_id,
            source_id,
            thread_id,
            "thread_incremental",
            0,
            summary_limit,
        )
        .await?
    };
    // Stable Agent threads may contain several projects. Their existing rollups can combine
    // those projects, so only project-scoped level-0 summaries are safe as Thread context.
    // Cross-project durable knowledge is already supplied by Agent Memory, while the current
    // project's complete rollup is supplied by Project Summary.
    let top_summary = if isolate_dynamic_project {
        None
    } else {
        summaries::list_latest_thread_summaries(db, tenant_id, source_id, thread_id, 1)
            .await?
            .into_iter()
            .next()
    };

    Ok(build_thread_summary_blocks_from_rows(
        level0_rows,
        top_summary,
    ))
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

        let rollup = subject_memories::list_subject_memories(
            db,
            tenant_id,
            source_id,
            subject_id,
            Some(memory_type),
            None,
            1,
            0,
        )
        .await?
        .into_iter()
        .find(|memory| memory.level > 0);
        let has_rollup = rollup.is_some();
        if let Some(memory) = rollup {
            blocks.push(ComposeContextBlock {
                block_type: rollup_block_type.to_string(),
                text: format_subject_memory(memory),
            });
        }

        let mut recent = subject_memories::query_subject_memories(
            db,
            tenant_id,
            source_id,
            subject_id,
            Some(memory_type),
            Some(0),
            None,
            Some("pending"),
            Some(subject_id),
            None,
            RECENT_SUBJECT_MEMORY_LIMIT,
            0,
        )
        .await?;
        if recent.is_empty() && !has_rollup {
            recent = subject_memories::list_subject_memories(
                db,
                tenant_id,
                source_id,
                subject_id,
                Some(memory_type),
                Some(0),
                1,
                0,
            )
            .await?;
        }
        if !recent.is_empty() {
            blocks.push(ComposeContextBlock {
                block_type: recent_block_type.to_string(),
                text: recent
                    .into_iter()
                    .map(format_subject_memory)
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n"),
            });
        }
    }

    Ok(blocks)
}
