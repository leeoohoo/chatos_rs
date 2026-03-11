use std::collections::BTreeSet;

use sqlx::SqlitePool;

use crate::models::{ComposeContextMeta, ComposeContextRequest, ComposeContextResponse, SessionSummary};
use crate::repositories::{messages, summaries};

const DEFAULT_SUMMARY_LIMIT: usize = 3;
const DEFAULT_KEEP_RAW_LEVEL0_COUNT: usize = 5;

pub async fn compose_context(
    pool: &SqlitePool,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    let summary_limit = req.summary_limit.unwrap_or(DEFAULT_SUMMARY_LIMIT).max(1).min(20);
    let include_raw = req.include_raw_messages.unwrap_or(true);

    let mut summary_records = summaries::list_summaries(
        pool,
        req.session_id.as_str(),
        None,
        Some("done"),
        Some("pending"),
        (summary_limit as i64).saturating_mul(20),
        0,
    )
    .await?;

    // Prioritize higher level summaries, then newer summaries in same level.
    summary_records.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    let selected: Vec<SessionSummary> = summary_records.into_iter().take(summary_limit).collect();

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
