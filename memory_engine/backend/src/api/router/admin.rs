// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    routing::{get, post, put},
    Router,
};

use crate::api::{admin_api, sources_api};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/memory-engine/v1/admin/job-policies",
            get(admin_api::list_job_policies),
        )
        .route(
            "/api/memory-engine/v1/admin/job-policies/{job_type}",
            get(admin_api::get_job_policy).put(admin_api::upsert_job_policy),
        )
        .route(
            "/api/memory-engine/v1/admin/job-policies/{job_type}/generate-prompt",
            post(admin_api::generate_job_policy_prompt),
        )
        .route(
            "/api/memory-engine/v1/admin/job-runs",
            get(admin_api::list_job_runs),
        )
        .route(
            "/api/memory-engine/v1/admin/dashboard/overview",
            get(admin_api::dashboard_overview),
        )
        .route(
            "/api/memory-engine/v1/admin/job-runs/bundle",
            get(admin_api::job_runs_bundle),
        )
        .route(
            "/api/memory-engine/v1/admin/job-runs/stats",
            get(admin_api::job_run_stats),
        )
        .route(
            "/api/memory-engine/v1/admin/sources",
            get(sources_api::admin_list_sources),
        )
        .route(
            "/api/memory-engine/v1/admin/sources/{source_id}",
            put(sources_api::admin_upsert_source),
        )
        .route(
            "/api/memory-engine/v1/admin/sources/{source_id}/rotate-key",
            post(sources_api::admin_rotate_source_secret),
        )
}

pub fn model_profile_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/memory-engine/v1/admin/model-profiles",
            get(admin_api::list_model_profiles).post(admin_api::create_model_profile),
        )
        .route(
            "/api/memory-engine/v1/admin/model-profiles/{model_id}",
            get(admin_api::get_model_profile)
                .put(admin_api::update_model_profile)
                .delete(admin_api::delete_model_profile),
        )
}
