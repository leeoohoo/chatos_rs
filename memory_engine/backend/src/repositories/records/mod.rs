// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
pub(crate) mod compact_turns;
mod queries;
mod status;
mod writes;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct ListRecordsQuery<'a> {
    pub thread_id: &'a str,
    pub tenant_id: Option<&'a str>,
    pub source_id: Option<&'a str>,
    pub role: Option<&'a str>,
    pub record_type: Option<&'a str>,
    pub summary_status: Option<&'a str>,
    pub after_created_at: Option<&'a str>,
    pub after_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
    pub asc: bool,
}

#[derive(Debug)]
pub struct RecordListCursor<'a> {
    pub created_at: DateTime<Utc>,
    pub id: &'a str,
}

impl<'a> ListRecordsQuery<'a> {
    pub fn cursor(&self) -> Result<Option<RecordListCursor<'a>>, String> {
        match (self.after_created_at, self.after_id) {
            (None, None) => Ok(None),
            (Some(created_at), Some(id)) => {
                let created_at = DateTime::parse_from_rfc3339(created_at.trim())
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|_| "after_created_at must use RFC3339".to_string())?;
                let id = id.trim();
                if id.is_empty() {
                    return Err("after_id must be non-empty".to_string());
                }
                Ok(Some(RecordListCursor { created_at, id }))
            }
            _ => Err("after_created_at and after_id must be provided together".to_string()),
        }
    }
}

pub(crate) use common::estimate_pending_record_tokens;
pub(crate) use queries::list_records_by_ids;
#[allow(unused_imports)]
pub use queries::{
    count_records, get_record_by_id, list_compact_turn_slices, list_context_records,
    list_pending_records, list_records_page, list_turn_process_records,
};
#[allow(unused_imports)]
pub use status::{
    claim_records_for_summary, mark_claimed_records_summarized, mark_records_summarized,
    release_records_from_summary, reset_records_summary_by_summary_id,
};
#[allow(unused_imports)]
pub use writes::{batch_sync_records, delete_record_by_id, delete_records_by_thread};

#[cfg(test)]
mod tests {
    use super::ListRecordsQuery;

    #[test]
    fn record_cursor_requires_a_complete_valid_tuple() {
        let partial = ListRecordsQuery {
            after_created_at: Some("2026-09-18T00:00:00Z"),
            ..ListRecordsQuery::default()
        };
        assert_eq!(
            partial.cursor().unwrap_err(),
            "after_created_at and after_id must be provided together"
        );

        let malformed = ListRecordsQuery {
            after_created_at: Some("not-a-time"),
            after_id: Some("record-1"),
            ..ListRecordsQuery::default()
        };
        assert_eq!(
            malformed.cursor().unwrap_err(),
            "after_created_at must use RFC3339"
        );
    }
}
