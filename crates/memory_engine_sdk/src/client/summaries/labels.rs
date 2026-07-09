// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{
    EngineSummary, ListResponse, ListSummariesByThreadLabelRequest,
    SdkListSummariesByThreadLabelRequest, SystemListSummariesByThreadLabelRequest,
};

use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn list_summaries_by_thread_label(
        &self,
        req: &ListSummariesByThreadLabelRequest,
    ) -> Result<Vec<EngineSummary>, String> {
        let resp: ListResponse<EngineSummary> = match &self.auth {
            AuthMode::Direct { .. } => {
                self.send_json(Method::POST, "/summaries/query-by-thread-label", Some(req))
                    .await?
            }
            AuthMode::SystemKey { .. } => {
                let direct = SdkListSummariesByThreadLabelRequest {
                    tenant_id: req.tenant_id.clone(),
                    thread_label: req.thread_label.clone(),
                    summary_type: req.summary_type.clone(),
                    status: req.status.clone(),
                    level: req.level,
                    subject_memory_summarized: req.subject_memory_summarized,
                    limit: req.limit,
                    offset: req.offset,
                };
                self.send_json(
                    Method::POST,
                    "/sdk/summaries/query-by-thread-label",
                    Some(&direct),
                )
                .await?
            }
        };
        Ok(resp.items)
    }

    pub async fn list_summaries_by_thread_label_system(
        &self,
        req: &SystemListSummariesByThreadLabelRequest,
    ) -> Result<Vec<EngineSummary>, String> {
        let resp: ListResponse<EngineSummary> = match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "list_summaries_by_thread_label_system")?;
                let direct = ListSummariesByThreadLabelRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: source_id.to_string(),
                    thread_label: req.thread_label.clone(),
                    summary_type: req.summary_type.clone(),
                    status: req.status.clone(),
                    level: req.level,
                    subject_memory_summarized: req.subject_memory_summarized,
                    limit: req.limit,
                    offset: req.offset,
                };
                self.send_json(
                    Method::POST,
                    "/summaries/query-by-thread-label",
                    Some(&direct),
                )
                .await?
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    "/sdk/summaries/query-by-thread-label",
                    Some(req),
                )
                .await?
            }
        };
        Ok(resp.items)
    }
}
