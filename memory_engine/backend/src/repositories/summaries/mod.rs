// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod dispatch;
mod queries;
mod status;
mod subject_dispatch;
mod writes;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct ListSummariesQuery<'a> {
    pub thread_id: &'a str,
    pub tenant_id: Option<&'a str>,
    pub source_id: Option<&'a str>,
    pub summary_type: Option<&'a str>,
    pub status: Option<&'a str>,
    pub level: Option<i64>,
    pub after_level: Option<i64>,
    pub after_created_at: Option<&'a str>,
    pub after_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug)]
pub struct SummaryListCursor<'a> {
    pub level: i64,
    pub created_at: DateTime<Utc>,
    pub id: &'a str,
}

impl<'a> ListSummariesQuery<'a> {
    pub fn cursor(&self) -> Result<Option<SummaryListCursor<'a>>, String> {
        match (
            self.after_level,
            self.after_created_at,
            self.after_id,
        ) {
            (None, None, None) => Ok(None),
            (Some(level), Some(created_at), Some(id)) => {
                if level < 0 {
                    return Err("after_level must be non-negative".to_string());
                }
                let created_at = DateTime::parse_from_rfc3339(created_at.trim())
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|_| "after_created_at must use RFC3339".to_string())?;
                let id = id.trim();
                if id.is_empty() {
                    return Err("after_id must be non-empty".to_string());
                }
                Ok(Some(SummaryListCursor {
                    level,
                    created_at,
                    id,
                }))
            }
            _ => Err(
                "after_level, after_created_at and after_id must be provided together".to_string(),
            ),
        }
    }
}

pub use dispatch::{
    get_pending_rollup_dispatch, get_rollup_dispatch_state, list_pending_rollup_dispatches,
    mark_rollup_dispatch_consumed, mark_rollup_dispatch_dead_lettered, mark_rollup_dispatch_failed,
    mark_rollup_dispatch_published, rearm_rollup_dispatch_if_eligible,
    replay_dead_lettered_rollup_dispatch, RollupDispatchOutbox,
};
pub use subject_dispatch::{
    get_pending_subject_memory_source_dispatch, get_subject_memory_source_dispatch_state,
    list_pending_subject_memory_source_dispatches, mark_subject_memory_source_dispatch_consumed,
    mark_subject_memory_source_dispatch_dead_lettered, mark_subject_memory_source_dispatch_failed,
    mark_subject_memory_source_dispatch_published,
    replay_dead_lettered_subject_memory_source_dispatch, SubjectMemorySourceDispatchOutbox,
};

#[allow(unused_imports)]
pub use queries::{
    find_summary_by_source_digest, list_latest_thread_summaries,
    list_latest_thread_summaries_at_level, list_latest_thread_summaries_by_type,
    list_pending_summaries_by_level, list_summaries_by_thread_label,
    list_summaries_by_thread_label_for_subject_memory_scope, list_thread_summaries,
    list_threads_with_pending_rollups,
};
#[allow(unused_imports)]
pub use status::{
    mark_summaries_rolled_up, mark_summaries_subject_memory_summarized,
    mark_summaries_subject_memory_summarized_for_scope,
};
#[allow(unused_imports)]
pub use writes::{
    create_rollup_summary, create_thread_summary, create_thread_summary_with_type,
    delete_thread_summary, upsert_thread_summary,
};

#[cfg(test)]
mod tests {
    use super::ListSummariesQuery;

    #[test]
    fn summary_cursor_requires_a_complete_valid_tuple() {
        let partial = ListSummariesQuery {
            after_level: Some(1),
            ..ListSummariesQuery::default()
        };
        assert_eq!(
            partial.cursor().unwrap_err(),
            "after_level, after_created_at and after_id must be provided together"
        );

        let malformed = ListSummariesQuery {
            after_level: Some(1),
            after_created_at: Some("not-a-time"),
            after_id: Some("summary-1"),
            ..ListSummariesQuery::default()
        };
        assert_eq!(
            malformed.cursor().unwrap_err(),
            "after_created_at must use RFC3339"
        );
    }
}
