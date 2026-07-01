// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{MemoryEngineClient, SdkBatchSyncRecordsRequest, UpsertRecordInput};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::traits::{
    MemoryRecordWriter, SaveAssistantRecordInput, SaveRecordInput, SaveToolRecordInput,
};

use super::log_summary::{
    summarize_assistant_record_input, summarize_record_batch, summarize_save_record_input,
    summarize_tool_record_input, summarize_tool_record_inputs,
};
use super::normalized;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecordScope {
    pub tenant_id: String,
    pub thread_id: Option<String>,
    pub record_type: String,
    pub default_summary_status: Option<String>,
}

impl MemoryRecordScope {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            thread_id: None,
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
        }
    }

    pub fn message_thread(tenant_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            thread_id: Some(thread_id.into()),
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
}

#[derive(Clone)]
pub struct BestEffortMemoryRecordWriter {
    inner: Arc<dyn MemoryRecordWriter>,
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
        })
    }

    pub fn from_client(client: MemoryEngineClient, scope: MemoryRecordScope) -> Self {
        Self {
            client,
            scope,
            source_id: None,
        }
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }
}

impl BestEffortMemoryRecordWriter {
    pub fn new<T>(inner: T) -> Self
    where
        T: MemoryRecordWriter + 'static,
    {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn from_arc(inner: Arc<dyn MemoryRecordWriter>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl MemoryRecordWriter for MemoryEngineRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let tenant_id = self.tenant_id()?;
        let thread_id = self.thread_id_for_record(&input)?;
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

        let tenant_id = self.tenant_id()?;
        let mut batches: BTreeMap<String, Vec<UpsertRecordInput>> = BTreeMap::new();
        for input in inputs {
            let input: SaveRecordInput = input.into();
            let thread_id = self.thread_id_for_record(&input)?;
            let record = self.upsert_record_input(input)?;
            batches.entry(thread_id).or_default().push(record);
        }

        for (thread_id, records) in batches {
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

#[async_trait]
impl MemoryRecordWriter for BestEffortMemoryRecordWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let summary = summarize_save_record_input(&input);
        if let Err(err) = self.inner.save_record(input).await {
            warn!(
                role = summary.role.as_str(),
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                tool_call_id = summary.tool_call_id.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory record sync failed"
            );
        }
        Ok(())
    }

    async fn save_assistant_record(&self, input: SaveAssistantRecordInput) -> Result<(), String> {
        let summary = summarize_assistant_record_input(&input);
        if let Err(err) = self.inner.save_assistant_record(input).await {
            warn!(
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                response_id = summary.response_id.as_str(),
                response_status = summary.response_status.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory assistant record sync failed"
            );
        }
        Ok(())
    }

    async fn save_tool_record(&self, input: SaveToolRecordInput) -> Result<(), String> {
        let summary = summarize_tool_record_input(&input);
        if let Err(err) = self.inner.save_tool_record(input).await {
            warn!(
                conversation_id = summary.conversation_id.as_str(),
                conversation_turn_id = summary.conversation_turn_id.as_str(),
                message_id = summary.message_id.as_str(),
                tool_call_id = summary.tool_call_id.as_str(),
                tool_name = summary.tool_name.as_str(),
                content_bytes = summary.content_bytes,
                error = err.as_str(),
                "best-effort memory tool record sync failed"
            );
        }
        Ok(())
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        let summary = summarize_tool_record_inputs(inputs.as_slice());
        if let Err(err) = self.inner.save_tool_records(inputs).await {
            warn!(
                record_count = summary.record_count,
                conversation_ids = summary.conversation_ids.as_str(),
                conversation_turn_ids = summary.conversation_turn_ids.as_str(),
                tool_names = summary.tool_names.as_str(),
                tool_call_ids = summary.tool_call_ids.as_str(),
                content_bytes = summary.content_bytes,
                max_content_bytes = summary.max_content_bytes,
                error = err.as_str(),
                "best-effort memory tool records sync failed"
            );
        }
        Ok(())
    }
}

impl MemoryEngineRecordWriter {
    fn tenant_id(&self) -> Result<String, String> {
        normalized(self.scope.tenant_id.as_str())
            .ok_or_else(|| "memory record tenant_id is required".to_string())
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
