use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde_json::json;
use tokio::try_join;

use super::error::internal_error;
use super::queries::{JobRunStatsQuery, JobRunsQuery};
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
    Query(q): Query<JobRunsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::list_job_runs(
        &state.pool,
        q.job_type.as_deref(),
        q.trigger_type.as_deref(),
        q.thread_id.as_deref(),
        q.status.as_deref(),
        q.tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await
    .map(|items| Json(json!({ "items": items })))
    .map_err(internal_error)
}

pub async fn job_run_stats(
    State(state): State<Arc<AppState>>,
    Query(q): Query<JobRunStatsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::job_run_stats(
        &state.pool,
        q.job_type.as_deref(),
        q.tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.since_hours.unwrap_or(24),
    )
    .await
    .map(|stats| Json(json!({ "stats": stats })))
    .map_err(internal_error)
}

pub async fn job_runs_bundle(
    State(state): State<Arc<AppState>>,
    Query(q): Query<JobRunsQuery>,
) -> Result<Json<JobRunsBundleResponse>, (axum::http::StatusCode, String)> {
    let limit = q.limit.unwrap_or(200);
    let (thread_triggers, scheduler_triggers) =
        bundle_trigger_filters(q.job_type.as_deref(), q.trigger_type.as_deref());
    let (thread_runs, scheduler_runs) = try_join!(
        list_job_run_bucket(&state.pool, &q, thread_triggers.as_slice(), limit),
        list_job_run_bucket(&state.pool, &q, scheduler_triggers.as_slice(), limit),
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
            query.tenant_id.as_deref(),
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
) -> Result<Json<DashboardOverviewResponse>, (axum::http::StatusCode, String)> {
    let (source_count, model_count, policy_count, job_stats) = try_join!(
        sources::count_sources(&state.pool),
        control_plane::count_model_profiles(&state.pool),
        control_plane::count_job_policies(&state.pool),
        control_plane::job_run_stats(&state.pool, None, None, None, 24),
    )
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
    let scheduler_only_job = matches!(normalized_job_type, Some(JOB_TYPE_SUMMARY | JOB_TYPE_ROLLUP));

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
