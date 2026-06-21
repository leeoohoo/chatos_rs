use serde_json::Value;

use crate::db::Db;
use crate::models::{ComposeContextBlock, EngineSubjectMemory, EngineSummary, EngineThread};
use crate::repositories::{subject_memories, summaries};

use super::policy::ResolvedComposeContextPolicy;

const FIXED_SUBJECT_MEMORY_LIMIT: i64 = 1;

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
    summary_subject_ids: &[String],
    subject_memory_subject_ids: &[String],
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
        )
        .await?;
        summary_count = summary_blocks.1;
        blocks.extend(summary_blocks.0);
    }

    if policy.include_subject_memory {
        let _ = summary_subject_ids;
        blocks.extend(
            load_subject_memory_blocks(db, tenant_id, source_id, subject_memory_subject_ids).await?,
        );
    }

    Ok(BuiltContextBlocks {
        blocks,
        summary_count,
    })
}

pub(crate) fn subject_ids_for_context(
    primary_subject_id: &str,
    related_subject_ids: Option<&Vec<String>>,
) -> Vec<String> {
    let mut subject_ids = Vec::new();
    let normalized_primary = primary_subject_id.trim();
    if !normalized_primary.is_empty() {
        subject_ids.push(normalized_primary.to_string());
    }
    if let Some(extra_ids) = related_subject_ids {
        for subject_id in extra_ids {
            let normalized = subject_id.trim();
            if normalized.is_empty() {
                continue;
            }
            if !subject_ids.iter().any(|item| item == normalized) {
                subject_ids.push(normalized.to_string());
            }
        }
    }
    subject_ids
}

pub(crate) fn subject_memory_subject_ids_for_context(
    thread: &EngineThread,
    requested_subject_id: Option<&str>,
    related_subject_ids: Option<&Vec<String>>,
) -> Vec<String> {
    let thread_subject_id = thread.subject_id.trim();
    let agent_subject_id = thread_agent_subject_id(thread);
    let requested = requested_subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let primary_subject_id = agent_subject_id
        .as_deref()
        .or(requested)
        .unwrap_or(thread_subject_id);

    let mut subject_ids = subject_ids_for_context(primary_subject_id, related_subject_ids);
    if let Some(value) = requested.filter(|value| {
        *value != thread_subject_id && !value.starts_with("session:")
    }) {
        if !subject_ids.iter().any(|item| item == value) {
            subject_ids.push(value.to_string());
        }
    }
    if agent_subject_id.is_some() {
        subject_ids.retain(|value| {
            let normalized = value.trim();
            !normalized.is_empty()
                && normalized != thread_subject_id
                && !normalized.starts_with("session:")
        });
        if subject_ids.is_empty() {
            subject_ids.push(primary_subject_id.to_string());
        }
    }

    subject_ids
}

fn text_value(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|item| item.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn thread_agent_subject_id(thread: &EngineThread) -> Option<String> {
    let subject_id = thread.subject_id.trim();
    if subject_id.starts_with("agent:") {
        return Some(subject_id.to_string());
    }

    if let Some(label) = thread
        .labels
        .as_ref()
        .into_iter()
        .flatten()
        .map(|label| label.trim())
        .find(|label| label.starts_with("agent:") && !label.is_empty())
    {
        return Some(label.to_string());
    }

    let metadata = thread.metadata.as_ref();
    let legacy_mapping = metadata.and_then(|value| value.get("legacy_session_mapping"));
    let agent_id = text_value(legacy_mapping, "agent_id").or_else(|| text_value(metadata, "agent_id"));
    agent_id.map(|agent_id| format!("agent:{agent_id}"))
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
) -> Result<(Vec<ComposeContextBlock>, usize), String> {
    let mut blocks = Vec::new();
    let mut summary_count = 0usize;

    let level0_rows = summaries::list_latest_thread_summaries_at_level(
        db,
        tenant_id,
        source_id,
        thread_id,
        "thread_incremental",
        0,
        summary_limit,
    )
    .await?;
    if !level0_rows.is_empty() {
        blocks.push(ComposeContextBlock {
            block_type: "thread_summary_level0".to_string(),
            text: build_thread_summary_level0_text(level0_rows.as_slice()),
        });
        summary_count += level0_rows.len();
    }

    if let Some(top_summary) =
        summaries::list_latest_thread_summaries(db, tenant_id, source_id, thread_id, 1)
            .await?
            .into_iter()
            .next()
    {
        if top_summary.level > 0 {
            summary_count += 1;
            blocks.push(ComposeContextBlock {
                block_type: "thread_summary_top_level".to_string(),
                text: top_summary.summary_text,
            });
        }
    }

    Ok((blocks, summary_count))
}

async fn load_subject_memory_blocks(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_ids: &[String],
) -> Result<Vec<ComposeContextBlock>, String> {
    let mut blocks = Vec::new();

    if let Some(level0_memory) = subject_memories::list_subject_memories_by_subject_ids(
        db,
        tenant_id,
        source_id,
        subject_ids,
        Some(0),
        FIXED_SUBJECT_MEMORY_LIMIT,
    )
    .await?
    .into_iter()
    .next()
    {
        blocks.push(ComposeContextBlock {
            block_type: "subject_memory_level0".to_string(),
            text: format_subject_memory(level0_memory),
        });
    }

    if let Some(top_memory) = subject_memories::list_subject_memories_by_subject_ids(
        db,
        tenant_id,
        source_id,
        subject_ids,
        None,
        1,
    )
    .await?
    .into_iter()
    .next()
    {
        if top_memory.level > 0 {
            blocks.push(ComposeContextBlock {
                block_type: "subject_memory_top_level".to_string(),
                text: format_subject_memory(top_memory),
            });
        }
    }

    Ok(blocks)
}
