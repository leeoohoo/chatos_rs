// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde_json::json;
use tokio::try_join;

use super::error::internal_error;
use super::queries::{JobRunStatsQuery, JobRunsQuery};
use crate::api::memory_auth::MemoryAuthContext;
use crate::models::{DashboardOverviewResponse, EngineJobRun, JobRunsBundleResponse};
use crate::repositories::{control_plane, sources};
use crate::state::AppState;

const THREAD_DIRECT_TRIGGER: &str = "thread_direct";
const SUBJECT_DIRECT_TRIGGER: &str = "subject_direct";
const SCHEDULER_TRIGGER: &str = "scheduler";
const DATABASE_WORKER_TRIGGER: &str = "database_worker";
const QUEUE_TRIGGER: &str = "queue";

pub async fn list_job_runs(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(q): Query<JobRunsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(q.tenant_id.as_deref())?;
    control_plane::list_job_runs(
        &state.pool,
        q.job_type.as_deref(),
        q.trigger_type.as_deref(),
        q.thread_id.as_deref(),
        q.status.as_deref(),
        tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await
    .map(|items| Json(json!({ "items": items })))
    .map_err(internal_error)
}

pub async fn job_run_stats(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(q): Query<JobRunStatsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(q.tenant_id.as_deref())?;
    control_plane::job_run_stats(
        &state.pool,
        q.job_type.as_deref(),
        tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.since_hours.unwrap_or(24),
    )
    .await
    .map(|stats| Json(json!({ "stats": stats })))
    .map_err(internal_error)
}

pub async fn job_runs_bundle(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(q): Query<JobRunsQuery>,
) -> Result<Json<JobRunsBundleResponse>, (axum::http::StatusCode, String)> {
    let limit = q.limit.unwrap_or(200);
    let tenant_id = auth.resolve_tenant_scope(q.tenant_id.as_deref())?;
    let (thread_triggers, scheduler_triggers) =
        bundle_trigger_filters(q.job_type.as_deref(), q.trigger_type.as_deref());
    let (thread_runs, scheduler_runs) = try_join!(
        list_job_run_bucket(
            &state.pool,
            &q,
            tenant_id.as_deref(),
            thread_triggers.as_slice(),
            limit
        ),
        list_job_run_bucket(
            &state.pool,
            &q,
            tenant_id.as_deref(),
            scheduler_triggers.as_slice(),
            limit
        ),
    )
    .map_err(internal_error)?;

    Ok(Json(JobRunsBundleResponse {
        thread_runs,
        scheduler_runs,
    }))
}

async fn list_job_run_bucket(
    db: &crate::db::Db,
    query: &JobRunsQuery,
    tenant_id: Option<&str>,
    trigger_types: &[String],
    limit: i64,
) -> Result<Vec<EngineJobRun>, String> {
    if trigger_types.is_empty() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for trigger_type in trigger_types {
        let mut chunk = control_plane::list_job_runs(
            db,
            query.job_type.as_deref(),
            Some(trigger_type.as_str()),
            query.thread_id.as_deref(),
            query.status.as_deref(),
            tenant_id,
            query.source_id.as_deref(),
            limit,
        )
        .await?;
        items.append(&mut chunk);
    }

    items.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    items.truncate(limit.clamp(1, 1000) as usize);
    Ok(items)
}

pub async fn dashboard_overview(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
) -> Result<Json<DashboardOverviewResponse>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(None)?;
    let source_count = match tenant_id.as_deref() {
        Some(tenant_id) => sources::count_sources_for_tenant(&state.pool, tenant_id).await,
        None => sources::count_sources(&state.pool).await,
    }
    .map_err(internal_error)?;
    let policy_count = if auth.is_super_admin_or_operator() {
        control_plane::count_job_policies(&state.pool)
            .await
            .map_err(internal_error)?
    } else {
        0
    };
    let job_stats = control_plane::job_run_stats(&state.pool, None, tenant_id.as_deref(), None, 24)
        .await
        .map_err(internal_error)?;

    Ok(Json(DashboardOverviewResponse {
        source_count,
        policy_count,
        job_stats,
    }))
}

fn bundle_trigger_filters(
    _job_type: Option<&str>,
    requested: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    match requested.map(str::trim).filter(|value| !value.is_empty()) {
        None => (
            vec![
                THREAD_DIRECT_TRIGGER.to_string(),
                SUBJECT_DIRECT_TRIGGER.to_string(),
            ],
            vec![
                SCHEDULER_TRIGGER.to_string(),
                DATABASE_WORKER_TRIGGER.to_string(),
                QUEUE_TRIGGER.to_string(),
            ],
        ),
        Some(SCHEDULER_TRIGGER) => (
            Vec::new(),
            vec![
                SCHEDULER_TRIGGER.to_string(),
                DATABASE_WORKER_TRIGGER.to_string(),
                QUEUE_TRIGGER.to_string(),
            ],
        ),
        Some(other @ (DATABASE_WORKER_TRIGGER | QUEUE_TRIGGER)) => {
            (Vec::new(), vec![other.to_string()])
        }
        Some(other) => (vec![other.to_string()], Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bundle_trigger_filters, DATABASE_WORKER_TRIGGER, QUEUE_TRIGGER, SCHEDULER_TRIGGER,
        SUBJECT_DIRECT_TRIGGER, THREAD_DIRECT_TRIGGER,
    };

    fn automatic_triggers() -> Vec<String> {
        vec![
            SCHEDULER_TRIGGER.to_string(),
            DATABASE_WORKER_TRIGGER.to_string(),
            QUEUE_TRIGGER.to_string(),
        ]
    }

    #[test]
    fn bundle_trigger_filters_defaults_to_thread_and_scheduler_buckets() {
        assert_eq!(
            bundle_trigger_filters(None, None),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                automatic_triggers()
            )
        );
        assert_eq!(
            bundle_trigger_filters(None, Some("   ")),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                automatic_triggers()
            )
        );
    }

    #[test]
    fn bundle_trigger_filters_includes_subject_direct_by_default() {
        let (direct, scheduler) = bundle_trigger_filters(None, None);

        assert!(direct.iter().any(|item| item == THREAD_DIRECT_TRIGGER));
        assert!(direct.iter().any(|item| item == SUBJECT_DIRECT_TRIGGER));
        assert_eq!(scheduler, automatic_triggers());
    }

    #[test]
    fn bundle_trigger_filters_respects_scheduler_only_requests() {
        assert_eq!(
            bundle_trigger_filters(None, Some("scheduler")),
            (Vec::new(), automatic_triggers())
        );
    }

    #[test]
    fn bundle_trigger_filters_routes_other_triggers_into_direct_bucket() {
        assert_eq!(
            bundle_trigger_filters(None, Some("thread_direct")),
            (vec![THREAD_DIRECT_TRIGGER.to_string()], Vec::new())
        );
        assert_eq!(
            bundle_trigger_filters(None, Some("subject_direct")),
            (vec![SUBJECT_DIRECT_TRIGGER.to_string()], Vec::new())
        );
    }

    #[test]
    fn bundle_trigger_filters_keeps_direct_and_automatic_summary_jobs_visible() {
        assert_eq!(
            bundle_trigger_filters(Some("summary"), None),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                automatic_triggers()
            )
        );
        assert_eq!(
            bundle_trigger_filters(Some("rollup"), None),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                automatic_triggers()
            )
        );
    }

    #[test]
    fn bundle_trigger_filters_respects_exact_direct_filter_for_summary_jobs() {
        assert_eq!(
            bundle_trigger_filters(Some("summary"), Some("thread_direct")),
            (vec![THREAD_DIRECT_TRIGGER.to_string()], Vec::new())
        );
        assert_eq!(
            bundle_trigger_filters(Some("rollup"), Some("database_worker")),
            (Vec::new(), vec![DATABASE_WORKER_TRIGGER.to_string()])
        );
    }
}
