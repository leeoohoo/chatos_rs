// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::api::sdk_api;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/memory-engine/v1/sdk/auth/status",
            get(sdk_api::auth_status),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/query",
            post(sdk_api::list_threads),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}",
            put(sdk_api::upsert_thread)
                .post(sdk_api::get_thread)
                .delete(sdk_api::delete_thread),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/records/batch-sync",
            put(sdk_api::batch_sync_records),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/records",
            post(sdk_api::list_thread_records).delete(sdk_api::delete_thread_records),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/compact-turns",
            post(sdk_api::list_compact_turns),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/turns/{turn_id}/process-records",
            post(sdk_api::get_turn_process_records),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/records/count",
            post(sdk_api::count_thread_records),
        )
        .route(
            "/api/memory-engine/v1/sdk/records/{record_id}",
            post(sdk_api::get_record).delete(sdk_api::delete_record),
        )
        .route(
            "/api/memory-engine/v1/sdk/context/compose",
            post(sdk_api::compose_context),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/snapshots/{snapshot_type}/turns/{turn_id}",
            put(sdk_api::upsert_thread_snapshot).post(sdk_api::get_thread_snapshot_by_turn),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/snapshots/{snapshot_type}/latest",
            post(sdk_api::get_latest_thread_snapshot),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/summaries",
            post(sdk_api::list_thread_summaries),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/summaries/{summary_id}",
            delete(sdk_api::delete_thread_summary),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/summaries/run",
            post(sdk_api::run_thread_summary),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/active-summary/run",
            post(sdk_api::run_thread_active_summary),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/active-summary/status",
            post(sdk_api::get_thread_active_summary_status),
        )
        .route(
            "/api/memory-engine/v1/sdk/threads/{thread_id}/repair-summaries/run",
            post(sdk_api::run_thread_repair_summary),
        )
        .route(
            "/api/memory-engine/v1/sdk/subject-memory-scopes/{scope_key}",
            put(sdk_api::upsert_subject_memory_scope),
        )
        .route(
            "/api/memory-engine/v1/sdk/subject-memories/query",
            post(sdk_api::query_subject_memories),
        )
        .route(
            "/api/memory-engine/v1/sdk/summaries/query-by-thread-label",
            post(sdk_api::list_summaries_by_thread_label),
        )
        .route(
            "/api/memory-engine/v1/sdk/jobs/summaries/run-once",
            post(sdk_api::run_pending_summaries_once),
        )
        .route(
            "/api/memory-engine/v1/sdk/jobs/rollups/run-once",
            post(sdk_api::run_pending_rollups_once),
        )
        .route(
            "/api/memory-engine/v1/sdk/jobs/subject-memory-scopes/run-once",
            post(sdk_api::run_subject_memory_scopes_once),
        )
}
