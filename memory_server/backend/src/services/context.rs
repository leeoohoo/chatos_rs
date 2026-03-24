use std::collections::BTreeSet;
use std::collections::HashSet;

use crate::db::Db;
use crate::models::{
    AgentRecall, ComposeContextMeta, ComposeContextRequest, ComposeContextResponse, SessionSummary,
};
use crate::repositories::{memories, messages, sessions, summaries};

const DEFAULT_SUMMARY_LIMIT: usize = 3;
const DEFAULT_KEEP_RAW_LEVEL0_COUNT: usize = 5;
const TOP_SUMMARY_COUNT: usize = 2;
const LEVEL0_SUMMARY_COUNT: usize = 2;
const DEFAULT_AGENT_MEMORY_LATEST_COUNT: usize = 1;
const DEFAULT_AGENT_MEMORY_TOP_LEVEL_COUNT: usize = 1;
const AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL: &str = "latest_plus_highest_level";

pub async fn compose_context(
    pool: &Db,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    // Keep request summary_limit only as a scan multiplier for compatibility.
    let summary_limit = req
        .summary_limit
        .unwrap_or(DEFAULT_SUMMARY_LIMIT)
        .max(1)
        .min(20);
    let include_raw = req.include_raw_messages.unwrap_or(true);

    let summary_records = summaries::list_summaries(
        pool,
        req.session_id.as_str(),
        None,
        Some("pending"),
        (summary_limit as i64).saturating_mul(20),
        0,
    )
    .await?;

    // Rule:
    // 1) top_part: highest 2 summaries by (level desc, created_at desc)
    // 2) level0_part: latest 2 summaries from level=0
    // 3) merge + dedupe by summary id
    let mut by_level_desc = summary_records.clone();
    by_level_desc.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    let top_part: Vec<SessionSummary> = by_level_desc.into_iter().take(TOP_SUMMARY_COUNT).collect();

    let mut level0_records: Vec<SessionSummary> = summary_records
        .into_iter()
        .filter(|s| s.level == 0)
        .collect();
    level0_records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let level0_part: Vec<SessionSummary> = level0_records
        .into_iter()
        .take(LEVEL0_SUMMARY_COUNT)
        .collect();

    let mut selected: Vec<SessionSummary> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for item in top_part.into_iter().chain(level0_part.into_iter()) {
        if seen_ids.insert(item.id.clone()) {
            selected.push(item);
        }
    }

    let mut merge_order = selected.clone();
    merge_order.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let mut summary_sections: Vec<String> = Vec::new();
    if !merge_order.is_empty() {
        let text = merge_order
            .iter()
            .map(|s| s.summary_text.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        summary_sections.push(format!(
            "以下是历史会话总结（按时间从旧到新）：\n\n{}",
            text
        ));
    }

    let mut summary_count = selected.len();
    if let Ok((agent_memory_section, agent_memory_count)) =
        compose_agent_memory_section(pool, req.session_id.as_str()).await
    {
        if let Some(agent_memory_section) = agent_memory_section {
            summary_sections.push(agent_memory_section);
        }
        summary_count += agent_memory_count;
    }

    let merged_summary = if summary_sections.is_empty() {
        None
    } else {
        Some(summary_sections.join("\n\n===\n\n"))
    };

    let pending_limit = req.pending_limit.map(|v| v as i64).filter(|v| *v > 0);
    let messages = if include_raw {
        messages::list_pending_messages(pool, req.session_id.as_str(), pending_limit).await?
    } else {
        Vec::new()
    };

    let used_levels_set: BTreeSet<i64> = selected.iter().map(|s| s.level).collect();
    let used_levels: Vec<i64> = used_levels_set.into_iter().rev().collect();

    Ok(ComposeContextResponse {
        session_id: req.session_id,
        merged_summary,
        summary_count,
        messages,
        meta: ComposeContextMeta {
            used_levels,
            filtered_rollup_count: selected.iter().filter(|s| s.level == 0).count(),
            kept_raw_level0_count: DEFAULT_KEEP_RAW_LEVEL0_COUNT,
        },
    })
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_string(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalized_text(cursor.as_str())
}

fn agent_id_from_session_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_agent_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactAgentId"]))
}

fn normalize_pick_mode(mode: &str) -> &str {
    if mode
        .trim()
        .eq_ignore_ascii_case(AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL)
    {
        AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL
    } else {
        AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL
    }
}

fn select_agent_memories(
    recalls: &[AgentRecall],
    pick_mode: &str,
    latest_count: usize,
    top_level_count: usize,
) -> Vec<AgentRecall> {
    if recalls.is_empty() {
        return Vec::new();
    }

    let mut selected: Vec<AgentRecall> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    if pick_mode == AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL {
        let mut by_updated = recalls.to_vec();
        by_updated.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        for item in by_updated.into_iter().take(latest_count.max(1)) {
            if seen_ids.insert(item.id.clone()) {
                selected.push(item);
            }
        }

        let mut by_level = recalls.to_vec();
        by_level.sort_by(|a, b| {
            b.level
                .cmp(&a.level)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        for item in by_level.into_iter().take(top_level_count.max(1)) {
            if seen_ids.insert(item.id.clone()) {
                selected.push(item);
            }
        }
    }

    selected.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    selected
}

async fn compose_agent_memory_section(
    pool: &Db,
    session_id: &str,
) -> Result<(Option<String>, usize), String> {
    let Some(session) = sessions::get_session_by_id(pool, session_id).await? else {
        return Ok((None, 0));
    };

    let user_id = session.user_id.trim().to_string();
    if user_id.is_empty() {
        return Ok((None, 0));
    }

    let metadata = session.metadata.as_ref();
    let Some(agent_id) = agent_id_from_session_metadata(metadata) else {
        return Ok((None, 0));
    };

    let recalls =
        memories::list_agent_recalls(pool, user_id.as_str(), agent_id.as_str(), 200, 0).await?;
    if recalls.is_empty() {
        return Ok((None, 0));
    }

    let pick_mode = normalize_pick_mode(AGENT_MEMORY_PICK_MODE_LATEST_PLUS_HIGHEST_LEVEL);
    let latest_count = DEFAULT_AGENT_MEMORY_LATEST_COUNT;
    let top_level_count = DEFAULT_AGENT_MEMORY_TOP_LEVEL_COUNT;
    let selected =
        select_agent_memories(recalls.as_slice(), pick_mode, latest_count, top_level_count);
    if selected.is_empty() {
        return Ok((None, 0));
    }

    let text = selected
        .iter()
        .map(|item| {
            format!(
                "[level={}][updated_at={}][key={}]\n{}",
                item.level, item.updated_at, item.recall_key, item.recall_text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let section = format!(
        "以下是该智能体的自身记忆（默认策略：最近{}条 + 最高level{}条）：\n\n{}",
        latest_count, top_level_count, text
    );

    Ok((Some(section), selected.len()))
}
