use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

mod admin_api;
mod context_api;
mod health_api;
mod jobs_api;
mod memory_auth;
mod model_profile_auth;
mod operator_auth;
#[cfg(test)]
mod operator_auth_tests;
mod records_api;
mod router;
mod sdk_api;
mod source_guard;
mod sources_api;
mod subject_memories_api;
mod subject_memory_scopes_api;
mod subjects_api;
mod summaries_api;
mod thread_snapshots_api;
mod threads_api;

pub fn router(state: Arc<AppState>) -> Router {
    router::build_router(state)
}
