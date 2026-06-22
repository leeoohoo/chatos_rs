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
const JOB_TYPE_SUMMARY: &str = "summary";
const JOB_TYPE_ROLLUP: &str = "rollup";

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
    items.truncate(limit.max(1).min(1000) as usize);
    Ok(items)
}

pub async fn dashboard_overview(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
) -> Result<Json<DashboardOverviewResponse>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(None)?;
    let owner_user_id = auth.resolve_owner_scope(None)?;
    let source_count = match tenant_id.as_deref() {
        Some(tenant_id) => {
            sources::list_sources(&state.pool, Some(tenant_id), None, None, None, 10_000, 0)
                .await
                .map(|items| items.len() as i64)
        }
        None => sources::count_sources(&state.pool).await,
    }
    .map_err(internal_error)?;
    let model_count = match owner_user_id.as_deref() {
        Some(owner_user_id) => {
            control_plane::list_model_profiles_by_owner(&state.pool, owner_user_id)
                .await
                .map(|items| items.len() as i64)
        }
        None => control_plane::count_model_profiles(&state.pool).await,
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
        model_count,
        policy_count,
        job_stats,
    }))
}

fn bundle_trigger_filters(
    job_type: Option<&str>,
    requested: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let normalized_job_type = job_type.map(str::trim).filter(|value| !value.is_empty());
    let scheduler_only_job = matches!(
        normalized_job_type,
        Some(JOB_TYPE_SUMMARY | JOB_TYPE_ROLLUP)
    );

    match requested.map(str::trim).filter(|value| !value.is_empty()) {
        None if scheduler_only_job => (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()]),
        None => (
            vec![
                THREAD_DIRECT_TRIGGER.to_string(),
                SUBJECT_DIRECT_TRIGGER.to_string(),
            ],
            vec![SCHEDULER_TRIGGER.to_string()],
        ),
        Some(SCHEDULER_TRIGGER) => (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()]),
        Some(THREAD_DIRECT_TRIGGER) if scheduler_only_job => {
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
        }
        Some(other) => (vec![other.to_string()], Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bundle_trigger_filters, JOB_TYPE_ROLLUP, JOB_TYPE_SUMMARY, SCHEDULER_TRIGGER,
        SUBJECT_DIRECT_TRIGGER, THREAD_DIRECT_TRIGGER,
    };

    #[test]
    fn bundle_trigger_filters_defaults_to_thread_and_scheduler_buckets() {
        assert_eq!(
            bundle_trigger_filters(None, None),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                vec![SCHEDULER_TRIGGER.to_string()]
            )
        );
        assert_eq!(
            bundle_trigger_filters(None, Some("   ")),
            (
                vec![
                    THREAD_DIRECT_TRIGGER.to_string(),
                    SUBJECT_DIRECT_TRIGGER.to_string()
                ],
                vec![SCHEDULER_TRIGGER.to_string()]
            )
        );
    }

    #[test]
    fn bundle_trigger_filters_includes_subject_direct_by_default() {
        let (direct, scheduler) = bundle_trigger_filters(None, None);

        assert!(direct.iter().any(|item| item == THREAD_DIRECT_TRIGGER));
        assert!(direct.iter().any(|item| item == SUBJECT_DIRECT_TRIGGER));
        assert_eq!(scheduler, vec![SCHEDULER_TRIGGER.to_string()]);
    }

    #[test]
    fn bundle_trigger_filters_respects_scheduler_only_requests() {
        assert_eq!(
            bundle_trigger_filters(None, Some("scheduler")),
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
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
    fn bundle_trigger_filters_routes_summary_jobs_into_scheduler_bucket_by_default() {
        assert_eq!(
            bundle_trigger_filters(Some(JOB_TYPE_SUMMARY), None),
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
        );
        assert_eq!(
            bundle_trigger_filters(Some(JOB_TYPE_ROLLUP), None),
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
        );
    }

    #[test]
    fn bundle_trigger_filters_treats_thread_direct_summary_filter_as_scheduler_bucket() {
        assert_eq!(
            bundle_trigger_filters(Some(JOB_TYPE_SUMMARY), Some("thread_direct")),
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
        );
        assert_eq!(
            bundle_trigger_filters(Some(JOB_TYPE_ROLLUP), Some("thread_direct")),
            (Vec::new(), vec![SCHEDULER_TRIGGER.to_string()])
        );
    }
}
