use reqwest::Method;

use crate::models::{
    EngineSummary, ListResponse, SdkDeleteThreadSummaryRequest, SdkListThreadSummariesRequest,
};

use super::super::transport::{append_optional_i64_query, append_optional_query};
use super::super::{
    optional_direct_source_id, require_direct_source_id, AuthMode, MemoryEngineClient,
};

impl MemoryEngineClient {
    pub async fn list_thread_summaries(
        &self,
        thread_id: &str,
        req: &SdkListThreadSummariesRequest,
    ) -> Result<Vec<EngineSummary>, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "summary_type", req.summary_type.as_deref());
                append_optional_query(&mut query, "status", req.status.as_deref());
                append_optional_i64_query(&mut query, "level", req.level);
                append_optional_i64_query(&mut query, "limit", req.limit);
                append_optional_i64_query(&mut query, "offset", req.offset);
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };
                let resp: ListResponse<EngineSummary> = self
                    .send_json(
                        Method::GET,
                        &format!(
                            "/threads/{}/summaries{suffix}",
                            urlencoding::encode(thread_id)
                        ),
                        Option::<&()>::None,
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
    ) -> Result<usize, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "delete_thread_summary")?;
                #[derive(serde::Deserialize)]
                struct DeleteSummaryResponse {
                    reset_records: usize,
                }
                let resp: DeleteSummaryResponse = self
                    .send_json(
                        Method::DELETE,
                        &format!(
                            "/threads/{}/summaries/{}?tenant_id={}&source_id={}",
                            urlencoding::encode(thread_id),
                            urlencoding::encode(summary_id),
                            urlencoding::encode(tenant_id),
                            urlencoding::encode(source_id)
                        ),
                        Option::<&()>::None,
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
                        }),
                    )
                    .await?;
                Ok(resp.reset_records)
            }
        }
    }
}
