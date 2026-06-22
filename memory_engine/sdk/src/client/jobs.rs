use reqwest::Method;
use serde::Serialize;

use crate::models::{
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
    SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest,
    SdkRunSubjectMemoryScopesRequest,
};

use super::{optional_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn run_pending_summaries_once(
        &self,
        tenant_id: Option<&str>,
        max_threads: Option<i64>,
    ) -> Result<RunPendingSummariesResponse, String> {
        #[derive(Serialize)]
        struct DirectRunPendingSummariesRequest<'a> {
            tenant_id: Option<&'a str>,
            source_id: Option<&'a str>,
            max_threads: Option<i64>,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let req = DirectRunPendingSummariesRequest {
                    tenant_id,
                    source_id: optional_direct_source_id(source_id),
                    max_threads,
                };
                self.send_json(Method::POST, "/jobs/summaries/run-once", Some(&req))
                    .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunPendingSummariesRequest {
                    tenant_id: tenant_id.map(ToOwned::to_owned),
                    max_threads,
                };
                self.send_json(Method::POST, "/sdk/jobs/summaries/run-once", Some(&req))
                    .await
            }
        }
    }

    pub async fn run_pending_rollups_once(
        &self,
        tenant_id: Option<&str>,
        summary_prompt: Option<&str>,
        max_threads: Option<i64>,
        token_limit: Option<i64>,
        target_summary_tokens: Option<i64>,
        count_limit: Option<i64>,
        keep_level0_count: Option<i64>,
        max_level: Option<i64>,
    ) -> Result<RunPendingRollupsResponse, String> {
        #[derive(Serialize)]
        struct DirectRunPendingRollupsRequest<'a> {
            tenant_id: Option<&'a str>,
            source_id: Option<&'a str>,
            summary_prompt: Option<&'a str>,
            max_threads: Option<i64>,
            token_limit: Option<i64>,
            target_summary_tokens: Option<i64>,
            count_limit: Option<i64>,
            keep_level0_count: Option<i64>,
            max_level: Option<i64>,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let req = DirectRunPendingRollupsRequest {
                    tenant_id,
                    source_id: optional_direct_source_id(source_id),
                    summary_prompt,
                    max_threads,
                    token_limit,
                    target_summary_tokens,
                    count_limit,
                    keep_level0_count,
                    max_level,
                };
                self.send_json(Method::POST, "/jobs/rollups/run-once", Some(&req))
                    .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunPendingRollupsRequest {
                    tenant_id: tenant_id.map(ToOwned::to_owned),
                    summary_prompt: summary_prompt.map(ToOwned::to_owned),
                    max_threads,
                    token_limit,
                    target_summary_tokens,
                    count_limit,
                    keep_level0_count,
                    max_level,
                };
                self.send_json(Method::POST, "/sdk/jobs/rollups/run-once", Some(&req))
                    .await
            }
        }
    }

    pub async fn run_subject_memory_scopes_once(
        &self,
        tenant_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<RunSubjectMemoryScopesResponse, String> {
        #[derive(Serialize)]
        struct DirectRunSubjectMemoryScopesRequest<'a> {
            tenant_id: Option<&'a str>,
            source_id: Option<&'a str>,
            limit: Option<i64>,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let req = DirectRunSubjectMemoryScopesRequest {
                    tenant_id,
                    source_id: optional_direct_source_id(source_id),
                    limit,
                };
                self.send_json(Method::POST, "/jobs/subject-memory-scopes/run-once", Some(&req))
                    .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunSubjectMemoryScopesRequest {
                    tenant_id: tenant_id.map(ToOwned::to_owned),
                    limit,
                };
                self.send_json(
                    Method::POST,
                    "/sdk/jobs/subject-memory-scopes/run-once",
                    Some(&req),
                )
                .await
            }
        }
    }
}
