// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use memory_engine_sdk::{
    MemoryEngineClient, RuntimeContextAssertionIssuer, SdkBatchSyncRecordsRequest,
    UpsertRecordInput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::traits::{MemoryRecordWriter, SaveRecordInput, SaveToolRecordInput};

use super::log_summary::summarize_record_batch;
use super::normalized;
use super::upsert_canonical_memory_thread;
use super::{RuntimeContextAssertionProvider, RuntimeContextAssertionRequest};

#[cfg(test)]
#[path = "record_partition_tests.rs"]
mod partition_tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecordScope {
    pub tenant_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_metadata_key: Option<String>,
    pub thread_id: Option<String>,
    pub record_type: String,
    pub default_summary_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_aggregation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_kind: Option<String>,
}

impl MemoryRecordScope {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            tenant_metadata_key: None,
            thread_id: None,
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
            agent_id: None,
            project_aggregation_id: None,
            thread_kind: None,
        }
    }

    pub fn message_thread(tenant_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            tenant_metadata_key: None,
            thread_id: Some(thread_id.into()),
            record_type: "message".to_string(),
            default_summary_status: Some("pending".to_string()),
            agent_id: None,
            project_aggregation_id: None,
            thread_kind: None,
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
            agent_id: None,
            project_aggregation_id: None,
            thread_kind: None,
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

    pub fn with_memory_identity(
        mut self,
        agent_id: impl Into<String>,
        project_aggregation_id: Option<String>,
        thread_kind: impl Into<String>,
    ) -> Self {
        self.agent_id = Some(agent_id.into());
        self.project_aggregation_id = project_aggregation_id;
        self.thread_kind = Some(thread_kind.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordThreadIdentity {
    agent_id: Option<String>,
    agent_name: Option<String>,
    project_aggregation_id: Option<String>,
    thread_kind: String,
}

#[derive(Clone)]
pub struct MemoryEngineRecordWriter {
    client: MemoryEngineClient,
    scope: MemoryRecordScope,
    source_id: Option<String>,
    assertion_provider: Option<Arc<dyn RuntimeContextAssertionProvider>>,
    ensured_threads: Arc<Mutex<HashMap<(String, String), RecordThreadIdentity>>>,
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
            assertion_provider: None,
            ensured_threads: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn from_client(client: MemoryEngineClient, scope: MemoryRecordScope) -> Self {
        let source_id = client.source_id().map(str::to_string);
        Self {
            client,
            scope,
            source_id,
            assertion_provider: None,
            ensured_threads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_runtime_context_assertion_issuer(
        self,
        assertion_issuer: RuntimeContextAssertionIssuer,
    ) -> Self {
        self.with_runtime_context_assertion_provider(assertion_issuer)
    }

    pub fn with_runtime_context_assertion_provider<P>(mut self, assertion_provider: P) -> Self
    where
        P: RuntimeContextAssertionProvider + 'static,
    {
        self.assertion_provider = Some(Arc::new(assertion_provider));
        self
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
        let identity = self.thread_identity_for_record(&input);
        let project_partition_key = record_project_partition_key(&input, &identity)?;
        self.ensure_thread(tenant_id.as_str(), thread_id.as_str(), &identity)
            .await?;
        let record = self.upsert_record_input(input)?;
        let records = vec![record];
        let runtime_context_assertion = self
            .issue_assertion(
                tenant_id.as_str(),
                thread_id.as_str(),
                &identity,
                project_partition_key.as_str(),
            )
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
            "memory engine record batch sync start"
        );
        let response = self
            .client
            .ingest_thread_records(
                thread_id.as_str(),
                &SdkBatchSyncRecordsRequest {
                    tenant_id,
                    project_partition_key,
                    runtime_context_assertion,
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

        let mut batches: BTreeMap<
            (String, String, String),
            (RecordThreadIdentity, Vec<UpsertRecordInput>),
        > = BTreeMap::new();
        for input in inputs {
            let input: SaveRecordInput = input.into();
            let tenant_id = self.tenant_id_for_record(&input)?;
            let thread_id = self.thread_id_for_record(&input)?;
            let identity = self.thread_identity_for_record(&input);
            let project_partition_key = record_project_partition_key(&input, &identity)?;
            self.ensure_thread(tenant_id.as_str(), thread_id.as_str(), &identity)
                .await?;
            let record = self.upsert_record_input(input)?;
            let entry = batches
                .entry((tenant_id, thread_id, project_partition_key))
                .or_insert_with(|| (identity.clone(), Vec::new()));
            if entry.0 != identity {
                return Err("memory record batch contains conflicting thread identity".to_string());
            }
            entry.1.push(record);
        }

        for ((tenant_id, thread_id, project_partition_key), (identity, records)) in batches {
            let runtime_context_assertion = self
                .issue_assertion(
                    tenant_id.as_str(),
                    thread_id.as_str(),
                    &identity,
                    project_partition_key.as_str(),
                )
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
                .ingest_thread_records(
                    thread_id.as_str(),
                    &SdkBatchSyncRecordsRequest {
                        tenant_id: tenant_id.clone(),
                        project_partition_key,
                        runtime_context_assertion,
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
    async fn issue_assertion(
        &self,
        tenant_id: &str,
        thread_id: &str,
        identity: &RecordThreadIdentity,
        project_partition_key: &str,
    ) -> Result<String, String> {
        let provider = self.assertion_provider.as_ref().ok_or_else(|| {
            "Memory runtime context assertion provider is not configured".to_string()
        })?;
        let source_id = self
            .source_id
            .as_deref()
            .and_then(normalized)
            .ok_or_else(|| "Memory record writer source_id is required".to_string())?;
        let agent_id = identity
            .agent_id
            .as_deref()
            .and_then(normalized)
            .ok_or_else(|| "memory thread requires an explicit agent_id".to_string())?;
        let canonical_agent_subject_id = format!("agent:{agent_id}");
        provider
            .issue(RuntimeContextAssertionRequest {
                principal_id: tenant_id,
                tenant_id,
                source_id: source_id.as_str(),
                canonical_agent_subject_id: canonical_agent_subject_id.as_str(),
                thread_id,
                project_partition_key,
                operation: "record_ingest",
            })
            .await
    }

    async fn ensure_thread(
        &self,
        tenant_id: &str,
        thread_id: &str,
        identity: &RecordThreadIdentity,
    ) -> Result<(), String> {
        let key = (tenant_id.to_string(), thread_id.to_string());
        let agent_id = identity
            .agent_id
            .as_deref()
            .ok_or_else(|| "memory thread requires an explicit agent_id".to_string())?;
        if let Some(ensured) = self.ensured_threads.lock().await.get(&key).cloned() {
            if ensured.agent_id != identity.agent_id
                || ensured.project_aggregation_id != identity.project_aggregation_id
                || ensured.thread_kind != identity.thread_kind
            {
                return Err("memory thread identity changed after it was established".to_string());
            }
            if ensured.agent_name == identity.agent_name {
                return Ok(());
            }
        }
        upsert_canonical_memory_thread(
            &self.client,
            tenant_id,
            thread_id,
            agent_id,
            identity.agent_name.as_deref(),
            identity.project_aggregation_id.as_deref(),
            identity.thread_kind.as_str(),
        )
        .await?;
        self.ensured_threads
            .lock()
            .await
            .insert(key, identity.clone());
        Ok(())
    }

    fn thread_identity_for_record(&self, input: &SaveRecordInput) -> RecordThreadIdentity {
        let metadata = input.metadata.as_ref();
        let value = |key: &str| {
            metadata
                .and_then(|item| item.get(key))
                .and_then(serde_json::Value::as_str)
                .and_then(normalized)
        };
        let agent_id = self
            .scope
            .agent_id
            .as_deref()
            .and_then(normalized)
            .or_else(|| value("agent_id"));
        RecordThreadIdentity {
            agent_id,
            agent_name: value("agent_name"),
            project_aggregation_id: self
                .scope
                .project_aggregation_id
                .as_deref()
                .and_then(normalized)
                .or_else(|| value("thread_project_aggregation_id")),
            thread_kind: self
                .scope
                .thread_kind
                .as_deref()
                .and_then(normalized)
                .or_else(|| value("thread_kind"))
                .or_else(|| input.message_mode.as_deref().and_then(normalized))
                .unwrap_or_else(|| "conversation".to_string()),
        }
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
        let projection_event = input
            .metadata
            .as_ref()
            .and_then(|value| value.get("memory_projection_mutable"))
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        if projection_event
            && (input.created_at.as_deref().and_then(normalized).is_none()
                || input.message_id.as_deref().and_then(normalized).is_none())
        {
            return Err(
                "Memory projection events require a durable message_id and created_at".to_string(),
            );
        }
        let projection_scope = projection_event
            .then(|| {
                Ok::<_, String>((
                    self.tenant_id_for_record(&input)?,
                    self.source_id.clone(),
                    self.thread_id_for_record(&input)?,
                ))
            })
            .transpose()?;
        let role = normalized(input.role.as_str())
            .ok_or_else(|| "memory record role is required".to_string())?;
        let record_type =
            normalized(self.scope.record_type.as_str()).unwrap_or_else(|| "message".to_string());
        let metadata = input.packed_metadata();
        let mut record = UpsertRecordInput {
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
        };
        if projection_event {
            // UI messages update in place; each published state transition is an
            // immutable Memory event. Replaying one snapshot must keep the same ID.
            if let Some(metadata) = record
                .metadata
                .as_mut()
                .and_then(serde_json::Value::as_object_mut)
            {
                metadata.remove("memory_projection_mutable");
                metadata.insert(
                    "ui_message_id".to_string(),
                    serde_json::Value::String(record.id.clone()),
                );
                metadata.insert(
                    "memory_projection_event".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
            let mut identity = serde_json::json!([
                projection_scope,
                record.id,
                record.role,
                record.record_type,
                record.content,
                record.structured_payload,
                record.metadata,
                record.created_at
            ]);
            identity.sort_all_objects();
            let identity = serde_json::to_vec(&identity).map_err(|error| {
                format!("encode Memory projection event identity failed: {error}")
            })?;
            record.id = format!("projection-event:{}", hex::encode(Sha256::digest(identity)));
        }
        Ok(record)
    }
}

fn record_project_partition_key(
    input: &SaveRecordInput,
    identity: &RecordThreadIdentity,
) -> Result<String, String> {
    let bound_partition = canonical_project_partition(identity.project_aggregation_id.as_deref())?;
    // Agent threads span projects. The runtime freezes the current record's project
    // separately from the optional project that owns an entire task thread.
    let declared = input
        .metadata
        .as_ref()
        .and_then(|value| value.get("memory_project_id"));
    let partition = match declared {
        Some(serde_json::Value::String(value)) => canonical_project_partition(Some(value))?,
        Some(serde_json::Value::Null) => "__none__".to_string(),
        None => bound_partition.clone(),
        Some(_) => {
            return Err("memory record memory_project_id must be a string or null".to_string())
        }
    };
    if bound_partition != "__none__" && bound_partition != partition {
        return Err("memory record project does not match its task thread project".to_string());
    }
    Ok(partition)
}

fn canonical_project_partition(value: Option<&str>) -> Result<String, String> {
    let Some(value) = value else {
        return Ok("__none__".to_string());
    };
    let value = value.trim();
    let project_id = value.strip_prefix("project:").unwrap_or(value);
    if project_id.is_empty() || project_id.len() > 1_024 || project_id.chars().any(char::is_control)
    {
        return Err("memory record project identity is invalid".to_string());
    }
    Ok(format!("project:{project_id}"))
}
