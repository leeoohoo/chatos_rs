// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{
    MemoryEngineClient, SdkBatchSyncRecordsRequest, SdkUpsertThreadRequest, UpsertRecordInput,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::traits::{MemoryRecordWriter, SaveRecordInput, SaveToolRecordInput};

use super::log_summary::summarize_record_batch;
use super::normalized;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecordScope {
    pub tenant_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_metadata_key: Option<String>,
    pub thread_id: Option<String>,
    pub record_type: String,
    pub default_summary_status: Option<String>,
}

impl MemoryRecordScope {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            tenant_metadata_key: None,
            thread_id: None,
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    pub fn message_thread(tenant_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            tenant_metadata_key: None,
            thread_id: Some(thread_id.into()),
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    /// Routes each record to the tenant carried in its metadata. This is used
    /// by multi-user runtimes that share one process-local Memory client.
    pub fn per_record_tenant(metadata_key: impl Into<String>) -> Self {
        Self {
            tenant_id: String::new(),
            tenant_metadata_key: Some(metadata_key.into()),
            thread_id: None,
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub fn with_record_type(mut self, record_type: impl Into<String>) -> Self {
        self.record_type = record_type.into();
        self
    }

    pub fn with_default_summary_status(mut self, default_summary_status: Option<String>) -> Self {
        self.default_summary_status = default_summary_status;
        self
    }
}

#[derive(Clone)]
pub struct MemoryEngineRecordWriter {
    client: MemoryEngineClient,
    scope: MemoryRecordScope,
    source_id: Option<String>,
    ensured_threads: Arc<Mutex<HashSet<(String, String)>>>,
}

impl MemoryEngineRecordWriter {
    pub fn new_direct(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        scope: MemoryRecordScope,
    ) -> Result<Self, String> {
        let source_id = source_id.into();
        Ok(Self {
            client: MemoryEngineClient::new_direct(base_url, timeout, source_id.clone())?,
            scope,
            source_id: Some(source_id),
            ensured_threads: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub fn from_client(client: MemoryEngineClient, scope: MemoryRecordScope) -> Self {
        Self {
            client,
            scope,
            source_id: None,
            ensured_threads: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }
}

#[async_trait]
impl MemoryRecordWriter for MemoryEngineRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let tenant_id = self.tenant_id_for_record(&input)?;
        let thread_id = self.thread_id_for_record(&input)?;
        self.ensure_thread(tenant_id.as_str(), thread_id.as_str())
            .await?;
        let record = self.upsert_record_input(input)?;
        let records = vec![record];
        let summary = summarize_record_batch(records.as_slice());
        let source_id = self.source_id.as_deref().unwrap_or("");
        info!(
            tenant_id = tenant_id.as_str(),
            source_id,
            thread_id = thread_id.as_str(),
            record_count = summary.record_count,
            record_roles = summary.roles.as_str(),
            record_ids = summary.record_ids.as_str(),
            tool_names = summary.tool_names.as_str(),
            content_bytes = summary.content_bytes,
            max_content_bytes = summary.max_content_bytes,
            metadata_bytes = summary.metadata_bytes,
            structured_payload_bytes = summary.structured_payload_bytes,
            "memory engine record batch sync start"
        );
        let response = self
            .client
            .batch_sync_records(
                thread_id.as_str(),
                &SdkBatchSyncRecordsRequest { tenant_id, records },
            )
            .await;
        match &response {
            Ok(response) => {
                info!(
                    source_id,
                    thread_id = thread_id.as_str(),
                    received_count = response.received_count,
                    upserted_count = response.upserted_count,
                    "memory engine record batch sync completed"
                );
            }
            Err(err) => {
                warn!(
                    source_id,
                    thread_id = thread_id.as_str(),
                    record_count = summary.record_count,
                    record_roles = summary.roles.as_str(),
                    record_ids = summary.record_ids.as_str(),
                    tool_names = summary.tool_names.as_str(),
                    content_bytes = summary.content_bytes,
                    max_content_bytes = summary.max_content_bytes,
                    metadata_bytes = summary.metadata_bytes,
                    structured_payload_bytes = summary.structured_payload_bytes,
                    error = err.as_str(),
                    "memory engine record batch sync failed"
                );
            }
        }
        response?;
        Ok(())
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        if inputs.is_empty() {
            return Ok(());
        }

        let mut batches: BTreeMap<(String, String), Vec<UpsertRecordInput>> = BTreeMap::new();
        for input in inputs {
            let input: SaveRecordInput = input.into();
            let tenant_id = self.tenant_id_for_record(&input)?;
            let thread_id = self.thread_id_for_record(&input)?;
            let record = self.upsert_record_input(input)?;
            batches
                .entry((tenant_id, thread_id))
                .or_default()
                .push(record);
        }

        for ((tenant_id, thread_id), records) in batches {
            self.ensure_thread(tenant_id.as_str(), thread_id.as_str())
                .await?;
            let summary = summarize_record_batch(records.as_slice());
            let source_id = self.source_id.as_deref().unwrap_or("");
            info!(
                tenant_id = tenant_id.as_str(),
                source_id,
                thread_id = thread_id.as_str(),
                record_count = summary.record_count,
                record_roles = summary.roles.as_str(),
                record_ids = summary.record_ids.as_str(),
                tool_names = summary.tool_names.as_str(),
                content_bytes = summary.content_bytes,
                max_content_bytes = summary.max_content_bytes,
                metadata_bytes = summary.metadata_bytes,
                structured_payload_bytes = summary.structured_payload_bytes,
                "memory engine tool record batch sync start"
            );
            let response = self
                .client
                .batch_sync_records(
                    thread_id.as_str(),
                    &SdkBatchSyncRecordsRequest {
                        tenant_id: tenant_id.clone(),
                        records,
                    },
                )
                .await;
            match &response {
                Ok(response) => {
                    info!(
                        source_id,
                        thread_id = thread_id.as_str(),
                        received_count = response.received_count,
                        upserted_count = response.upserted_count,
                        "memory engine tool record batch sync completed"
                    );
                }
                Err(err) => {
                    warn!(
                        source_id,
                        thread_id = thread_id.as_str(),
                        record_count = summary.record_count,
                        record_roles = summary.roles.as_str(),
                        record_ids = summary.record_ids.as_str(),
                        tool_names = summary.tool_names.as_str(),
                        content_bytes = summary.content_bytes,
                        max_content_bytes = summary.max_content_bytes,
                        metadata_bytes = summary.metadata_bytes,
                        structured_payload_bytes = summary.structured_payload_bytes,
                        error = err.as_str(),
                        "memory engine tool record batch sync failed"
                    );
                }
            }
            response?;
        }

        Ok(())
    }
}

impl MemoryEngineRecordWriter {
    async fn ensure_thread(&self, tenant_id: &str, thread_id: &str) -> Result<(), String> {
        let key = (tenant_id.to_string(), thread_id.to_string());
        if self.ensured_threads.lock().await.contains(&key) {
            return Ok(());
        }
        self.client
            .upsert_thread(
                thread_id,
                &SdkUpsertThreadRequest {
                    tenant_id: tenant_id.to_string(),
                    subject_id: tenant_id.to_string(),
                    thread_type: "conversation".to_string(),
                    external_thread_id: Some(thread_id.to_string()),
                    title: None,
                    labels: None,
                    metadata: None,
                    status: Some("active".to_string()),
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                },
            )
            .await?;
        self.ensured_threads.lock().await.insert(key);
        Ok(())
    }

    pub(crate) fn tenant_id_for_record(&self, input: &SaveRecordInput) -> Result<String, String> {
        if let Some(tenant_id) = normalized(self.scope.tenant_id.as_str()) {
            return Ok(tenant_id);
        }
        let metadata_key = self
            .scope
            .tenant_metadata_key
            .as_deref()
            .and_then(normalized)
            .ok_or_else(|| "memory record tenant_id is required".to_string())?;
        input
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(metadata_key.as_str()))
            .and_then(serde_json::Value::as_str)
            .and_then(normalized)
            .ok_or_else(|| {
                format!("memory record metadata.{metadata_key} must contain a tenant_id")
            })
    }

    fn thread_id_for_record(&self, input: &SaveRecordInput) -> Result<String, String> {
        self.scope
            .thread_id
            .as_deref()
            .and_then(normalized)
            .or_else(|| normalized(input.conversation_id.as_str()))
            .ok_or_else(|| "memory record thread_id is required".to_string())
    }

    fn upsert_record_input(&self, input: SaveRecordInput) -> Result<UpsertRecordInput, String> {
        let role = normalized(input.role.as_str())
            .ok_or_else(|| "memory record role is required".to_string())?;
        let record_type =
            normalized(self.scope.record_type.as_str()).unwrap_or_else(|| "message".to_string());
        let metadata = input.packed_metadata();
        Ok(UpsertRecordInput {
            id: input
                .message_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            external_record_id: None,
            role,
            record_type,
            content: input.content,
            structured_payload: input.structured_payload,
            metadata,
            summary_status: input
                .summary_status
                .or_else(|| self.scope.default_summary_status.clone()),
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input
                .created_at
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        })
    }
}
