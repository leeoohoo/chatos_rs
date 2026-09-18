// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod dispatch;
mod queries;
mod writes;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct ListThreadsQuery<'a> {
    pub tenant_id: Option<&'a str>,
    pub source_id: Option<&'a str>,
    pub subject_id: Option<&'a str>,
    pub external_thread_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub contact_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub mapping_source: Option<&'a str>,
    pub mapping_version: Option<&'a str>,
    pub thread_label: Option<&'a str>,
    pub status: Option<&'a str>,
    pub before_updated_at: Option<&'a str>,
    pub before_created_at: Option<&'a str>,
    pub before_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug)]
pub struct ThreadListCursor<'a> {
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub id: &'a str,
}

impl<'a> ListThreadsQuery<'a> {
    pub fn cursor(&self) -> Result<Option<ThreadListCursor<'a>>, String> {
        match (
            self.before_updated_at,
            self.before_created_at,
            self.before_id,
        ) {
            (None, None, None) => Ok(None),
            (Some(updated_at), Some(created_at), Some(id)) => {
                let updated_at = parse_cursor_time("before_updated_at", updated_at)?;
                let created_at = parse_cursor_time("before_created_at", created_at)?;
                let id = id.trim();
                if id.is_empty() {
                    return Err("before_id must be non-empty".to_string());
                }
                Ok(Some(ThreadListCursor {
                    updated_at,
                    created_at,
                    id,
                }))
            }
            _ => Err(
                "before_updated_at, before_created_at and before_id must be provided together"
                    .to_string(),
            ),
        }
    }
}

fn parse_cursor_time(field: &str, value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| format!("{field} must use RFC3339"))
}

pub use dispatch::{
    defer_summary_dispatch_until_unlock, get_pending_summary_dispatch, get_summary_dispatch_state,
    list_eligible_summary_dispatches, list_pending_summary_dispatches,
    list_stale_published_summary_dispatches, mark_summary_dispatch_consumed,
    mark_summary_dispatch_dead_lettered, mark_summary_dispatch_failed,
    mark_summary_dispatch_published, rearm_stale_published_summary_dispatch,
    rearm_summary_dispatch_if_eligible, replay_dead_lettered_summary_dispatch,
    SummaryDispatchOutbox,
};
#[allow(unused_imports)]
pub use queries::{
    get_thread, get_thread_by_id, list_threads, list_threads_by_label,
    list_threads_with_pending_records_by_token_threshold,
};
#[allow(unused_imports)]
pub use writes::{
    apply_summary_queue_state_delta, begin_record_sync, delete_thread, finish_record_sync,
    refresh_summary_queue_state, refresh_summary_slot, release_rollup_slot, release_summary_slot,
    try_acquire_rollup_slot, try_acquire_summary_slot, upsert_thread,
};

#[cfg(test)]
mod tests {
    use super::ListThreadsQuery;

    #[test]
    fn list_cursor_requires_a_complete_valid_tuple() {
        let partial = ListThreadsQuery {
            before_updated_at: Some("2026-09-18T00:00:00Z"),
            ..ListThreadsQuery::default()
        };
        assert_eq!(
            partial.cursor().unwrap_err(),
            "before_updated_at, before_created_at and before_id must be provided together"
        );

        let malformed = ListThreadsQuery {
            before_updated_at: Some("not-a-time"),
            before_created_at: Some("2026-09-18T00:00:00Z"),
            before_id: Some("thread-1"),
            ..ListThreadsQuery::default()
        };
        assert_eq!(
            malformed.cursor().unwrap_err(),
            "before_updated_at must use RFC3339"
        );

        let empty_id = ListThreadsQuery {
            before_updated_at: Some("2026-09-18T00:00:00Z"),
            before_created_at: Some("2026-09-18T00:00:00Z"),
            before_id: Some("  "),
            ..ListThreadsQuery::default()
        };
        assert_eq!(empty_id.cursor().unwrap_err(), "before_id must be non-empty");
    }
}
