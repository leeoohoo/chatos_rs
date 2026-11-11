// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Serialize;

use crate::models::{
    EngineSummary, ListResponse, SdkDeleteThreadSummaryRequest, SdkListThreadSummariesRequest,
};

use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn list_thread_summaries(
        &self,
        thread_id: &str,
        req: &SdkListThreadSummariesRequest,
    ) -> Result<Vec<EngineSummary>, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "list_thread_summaries")?;
                #[derive(Serialize)]
                struct DirectListThreadSummariesRequest<'a> {
                    source_id: &'a str,
                    #[serde(flatten)]
                    request: &'a SdkListThreadSummariesRequest,
                }
                let resp: ListResponse<EngineSummary> = self
                    .send_json(
                        Method::POST,
                        &format!("/threads/{}/summaries", urlencoding::encode(thread_id)),
                        Some(&DirectListThreadSummariesRequest {
                            source_id,
                            request: req,
                        }),
                    )
                    .await?;
                Ok(resp.items)
            }
            AuthMode::SystemKey { .. } => {
                let resp: ListResponse<EngineSummary> = self
                    .send_json(
                        Method::POST,
                        &format!("/sdk/threads/{}/summaries", urlencoding::encode(thread_id)),
                        Some(req),
                    )
                    .await?;
                Ok(resp.items)
            }
        }
    }

    pub async fn delete_thread_summary(
        &self,
        thread_id: &str,
        summary_id: &str,
        tenant_id: &str,
        project_partition_key: &str,
        runtime_context_assertion: &str,
    ) -> Result<usize, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "delete_thread_summary")?;
                #[derive(serde::Deserialize)]
                struct DeleteSummaryResponse {
                    reset_records: usize,
                }
                #[derive(Serialize)]
                struct DirectDeleteThreadSummaryRequest<'a> {
                    tenant_id: &'a str,
                    source_id: &'a str,
                    project_partition_key: &'a str,
                    runtime_context_assertion: &'a str,
                }
                let resp: DeleteSummaryResponse = self
                    .send_json(
                        Method::DELETE,
                        &format!(
                            "/threads/{}/summaries/{}",
                            urlencoding::encode(thread_id),
                            urlencoding::encode(summary_id)
                        ),
                        Some(&DirectDeleteThreadSummaryRequest {
                            tenant_id,
                            source_id,
                            project_partition_key,
                            runtime_context_assertion,
                        }),
                    )
                    .await?;
                Ok(resp.reset_records)
            }
            AuthMode::SystemKey { .. } => {
                #[derive(serde::Deserialize)]
                struct DeleteSummaryResponse {
                    reset_records: usize,
                }
                let resp: DeleteSummaryResponse = self
                    .send_json(
                        Method::DELETE,
                        &format!(
                            "/sdk/threads/{}/summaries/{}",
                            urlencoding::encode(thread_id),
                            urlencoding::encode(summary_id)
                        ),
                        Some(&SdkDeleteThreadSummaryRequest {
                            tenant_id: tenant_id.to_string(),
                            project_partition_key: project_partition_key.to_string(),
                            runtime_context_assertion: runtime_context_assertion.to_string(),
                        }),
                    )
                    .await?;
                Ok(resp.reset_records)
            }
        }
    }
}
