// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;

mod auth;
mod context;
mod jobs;
mod records;
mod requests;
mod snapshots;
mod subject_memories;
mod summaries;
mod threads;

pub use auth::auth_status;
pub use context::compose_context;
pub use jobs::{
    run_pending_rollups_once, run_pending_summaries_once, run_subject_memory_scopes_once,
};
pub use records::{
    batch_sync_records, count_thread_records, delete_record, delete_thread_records, get_record,
    get_turn_process_records, list_compact_turns, list_thread_records,
};
pub use snapshots::{
    get_latest_thread_snapshot, get_thread_snapshot_by_turn, upsert_thread_snapshot,
};
pub use subject_memories::{
    list_summaries_by_thread_label, query_subject_memories, upsert_subject_memory_scope,
};
pub use summaries::{
    delete_thread_summary, get_thread_active_summary_status, list_thread_summaries,
    run_thread_active_summary, run_thread_repair_summary, run_thread_summary,
};
pub use threads::{delete_thread, get_thread, list_threads, upsert_thread};

pub(crate) fn internal_error(message: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message)
}
