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
            "/api/memory-engine/v1/admin/agents",
            get(admin_api::list_agents),
        )
        .route(
            "/api/memory-engine/v1/admin/agents/{agent_subject_id}/projects",
            get(admin_api::list_agent_projects),
        )
        .route(
            "/api/memory-engine/v1/admin/agents/{agent_subject_id}/conversations",
            get(admin_api::list_agent_conversations),
        )
        .route(
            "/api/memory-engine/v1/admin/job-policies",
            get(admin_api::list_job_policies),
        )
        .route(
            "/api/memory-engine/v1/admin/job-policies/{job_type}",
            get(admin_api::get_job_policy),
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
        .route(
            "/api/memory-engine/v1/admin/reliability/migrations",
            get(admin_api::list_migration_runs),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/dashboard",
            get(admin_api::reliability_dashboard),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/rebuild-backlog",
            get(admin_api::list_rebuild_backlog),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/compose-dry-run",
            post(admin_api::compose_dry_run),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/queues",
            get(admin_api::list_reliability_queues),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/operator-actions",
            get(admin_api::list_operator_actions),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/rebuild-backlog/rearm",
            post(admin_api::rearm_rebuild_backlog),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/resummarize",
            post(admin_api::resummarize_covered_range),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/retire-summary",
            post(admin_api::retire_summary),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/restore-pending",
            post(admin_api::restore_record_pending),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/queues/rearm",
            post(admin_api::rearm_reliability_queue),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/records/{record_id}/coverage",
            get(admin_api::record_coverage),
        )
        .route(
            "/api/memory-engine/v1/admin/reliability/outputs/{output_kind}/{output_id}/lineage",
            get(admin_api::output_lineage),
        )
}
