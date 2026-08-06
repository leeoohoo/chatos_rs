// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

mod admin_api;
mod context_api;
mod health_api;
mod internal_audit;
mod internal_auth;
mod jobs_api;
mod memory_auth;
mod model_profile_auth;
mod operator_auth;
mod queue_operations_api;
mod records_api;
mod router;
mod sdk_api;
mod source_guard;
mod sources_api;
mod subject_memories_api;
mod subject_memory_scopes_api;
mod subjects_api;
mod summaries_api;
mod system_api;
mod thread_snapshots_api;
mod threads_api;

pub fn build_public_router(state: Arc<AppState>) -> Router {
    router::build_public_router(state)
}

pub fn build_internal_router(state: Arc<AppState>) -> Router {
    router::build_internal_router(state)
}
