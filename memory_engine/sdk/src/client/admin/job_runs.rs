// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{
    DashboardOverviewResponse, EngineJobRun, JobRunsBundleResponse, ListJobRunsRequest,
    ListResponse,
};

use super::super::transport::{append_optional_i64_query, append_optional_query};
use super::super::MemoryEngineClient;

#[derive(serde::Deserialize)]
struct JobRunStatsEnvelope {
    #[serde(default)]
    stats: serde_json::Value,
}

fn build_job_runs_suffix(req: &ListJobRunsRequest) -> String {
    let mut query = String::new();
    append_optional_query(&mut query, "job_type", req.job_type.as_deref());
    append_optional_query(&mut query, "trigger_type", req.trigger_type.as_deref());
    append_optional_query(&mut query, "thread_id", req.thread_id.as_deref());
    append_optional_query(&mut query, "status", req.status.as_deref());
    append_optional_query(&mut query, "tenant_id", req.tenant_id.as_deref());
    append_optional_query(&mut query, "source_id", req.source_id.as_deref());
    append_optional_i64_query(&mut query, "limit", req.limit);
    if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    }
}

fn build_job_run_stats_suffix(
    job_type: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    since_hours: i64,
) -> String {
    let mut query = String::new();
    append_optional_query(&mut query, "job_type", job_type);
    append_optional_query(&mut query, "tenant_id", tenant_id);
    append_optional_query(&mut query, "source_id", source_id);
    append_optional_i64_query(&mut query, "since_hours", Some(since_hours.max(1)));
    if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    }
}

impl MemoryEngineClient {
    pub async fn list_job_runs(
        &self,
        req: &ListJobRunsRequest,
    ) -> Result<Vec<EngineJobRun>, String> {
        let suffix = build_job_runs_suffix(req);
        let resp: ListResponse<EngineJobRun> = self
            .send_json(
                Method::GET,
                &format!("/admin/job-runs{suffix}"),
                Option::<&()>::None,
            )
            .await?;
        Ok(resp.items)
    }

    pub async fn get_job_run_stats(
        &self,
        job_type: Option<&str>,
        tenant_id: Option<&str>,
        source_id: Option<&str>,
        since_hours: i64,
    ) -> Result<serde_json::Value, String> {
        let suffix = build_job_run_stats_suffix(job_type, tenant_id, source_id, since_hours);
        let resp: JobRunStatsEnvelope = self
            .send_json(
                Method::GET,
                &format!("/admin/job-runs/stats{suffix}"),
                Option::<&()>::None,
            )
            .await?;
        Ok(resp.stats)
    }

    pub async fn get_job_runs_bundle(
        &self,
        req: &ListJobRunsRequest,
    ) -> Result<JobRunsBundleResponse, String> {
        let suffix = build_job_runs_suffix(req);
        self.send_json(
            Method::GET,
            &format!("/admin/job-runs/bundle{suffix}"),
            Option::<&()>::None,
        )
        .await
    }

    pub async fn get_dashboard_overview(&self) -> Result<DashboardOverviewResponse, String> {
        self.send_json(
            Method::GET,
            "/admin/dashboard/overview",
            Option::<&()>::None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{build_job_run_stats_suffix, build_job_runs_suffix, JobRunStatsEnvelope};
    use crate::models::ListJobRunsRequest;

    #[test]
    fn build_job_runs_suffix_encodes_filters() {
        let suffix = build_job_runs_suffix(&ListJobRunsRequest {
            job_type: Some("summary rollup".to_string()),
            trigger_type: Some("thread_direct".to_string()),
            thread_id: Some("thread-1".to_string()),
            status: Some("running".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            source_id: Some("source-1".to_string()),
            limit: Some(20),
        });

        assert_eq!(
            suffix,
            "?job_type=summary%20rollup&trigger_type=thread_direct&thread_id=thread-1&status=running&tenant_id=tenant-1&source_id=source-1&limit=20"
        );
    }

    #[test]
    fn build_job_run_stats_suffix_clamps_since_hours() {
        let suffix = build_job_run_stats_suffix(Some("summary"), Some("tenant-1"), None, 0);

        assert_eq!(suffix, "?job_type=summary&tenant_id=tenant-1&since_hours=1");
    }

    #[test]
    fn build_job_runs_bundle_path_reuses_list_filters() {
        let suffix = build_job_runs_suffix(&ListJobRunsRequest {
            job_type: Some("summary".to_string()),
            trigger_type: Some("scheduler".to_string()),
            thread_id: None,
            status: Some("running".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            source_id: Some("source-1".to_string()),
            limit: Some(5),
        });

        assert_eq!(
            format!("/admin/job-runs/bundle{suffix}"),
            "/admin/job-runs/bundle?job_type=summary&trigger_type=scheduler&status=running&tenant_id=tenant-1&source_id=source-1&limit=5"
        );
    }

    #[test]
    fn job_run_stats_envelope_extracts_stats_payload() {
        let resp: JobRunStatsEnvelope = serde_json::from_value(serde_json::json!({
            "stats": {
                "summary": {
                    "running": 2,
                    "done": 5
                }
            }
        }))
        .expect("stats envelope");

        assert_eq!(resp.stats["summary"]["running"], 2);
        assert_eq!(resp.stats["summary"]["done"], 5);
    }
}
