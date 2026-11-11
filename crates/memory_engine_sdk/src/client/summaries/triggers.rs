// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Serialize;

use crate::models::{
    RunPendingSummaryRebuildResponse, RunThreadActiveSummaryResponse, RunThreadSummaryResponse,
    SdkGetThreadActiveSummaryStatusRequest, SdkRunPendingSummaryRebuildRequest,
    SdkRunThreadActiveSummaryRequest, SdkRunThreadSummaryRequest,
};

use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn run_thread_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
        project_partition_key: &str,
        runtime_context_assertion: &str,
    ) -> Result<RunThreadSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRunThreadSummaryRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            project_partition_key: &'a str,
            runtime_context_assertion: &'a str,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "run_thread_summary")?;
                let req = DirectRunThreadSummaryRequest {
                    tenant_id,
                    source_id,
                    project_partition_key,
                    runtime_context_assertion,
                };
                self.send_json(
                    Method::POST,
                    &format!("/threads/{}/summaries/run", urlencoding::encode(thread_id)),
                    Some(&req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunThreadSummaryRequest {
                    tenant_id: tenant_id.to_string(),
                    project_partition_key: project_partition_key.to_string(),
                    runtime_context_assertion: runtime_context_assertion.to_string(),
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/summaries/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
        }
    }

    pub async fn run_thread_active_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
        project_partition_key: &str,
        runtime_context_assertion: &str,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRunThreadActiveSummaryRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            project_partition_key: &'a str,
            runtime_context_assertion: &'a str,
            trigger_reason: Option<&'a str>,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "run_thread_active_summary")?;
                let req = DirectRunThreadActiveSummaryRequest {
                    tenant_id,
                    source_id,
                    project_partition_key,
                    runtime_context_assertion,
                    trigger_reason,
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/threads/{}/active-summary/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunThreadActiveSummaryRequest {
                    tenant_id: tenant_id.to_string(),
                    project_partition_key: project_partition_key.to_string(),
                    runtime_context_assertion: runtime_context_assertion.to_string(),
                    trigger_reason: trigger_reason.map(ToOwned::to_owned),
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/active-summary/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
        }
    }

    pub async fn get_thread_active_summary_status(
        &self,
        thread_id: &str,
        tenant_id: &str,
        project_partition_key: &str,
        runtime_context_assertion: &str,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectGetThreadActiveSummaryStatusRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            project_partition_key: &'a str,
            runtime_context_assertion: &'a str,
            job_run_id: Option<&'a str>,
        }
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "get_thread_active_summary_status")?;
                let req = DirectGetThreadActiveSummaryStatusRequest {
                    tenant_id,
                    source_id,
                    project_partition_key,
                    runtime_context_assertion,
                    job_run_id,
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/threads/{}/active-summary/status",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkGetThreadActiveSummaryStatusRequest {
                    tenant_id: tenant_id.to_string(),
                    project_partition_key: project_partition_key.to_string(),
                    runtime_context_assertion: runtime_context_assertion.to_string(),
                    job_run_id: job_run_id.map(ToOwned::to_owned),
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/active-summary/status",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
        }
    }

    pub async fn run_pending_summary_rebuild(
        &self,
        thread_id: &str,
        tenant_id: &str,
        project_partition_key: &str,
        runtime_context_assertion: &str,
    ) -> Result<RunPendingSummaryRebuildResponse, String> {
        #[derive(Serialize)]
        struct DirectRunPendingSummaryRebuildRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            project_partition_key: &'a str,
            runtime_context_assertion: &'a str,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "run_pending_summary_rebuild")?;
                let req = DirectRunPendingSummaryRebuildRequest {
                    tenant_id,
                    source_id,
                    project_partition_key,
                    runtime_context_assertion,
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/threads/{}/pending-rebuild/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunPendingSummaryRebuildRequest {
                    tenant_id: tenant_id.to_string(),
                    project_partition_key: project_partition_key.to_string(),
                    runtime_context_assertion: runtime_context_assertion.to_string(),
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/pending-rebuild/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
        }
    }
}
