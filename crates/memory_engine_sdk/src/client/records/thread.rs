// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Serialize;

use crate::models::{
    BatchSyncRecordsResponse, CompactTurnsResponse, CountThreadRecordsResponse, EngineRecord,
    SdkBatchSyncRecordsRequest, SdkCountThreadRecordsRequest, SdkDeleteThreadRecordsRequest,
    SdkGetTurnProcessRecordsRequest, SdkListCompactTurnsRequest, SdkListThreadRecordsRequest,
    ThreadRecordsPageResponse, TurnProcessRecordsResponse,
};

use super::super::transport::{append_optional_i64_query, append_optional_query};
use super::super::{
    optional_direct_source_id, require_direct_source_id, AuthMode, MemoryEngineClient,
};

impl MemoryEngineClient {
    pub async fn batch_sync_records(
        &self,
        thread_id: &str,
        req: &SdkBatchSyncRecordsRequest,
    ) -> Result<BatchSyncRecordsResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "batch_sync_records")?;
                #[derive(Serialize)]
                struct DirectBatchRequest<'a> {
                    tenant_id: &'a str,
                    source_id: &'a str,
                    records: &'a [crate::models::UpsertRecordInput],
                }
                let direct = DirectBatchRequest {
                    tenant_id: req.tenant_id.as_str(),
                    source_id,
                    records: req.records.as_slice(),
                };
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/threads/{}/records/batch-sync",
                        urlencoding::encode(thread_id)
                    ),
                    Some(&direct),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/sdk/threads/{}/records/batch-sync",
                        urlencoding::encode(thread_id)
                    ),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn ingest_thread_records(
        &self,
        thread_id: &str,
        req: &SdkBatchSyncRecordsRequest,
    ) -> Result<BatchSyncRecordsResponse, String> {
        self.batch_sync_records(thread_id, req).await
    }

    pub async fn delete_thread_records(
        &self,
        thread_id: &str,
        tenant_id: &str,
        record_type: Option<&str>,
    ) -> Result<i64, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "delete_thread_records")?;
                let mut query = format!(
                    "tenant_id={}&source_id={}",
                    urlencoding::encode(tenant_id),
                    urlencoding::encode(source_id)
                );
                if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
                    query.push_str("&record_type=");
                    query.push_str(urlencoding::encode(value).as_ref());
                }
                #[derive(serde::Deserialize)]
                struct DeletedResponse {
                    deleted: i64,
                }
                let resp: DeletedResponse = self
                    .send_json::<DeletedResponse, _>(
                        Method::DELETE,
                        &format!(
                            "/threads/{}/records?{query}",
                            urlencoding::encode(thread_id)
                        ),
                        Option::<&()>::None,
                    )
                    .await?;
                Ok(resp.deleted)
            }
            AuthMode::SystemKey { .. } => {
                let req = SdkDeleteThreadRecordsRequest {
                    tenant_id: tenant_id.to_string(),
                    record_type: record_type.map(ToOwned::to_owned),
                };
                #[derive(serde::Deserialize)]
                struct DeletedResponse {
                    deleted: i64,
                }
                let resp: DeletedResponse = self
                    .send_json(
                        Method::DELETE,
                        &format!("/sdk/threads/{}/records", urlencoding::encode(thread_id)),
                        Some(&req),
                    )
                    .await?;
                Ok(resp.deleted)
            }
        }
    }

    pub async fn list_thread_records_page(
        &self,
        thread_id: &str,
        req: &SdkListThreadRecordsRequest,
    ) -> Result<ThreadRecordsPageResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "role", req.role.as_deref());
                append_optional_query(&mut query, "record_type", req.record_type.as_deref());
                append_optional_query(&mut query, "summary_status", req.summary_status.as_deref());
                append_optional_i64_query(&mut query, "limit", req.limit);
                append_optional_i64_query(&mut query, "offset", req.offset);
                append_optional_query(&mut query, "order", req.order.as_deref());
                self.send_json(
                    Method::GET,
                    &format!(
                        "/threads/{}/records?{query}",
                        urlencoding::encode(thread_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    &format!("/sdk/threads/{}/records", urlencoding::encode(thread_id)),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn list_thread_records(
        &self,
        thread_id: &str,
        req: &SdkListThreadRecordsRequest,
    ) -> Result<Vec<EngineRecord>, String> {
        self.list_thread_records_page(thread_id, req)
            .await
            .map(|resp| resp.items)
    }

    pub async fn list_compact_turns(
        &self,
        thread_id: &str,
        req: &SdkListCompactTurnsRequest,
    ) -> Result<CompactTurnsResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "record_type", req.record_type.as_deref());
                append_optional_i64_query(&mut query, "limit", req.limit);
                append_optional_query(&mut query, "before_turn_id", req.before_turn_id.as_deref());
                self.send_json(
                    Method::GET,
                    &format!(
                        "/threads/{}/compact-turns?{query}",
                        urlencoding::encode(thread_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/compact-turns",
                        urlencoding::encode(thread_id)
                    ),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn get_turn_process_records(
        &self,
        thread_id: &str,
        turn_id: &str,
        req: &SdkGetTurnProcessRecordsRequest,
    ) -> Result<TurnProcessRecordsResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "record_type", req.record_type.as_deref());
                self.send_json(
                    Method::GET,
                    &format!(
                        "/threads/{}/turns/{}/process-records?{query}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(turn_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/turns/{}/process-records",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(turn_id)
                    ),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn count_thread_records(
        &self,
        thread_id: &str,
        req: &SdkCountThreadRecordsRequest,
    ) -> Result<i64, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", Some(req.tenant_id.as_str()));
                append_optional_query(
                    &mut query,
                    "source_id",
                    optional_direct_source_id(source_id),
                );
                append_optional_query(&mut query, "role", req.role.as_deref());
                append_optional_query(&mut query, "record_type", req.record_type.as_deref());
                append_optional_query(&mut query, "summary_status", req.summary_status.as_deref());
                let resp: CountThreadRecordsResponse = self
                    .send_json(
                        Method::GET,
                        &format!(
                            "/threads/{}/records/count?{query}",
                            urlencoding::encode(thread_id)
                        ),
                        Option::<&()>::None,
                    )
                    .await?;
                Ok(resp.count)
            }
            AuthMode::SystemKey { .. } => {
                let resp: CountThreadRecordsResponse = self
                    .send_json(
                        Method::POST,
                        &format!(
                            "/sdk/threads/{}/records/count",
                            urlencoding::encode(thread_id)
                        ),
                        Some(req),
                    )
                    .await?;
                Ok(resp.count)
            }
        }
    }
}
