use std::collections::BTreeSet;
use std::collections::HashSet;

use sqlx::SqlitePool;

use crate::models::{ComposeContextMeta, ComposeContextRequest, ComposeContextResponse, SessionSummary};
use crate::repositories::{messages, summaries};

const DEFAULT_SUMMARY_LIMIT: usize = 3;
const DEFAULT_KEEP_RAW_LEVEL0_COUNT: usize = 5;
const TOP_SUMMARY_COUNT: usize = 2;
const LEVEL0_SUMMARY_COUNT: usize = 2;

pub async fn compose_context(
    pool: &SqlitePool,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    // Keep request summary_limit only as a scan multiplier for compatibility.
    let summary_limit = req.summary_limit.unwrap_or(DEFAULT_SUMMARY_LIMIT).max(1).min(20);
    let include_raw = req.include_raw_messages.unwrap_or(true);

    let summary_records = summaries::list_summaries(
        pool,
        req.session_id.as_str(),
        None,
        Some("done"),
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

    let merged_summary = if merge_order.is_empty() {
        None
    } else {
        let text = merge_order
            .iter()
            .map(|s| s.summary_text.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        Some(format!(
            "以下是历史会话总结（按时间从旧到新）：\n\n{}",
            text
        ))
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
        summary_count: selected.len(),
        messages,
        meta: ComposeContextMeta {
            used_levels,
            filtered_rollup_count: selected.iter().filter(|s| s.level == 0).count(),
            kept_raw_level0_count: DEFAULT_KEEP_RAW_LEVEL0_COUNT,
        },
    })
}
