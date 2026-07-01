// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod queries;
mod writes;

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
    pub limit: i64,
    pub offset: i64,
}

#[allow(unused_imports)]
pub use queries::{
    get_thread, get_thread_by_id, list_threads, list_threads_by_label,
    list_threads_with_pending_records_by_token_threshold,
};
#[allow(unused_imports)]
pub use writes::{
    apply_summary_queue_state_delta, delete_thread, refresh_summary_queue_state,
    release_summary_slot, try_acquire_summary_slot, upsert_thread,
};
