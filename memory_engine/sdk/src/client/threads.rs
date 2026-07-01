// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Serialize;

use crate::models::{
    DeleteThreadResponse, EngineThread, GetThreadResponse, ListResponse, SdkGetThreadRequest,
    SdkListThreadsRequest, SdkUpsertThreadRequest,
};

use super::transport::{append_optional_i64_query, append_optional_query};
use super::{optional_direct_source_id, require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn upsert_thread(
        &self,
        thread_id: &str,
        req: &SdkUpsertThreadRequest,
    ) -> Result<EngineThread, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "upsert_thread")?;
                #[derive(Serialize)]
                struct DirectThreadRequest<'a> {
                    tenant_id: &'a str,
                    source_id: &'a str,
                    subject_id: &'a str,
                    thread_type: &'a str,
                    external_thread_id: &'a Option<String>,
                    title: &'a Option<String>,
                    labels: &'a Option<Vec<String>>,
                    metadata: &'a Option<serde_json::Value>,
                    status: &'a Option<String>,
                    created_at: &'a Option<String>,
                    updated_at: &'a Option<String>,
                    archived_at: &'a Option<String>,
                }

                let direct = DirectThreadRequest {
                    tenant_id: req.tenant_id.as_str(),
                    source_id,
                    subject_id: req.subject_id.as_str(),
                    thread_type: req.thread_type.as_str(),
                    external_thread_id: &req.external_thread_id,
                    title: &req.title,
                    labels: &req.labels,
                    metadata: &req.metadata,
                    status: &req.status,
                    created_at: &req.created_at,
                    updated_at: &req.updated_at,
                    archived_at: &req.archived_at,
                };
                self.send_json(
                    Method::PUT,
                    &format!("/threads/{}", urlencoding::encode(thread_id)),
                    Some(&direct),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::PUT,
                    &format!("/sdk/threads/{}", urlencoding::encode(thread_id)),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn delete_thread(
        &self,
        thread_id: &str,
        tenant_id: &str,
    ) -> Result<DeleteThreadResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "delete_thread")?;
                self.send_json::<DeleteThreadResponse, _>(
                    Method::DELETE,
                    &format!(
                        "/threads/{}?tenant_id={}&source_id={}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(tenant_id),
                        urlencoding::encode(source_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json::<DeleteThreadResponse, _>(
                    Method::DELETE,
                    &format!(
                        "/sdk/threads/{}?tenant_id={}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(tenant_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
        }
    }

    pub async fn get_thread(
        &self,
        thread_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Option<EngineThread>, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", tenant_id);
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                let full_path = if query.is_empty() {
                    format!("/threads/{}", urlencoding::encode(thread_id))
                } else {
                    format!("/threads/{}?{query}", urlencoding::encode(thread_id))
                };
                let resp: GetThreadResponse = self
                    .send_json(Method::GET, full_path.as_str(), Option::<&()>::None)
                    .await?;
                Ok(resp.item)
            }
            AuthMode::SystemKey { .. } => {
                let resp: GetThreadResponse = self
                    .send_json(
                        Method::POST,
                        &format!("/sdk/threads/{}", urlencoding::encode(thread_id)),
                        Some(&SdkGetThreadRequest {
                            tenant_id: tenant_id.map(ToOwned::to_owned),
                        }),
                    )
                    .await?;
                Ok(resp.item)
            }
        }
    }

    pub async fn list_threads(
        &self,
        req: &SdkListThreadsRequest,
    ) -> Result<Vec<EngineThread>, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "subject_id", req.subject_id.as_deref());
                append_optional_query(
                    &mut query,
                    "external_thread_id",
                    req.external_thread_id.as_deref(),
                );
                append_optional_query(&mut query, "session_id", req.session_id.as_deref());
                append_optional_query(&mut query, "contact_id", req.contact_id.as_deref());
                append_optional_query(&mut query, "project_id", req.project_id.as_deref());
                append_optional_query(&mut query, "agent_id", req.agent_id.as_deref());
                append_optional_query(&mut query, "mapping_source", req.mapping_source.as_deref());
                append_optional_query(
                    &mut query,
                    "mapping_version",
                    req.mapping_version.as_deref(),
                );
                append_optional_query(&mut query, "thread_label", req.thread_label.as_deref());
                append_optional_query(&mut query, "status", req.status.as_deref());
                append_optional_i64_query(&mut query, "limit", req.limit);
                append_optional_i64_query(&mut query, "offset", req.offset);
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };
                let resp: ListResponse<EngineThread> = self
                    .send_json(
                        Method::GET,
                        &format!("/admin/threads/query{suffix}"),
                        Option::<&()>::None,
                    )
                    .await?;
                Ok(resp.items)
            }
            AuthMode::SystemKey { .. } => {
                let resp: ListResponse<EngineThread> = self
                    .send_json(Method::POST, "/sdk/threads/query", Some(req))
                    .await?;
                Ok(resp.items)
            }
        }
    }
}
