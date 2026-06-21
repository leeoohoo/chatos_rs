use reqwest::Method;
use serde::Serialize;

use crate::models::{
    RunThreadActiveSummaryResponse, RunThreadRepairSummaryResponse, RunThreadSummaryResponse,
    SdkGetThreadActiveSummaryStatusRequest, SdkRunThreadActiveSummaryRequest,
    SdkRunThreadRepairSummaryRequest, SdkRunThreadSummaryRequest,
};

use super::super::transport::append_optional_query;
use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn run_thread_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
    ) -> Result<RunThreadSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRunThreadSummaryRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "run_thread_summary")?;
                let req = DirectRunThreadSummaryRequest {
                    tenant_id,
                    source_id,
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
                };
                self.send_json(
                    Method::POST,
                    &format!("/sdk/threads/{}/summaries/run", urlencoding::encode(thread_id)),
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
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRunThreadActiveSummaryRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            trigger_reason: Option<&'a str>,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "run_thread_active_summary")?;
                let req = DirectRunThreadActiveSummaryRequest {
                    tenant_id,
                    source_id,
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
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(
                    source_id,
                    "get_thread_active_summary_status",
                )?;
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(tenant_id));
                append_optional_query(&mut query, "source_id", Some(source_id));
                append_optional_query(&mut query, "job_run_id", job_run_id);
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };
                self.send_json(
                    Method::GET,
                    &format!(
                        "/threads/{}/active-summary/status{suffix}",
                        urlencoding::encode(thread_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkGetThreadActiveSummaryStatusRequest {
                    tenant_id: tenant_id.to_string(),
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

    pub async fn run_thread_repair_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
    ) -> Result<RunThreadRepairSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRunThreadRepairSummaryRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
        }

        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "run_thread_repair_summary")?;
                let req = DirectRunThreadRepairSummaryRequest {
                    tenant_id,
                    source_id,
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/threads/{}/repair-summaries/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkRunThreadRepairSummaryRequest {
                    tenant_id: tenant_id.to_string(),
                };
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/repair-summaries/run",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&req),
                )
                .await
            }
        }
    }
}
