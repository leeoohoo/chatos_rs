// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    routing::{get, post, put},
    Router,
};

use crate::api::{
    context_api, health_api, jobs_api, records_api, sources_api, subject_memories_api,
    subject_memory_scopes_api, subjects_api, summaries_api, thread_snapshots_api, threads_api,
};
use crate::state::AppState;

pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health_api::health))
}

pub fn operator_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/memory-engine/v1/sources/{source_id}",
            put(sources_api::upsert_source),
        )
        .route(
            "/api/memory-engine/v1/jobs/summaries/run-once",
            post(jobs_api::run_pending_summaries_once),
        )
        .route(
            "/api/memory-engine/v1/jobs/rollups/run-once",
            post(jobs_api::run_pending_rollups_once),
        )
        .route(
            "/api/memory-engine/v1/jobs/subject-memories/run-once",
            post(jobs_api::run_subject_memory_job_once),
        )
        .route(
            "/api/memory-engine/v1/jobs/subject-memory-scopes/run-once",
            post(jobs_api::run_subject_memory_scopes_once),
        )
}

pub fn data_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/memory-engine/v1/subjects/{subject_id}",
            put(subjects_api::upsert_subject),
        )
        .route(
            "/api/memory-engine/v1/subject-memory-scopes",
            get(subject_memory_scopes_api::list_subject_memory_scopes),
        )
        .route(
            "/api/memory-engine/v1/subject-memory-scopes/{scope_key}",
            put(subject_memory_scopes_api::upsert_subject_memory_scope),
        )
        .route(
            "/api/memory-engine/v1/subjects/{subject_id}/memories/{memory_key}",
            put(subject_memories_api::upsert_subject_memory),
        )
        .route(
            "/api/memory-engine/v1/subjects/{subject_id}/memories",
            get(subject_memories_api::list_subject_memories),
        )
        .route(
            "/api/memory-engine/v1/subjects/{subject_id}/memories/mark-rolled-up",
            post(subject_memories_api::mark_subject_memories_rolled_up),
        )
        .route(
            "/api/memory-engine/v1/subject-memories/query",
            post(subject_memories_api::query_subject_memories),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}",
            get(threads_api::get_thread)
                .put(threads_api::upsert_thread)
                .delete(threads_api::delete_thread),
        )
        .route(
            "/api/memory-engine/v1/admin/threads/query",
            get(threads_api::list_threads_query),
        )
        .route(
            "/api/memory-engine/v1/threads/query-by-label",
            post(threads_api::list_threads_by_label),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/records",
            get(threads_api::list_records).delete(threads_api::delete_records),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/records/count",
            get(threads_api::count_records),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/compact-turns",
            get(threads_api::list_compact_turns),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/turns/{turn_id}/process-records",
            get(threads_api::get_turn_process_records),
        )
        .route(
            "/api/memory-engine/v1/records/{record_id}",
            get(records_api::get_record).delete(records_api::delete_record),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/records/batch-sync",
            put(threads_api::batch_sync_records),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/snapshots/{snapshot_type}/turns/{turn_id}",
            put(thread_snapshots_api::upsert_thread_snapshot)
                .get(thread_snapshots_api::get_thread_snapshot_by_turn),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/snapshots/{snapshot_type}/latest",
            get(thread_snapshots_api::get_latest_thread_snapshot),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/summaries/run",
            post(summaries_api::run_thread_summary),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/active-summary/run",
            post(summaries_api::run_thread_active_summary),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/active-summary/status",
            get(summaries_api::get_thread_active_summary_status),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/summaries",
            get(summaries_api::list_thread_summaries),
        )
        .route(
            "/api/memory-engine/v1/summaries/query-by-thread-label",
            post(summaries_api::list_summaries_by_thread_label),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/summaries/{summary_id}",
            put(summaries_api::upsert_thread_summary).delete(summaries_api::delete_thread_summary),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/summaries/mark-subject-memory",
            post(summaries_api::mark_subject_memory_summarized),
        )
        .route(
            "/api/memory-engine/v1/threads/{thread_id}/repair-summaries/run",
            post(summaries_api::run_thread_repair_summary),
        )
        .route(
            "/api/memory-engine/v1/context/compose",
            post(context_api::compose_context),
        )
}
