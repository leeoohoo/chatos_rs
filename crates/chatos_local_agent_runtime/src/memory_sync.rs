// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chatos_client_storage::{
    AgentMessageStateRecord, ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, SyncOutboxStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, MemorySyncStatus, MessageMode, SyncDestination, SyncOutboxItem,
    SyncOutboxStatus,
};
use chrono::{DateTime, Utc};
use memory_engine_sdk::{MemoryEngineClient, SdkBatchSyncRecordsRequest, UpsertRecordInput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::ui_events::append_memory_sync_status;
use crate::{digest::canonical_json_digest, digest::stable_digest_id, pagination::advance_cursor};

#[derive(Debug, Clone)]
pub struct RecordSemanticMessageRequest {
    pub scope: RecordScope,
    pub message: AgentMessage,
    pub origin_device_id: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedSemanticMessage {
    pub message: AgentMessageStateRecord,
    pub outbox: SyncOutboxStateRecord,
}

/// Persists a semantic message and its Memory Engine outbox item in one
/// transaction. Provider-only continuation items are intentionally rejected.
pub async fn record_semantic_message(
    storage: &dyn ClientStorage,
    request: RecordSemanticMessageRequest,
) -> StorageResult<RecordedSemanticMessage> {
    let mut operation = RecordSemanticMessageOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "semantic message transaction returned no result".to_string(),
    })
}

struct RecordSemanticMessageOperation {
    request: Option<RecordSemanticMessageRequest>,
    result: Option<RecordedSemanticMessage>,
}

#[async_trait]
impl StorageTransaction for RecordSemanticMessageOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "semantic message request was already consumed".to_string(),
        })?;
        let event_scope = request.scope.clone();
        let event_origin_device_id = request.origin_device_id.clone();
        request
            .message
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("invalid semantic message: {error}"),
            })?;
        if request.message.message_mode != MessageMode::Semantic
            || request.message.memory_sync_status != MemorySyncStatus::Pending
        {
            return invalid_data(
                "only pending semantic messages can create Memory Engine outbox records",
            );
        }
        if request.origin_device_id.trim().is_empty() {
            return invalid_data("semantic message origin device must not be empty");
        }
        let remote_record = MemorySyncRecord::from_message(&request.message)?;
        let payload_digest = remote_record.digest()?;
        let outbox_id = stable_digest_id(
            "memory",
            &[
                request.scope.owner_user_id.as_str(),
                request.message.record_id.as_str(),
            ],
        );
        let message_record = AgentMessageStateRecord {
            metadata: RecordMetadata {
                id: request.message.record_id.clone(),
                scope: request.scope.clone(),
                origin_device_id: request.origin_device_id.clone(),
                revision: 0,
                created_at: request.now,
                updated_at: request.now,
            },
            message: request.message,
        };
        let outbox_record = SyncOutboxStateRecord {
            metadata: RecordMetadata {
                id: outbox_id.clone(),
                scope: request.scope.clone(),
                origin_device_id: request.origin_device_id,
                revision: 0,
                created_at: request.now,
                updated_at: request.now,
            },
            item: SyncOutboxItem {
                outbox_id,
                destination: SyncDestination::MemoryEngine,
                record_id: message_record.message.record_id.clone(),
                payload_digest,
                status: SyncOutboxStatus::Pending,
                attempt_count: 0,
                available_at: request.now,
                last_error: None,
            },
        };
        let (stored_message, message_created) =
            put_message_idempotently(repositories, message_record).await?;
        let (stored_outbox, outbox_created) =
            put_outbox_idempotently(repositories, outbox_record).await?;
        if message_created || outbox_created {
            append_memory_sync_status(repositories, &event_scope, &event_origin_device_id).await?;
        }
        self.result = Some(RecordedSemanticMessage {
            message: stored_message,
            outbox: stored_outbox,
        });
        Ok(())
    }
}

async fn put_message_idempotently(
    repositories: &mut dyn TransactionRepositories,
    record: AgentMessageStateRecord,
) -> StorageResult<(AgentMessageStateRecord, bool)> {
    let query = RecordQuery {
        scope: record.metadata.scope.clone(),
        id: record.metadata.id.clone(),
    };
    let existing = {
        let mut messages = repositories.agent_messages();
        messages.get(&query).await?
    };
    if let Some(existing) = existing {
        if MemorySyncRecord::from_message(&existing.message)?.digest()?
            == MemorySyncRecord::from_message(&record.message)?.digest()?
        {
            return Ok((existing, false));
        }
        return Err(StorageError::Conflict {
            actual_revision: existing.metadata.revision,
        });
    }
    let stored = repositories
        .agent_messages()
        .put(PutRecord {
            record,
            expected_revision: None,
        })
        .await?;
    Ok((stored, true))
}

async fn put_outbox_idempotently(
    repositories: &mut dyn TransactionRepositories,
    record: SyncOutboxStateRecord,
) -> StorageResult<(SyncOutboxStateRecord, bool)> {
    let query = RecordQuery {
        scope: record.metadata.scope.clone(),
        id: record.metadata.id.clone(),
    };
    let existing = {
        let mut outbox = repositories.sync_outbox();
        outbox.get(&query).await?
    };
    if let Some(existing) = existing {
        if existing.item.destination == record.item.destination
            && existing.item.record_id == record.item.record_id
            && existing.item.payload_digest == record.item.payload_digest
        {
            return Ok((existing, false));
        }
        return Err(StorageError::Conflict {
            actual_revision: existing.metadata.revision,
        });
    }
    let stored = repositories
        .sync_outbox()
        .put(PutRecord {
            record,
            expected_revision: None,
        })
        .await?;
    Ok((stored, true))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySyncPolicy {
    pub batch_limit: u32,
    pub lease_duration: Duration,
    pub maximum_attempts: u32,
    pub base_retry_delay: Duration,
    pub maximum_retry_delay: Duration,
}

impl Default for MemorySyncPolicy {
    fn default() -> Self {
        Self {
            batch_limit: 100,
            lease_duration: Duration::from_secs(2 * 60),
            maximum_attempts: 8,
            base_retry_delay: Duration::from_secs(2),
            maximum_retry_delay: Duration::from_secs(5 * 60),
        }
    }
}

impl MemorySyncPolicy {
    fn validate(self) -> StorageResult<Self> {
        if self.batch_limit == 0
            || self.batch_limit > ListQuery::MAX_LIMIT
            || self.lease_duration.is_zero()
            || self.maximum_attempts == 0
            || self.base_retry_delay.is_zero()
            || self.maximum_retry_delay < self.base_retry_delay
        {
            return invalid_data("Memory Sync policy values are invalid");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct ClaimMemorySyncBatchRequest {
    pub scope: RecordScope,
    pub now: DateTime<Utc>,
    pub policy: MemorySyncPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedMemorySyncRecord {
    pub outbox: SyncOutboxStateRecord,
    pub message: AgentMessageStateRecord,
    pub remote_record: MemorySyncRecord,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedMemorySyncBatch {
    pub records: Vec<ClaimedMemorySyncRecord>,
    pub exhausted_outbox_ids: Vec<String>,
}

pub async fn claim_memory_sync_batch(
    storage: &dyn ClientStorage,
    request: ClaimMemorySyncBatchRequest,
) -> StorageResult<ClaimedMemorySyncBatch> {
    request.policy.validate()?;
    let mut operation = ClaimMemorySyncBatchOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "Memory Sync claim returned no batch".to_string(),
    })
}

struct ClaimMemorySyncBatchOperation {
    request: Option<ClaimMemorySyncBatchRequest>,
    result: Option<ClaimedMemorySyncBatch>,
}

#[async_trait]
impl StorageTransaction for ClaimMemorySyncBatchOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "Memory Sync claim request was already consumed".to_string(),
        })?;
        let lease_duration =
            chrono::Duration::from_std(request.policy.lease_duration).map_err(|_| {
                StorageError::InvalidData {
                    reason: "Memory Sync lease duration cannot be represented".to_string(),
                }
            })?;
        let lease_until = request
            .now
            .checked_add_signed(lease_duration)
            .ok_or_else(|| StorageError::InvalidData {
                reason: "Memory Sync lease deadline overflow".to_string(),
            })?;
        let mut candidates = list_due_outbox(repositories, &request.scope, request.now).await?;
        candidates.sort_by(|left, right| {
            left.item
                .available_at
                .cmp(&right.item.available_at)
                .then_with(|| left.item.outbox_id.cmp(&right.item.outbox_id))
        });
        candidates.truncate(request.policy.batch_limit as usize);

        let mut claimed = Vec::new();
        let mut exhausted = Vec::new();
        let mut event_origin_device_id = None;
        for mut outbox in candidates {
            let message_query = RecordQuery {
                scope: request.scope.clone(),
                id: outbox.item.record_id.clone(),
            };
            let mut message = repositories
                .agent_messages()
                .get(&message_query)
                .await?
                .ok_or(StorageError::NotFound)?;
            if message.message.message_mode != MessageMode::Semantic {
                return invalid_data("Memory Sync outbox points to a provider-only message");
            }
            let remote_record = MemorySyncRecord::from_message(&message.message)?;
            if remote_record.digest()? != outbox.item.payload_digest {
                return invalid_data("Memory Sync outbox payload digest no longer matches message");
            }
            if outbox.item.attempt_count >= request.policy.maximum_attempts {
                event_origin_device_id
                    .get_or_insert_with(|| outbox.metadata.origin_device_id.clone());
                let outbox_revision = outbox.metadata.revision;
                outbox.item.status = SyncOutboxStatus::Failed;
                outbox.item.last_error = Some("memory_sync_attempt_limit_exceeded".to_string());
                repositories
                    .sync_outbox()
                    .put(PutRecord {
                        record: outbox.clone(),
                        expected_revision: Some(outbox_revision),
                    })
                    .await?;
                let message_revision = message.metadata.revision;
                message.message.memory_sync_status = MemorySyncStatus::Failed;
                repositories
                    .agent_messages()
                    .put(PutRecord {
                        record: message,
                        expected_revision: Some(message_revision),
                    })
                    .await?;
                exhausted.push(outbox.item.outbox_id);
                continue;
            }
            let revision = outbox.metadata.revision;
            event_origin_device_id.get_or_insert_with(|| outbox.metadata.origin_device_id.clone());
            outbox.item.status = SyncOutboxStatus::InFlight;
            outbox.item.attempt_count =
                outbox.item.attempt_count.checked_add(1).ok_or_else(|| {
                    StorageError::InvalidData {
                        reason: "Memory Sync attempt count overflow".to_string(),
                    }
                })?;
            outbox.item.available_at = lease_until;
            outbox.item.last_error = None;
            outbox = repositories
                .sync_outbox()
                .put(PutRecord {
                    record: outbox,
                    expected_revision: Some(revision),
                })
                .await?;
            claimed.push(ClaimedMemorySyncRecord {
                outbox,
                message,
                remote_record,
            });
        }
        if let Some(origin_device_id) = event_origin_device_id {
            append_memory_sync_status(repositories, &request.scope, &origin_device_id).await?;
        }
        self.result = Some(ClaimedMemorySyncBatch {
            records: claimed,
            exhausted_outbox_ids: exhausted,
        });
        Ok(())
    }
}

async fn list_due_outbox(
    repositories: &mut dyn TransactionRepositories,
    scope: &RecordScope,
    now: DateTime<Utc>,
) -> StorageResult<Vec<SyncOutboxStateRecord>> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = repositories
            .sync_outbox()
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        records.extend(page.records.into_iter().filter(|record| {
            matches!(
                record.item.status,
                SyncOutboxStatus::Pending | SyncOutboxStatus::InFlight
            ) && record.item.available_at <= now
        }));
        if !advance_cursor(&mut cursor, page.next_cursor)? {
            break;
        }
    }
    Ok(records)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MemorySyncRecord {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Value,
    pub created_at: String,
}

impl MemorySyncRecord {
    fn from_message(message: &AgentMessage) -> StorageResult<Self> {
        message
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: format!("invalid Memory Sync message: {error}"),
            })?;
        let role = match message.role {
            AgentMessageRole::System => "system",
            AgentMessageRole::User => "user",
            AgentMessageRole::Assistant => "assistant",
            AgentMessageRole::Tool => "tool",
        };
        let content = message
            .content
            .clone()
            .filter(|content| !content.is_empty())
            .or_else(|| {
                message
                    .reasoning
                    .clone()
                    .filter(|reasoning| !reasoning.is_empty())
            })
            .or_else(|| message.structured_payload.as_ref().map(Value::to_string))
            .ok_or_else(|| StorageError::InvalidData {
                reason: "Memory Sync message has no semantic content".to_string(),
            })?;
        let structured_payload =
            if message.reasoning.is_some() || message.structured_payload.is_some() {
                Some(json!({
                    "payload": message.structured_payload,
                    "reasoning": message.reasoning,
                }))
            } else {
                None
            };
        Ok(Self {
            id: message.record_id.clone(),
            thread_id: message.thread_id.clone(),
            role: role.to_string(),
            record_type: "message".to_string(),
            content,
            structured_payload,
            metadata: json!({
                "run_id": message.run_id,
                "turn_id": message.turn_id,
                "sequence": message.sequence,
                "tool_call_id": message.tool_call_id,
                "response_id": message.response_id,
                "message_source": message.message_source,
            }),
            created_at: message.created_at.to_rfc3339(),
        })
    }

    fn digest(&self) -> StorageResult<String> {
        let value = serde_json::to_value(self).map_err(|error| StorageError::InvalidData {
            reason: format!("Memory Sync record cannot be serialized: {error}"),
        })?;
        canonical_json_digest(&value)
    }

    fn into_sdk(self) -> UpsertRecordInput {
        UpsertRecordInput {
            id: self.id.clone(),
            external_record_id: Some(self.id),
            role: self.role,
            record_type: self.record_type,
            content: self.content,
            structured_payload: self.structured_payload,
            metadata: Some(self.metadata),
            summary_status: Some("pending".to_string()),
            summary_id: None,
            summarized_at: None,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemorySyncApiRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub records: Vec<MemorySyncRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySyncApiReceipt {
    pub thread_id: String,
    pub received_count: usize,
    pub upserted_count: usize,
}

#[async_trait]
pub trait MemorySyncApi: Send + Sync {
    async fn batch_sync(
        &self,
        request: MemorySyncApiRequest,
        cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String>;
}

#[async_trait]
impl MemorySyncApi for MemoryEngineClient {
    async fn batch_sync(
        &self,
        request: MemorySyncApiRequest,
        cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String> {
        let record_count = request.records.len();
        let engine_request = SdkBatchSyncRecordsRequest {
            tenant_id: request.tenant_id,
            records: request
                .records
                .into_iter()
                .map(MemorySyncRecord::into_sdk)
                .collect(),
        };
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("memory_sync_cancelled".to_string()),
            response = self.batch_sync_records(request.thread_id.as_str(), &engine_request) => response?,
        };
        if response.thread_id != request.thread_id || response.received_count != record_count {
            return Err(
                "Memory Engine batch acknowledgement does not match the request".to_string(),
            );
        }
        Ok(MemorySyncApiReceipt {
            thread_id: response.thread_id,
            received_count: response.received_count,
            upserted_count: response.upserted_count,
        })
    }
}

#[derive(Debug, Clone)]
struct CompleteMemorySyncRequest {
    scope: RecordScope,
    claims: Vec<MemorySyncClaimRef>,
    result: Result<(), String>,
    retry_at: DateTime<Utc>,
    maximum_attempts: u32,
}

#[derive(Debug, Clone)]
struct MemorySyncClaimRef {
    outbox_id: String,
    attempt_count: u32,
}

async fn complete_memory_sync(
    storage: &dyn ClientStorage,
    request: CompleteMemorySyncRequest,
) -> StorageResult<MemorySyncCompletion> {
    let mut operation = CompleteMemorySyncOperation {
        request: Some(request),
        result: None,
    };
    storage.transaction(&mut operation).await?;
    operation.result.ok_or(StorageError::Transaction {
        reason: "Memory Sync completion returned no result".to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MemorySyncCompletion {
    synced: usize,
    deferred: usize,
    permanently_failed: usize,
}

struct CompleteMemorySyncOperation {
    request: Option<CompleteMemorySyncRequest>,
    result: Option<MemorySyncCompletion>,
}

#[async_trait]
impl StorageTransaction for CompleteMemorySyncOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let request = self.request.take().ok_or(StorageError::Transaction {
            reason: "Memory Sync completion request was already consumed".to_string(),
        })?;
        let event_scope = request.scope.clone();
        let mut event_origin_device_id = None;
        let mut completion = MemorySyncCompletion::default();
        for claim in request.claims {
            let query = RecordQuery {
                scope: request.scope.clone(),
                id: claim.outbox_id,
            };
            let mut outbox = repositories
                .sync_outbox()
                .get(&query)
                .await?
                .ok_or(StorageError::NotFound)?;
            event_origin_device_id.get_or_insert_with(|| outbox.metadata.origin_device_id.clone());
            if outbox.item.status != SyncOutboxStatus::InFlight
                || outbox.item.attempt_count != claim.attempt_count
            {
                return invalid_data("stale Memory Sync attempt cannot complete an outbox item");
            }
            let message_query = RecordQuery {
                scope: request.scope.clone(),
                id: outbox.item.record_id.clone(),
            };
            let mut message = repositories
                .agent_messages()
                .get(&message_query)
                .await?
                .ok_or(StorageError::NotFound)?;
            let outbox_revision = outbox.metadata.revision;
            let message_revision = message.metadata.revision;
            match &request.result {
                Ok(()) => {
                    outbox.item.status = SyncOutboxStatus::Succeeded;
                    outbox.item.last_error = None;
                    message.message.memory_sync_status = MemorySyncStatus::Synced;
                    completion.synced += 1;
                }
                Err(error) if outbox.item.attempt_count >= request.maximum_attempts => {
                    outbox.item.status = SyncOutboxStatus::Failed;
                    outbox.item.last_error = Some(bounded_error(error));
                    message.message.memory_sync_status = MemorySyncStatus::Failed;
                    completion.permanently_failed += 1;
                }
                Err(error) => {
                    outbox.item.status = SyncOutboxStatus::Pending;
                    outbox.item.available_at = request.retry_at;
                    outbox.item.last_error = Some(bounded_error(error));
                    message.message.memory_sync_status = MemorySyncStatus::Pending;
                    completion.deferred += 1;
                }
            }
            repositories
                .sync_outbox()
                .put(PutRecord {
                    record: outbox,
                    expected_revision: Some(outbox_revision),
                })
                .await?;
            repositories
                .agent_messages()
                .put(PutRecord {
                    record: message,
                    expected_revision: Some(message_revision),
                })
                .await?;
        }
        if let Some(origin_device_id) = event_origin_device_id {
            append_memory_sync_status(repositories, &event_scope, &origin_device_id).await?;
        }
        self.result = Some(completion);
        Ok(())
    }
}

fn bounded_error(error: &str) -> String {
    const MAX_ERROR_BYTES: usize = 2_048;
    if error.len() <= MAX_ERROR_BYTES {
        return error.to_string();
    }
    let mut end = MAX_ERROR_BYTES;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_string()
}

#[derive(Clone)]
pub struct MemorySynchronizer {
    api: Arc<dyn MemorySyncApi>,
    tenant_id: String,
    source_id: String,
    policy: MemorySyncPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemorySyncRunReport {
    pub claimed: usize,
    pub synced: usize,
    pub deferred: usize,
    pub permanently_failed: usize,
    pub exhausted_before_send: usize,
    pub errors: Vec<String>,
}

impl MemorySynchronizer {
    pub fn new(
        api: Arc<dyn MemorySyncApi>,
        tenant_id: impl Into<String>,
        source_id: impl Into<String>,
        policy: MemorySyncPolicy,
    ) -> Result<Self, String> {
        let tenant_id = tenant_id.into();
        let source_id = source_id.into();
        if tenant_id.trim().is_empty() || source_id.trim().is_empty() {
            return Err("Memory Sync tenant and source identifiers are required".to_string());
        }
        policy.validate().map_err(|error| error.to_string())?;
        Ok(Self {
            api,
            tenant_id,
            source_id,
            policy,
        })
    }

    /// Claims and sends at most one bounded batch. It never owns a polling
    /// loop; the Host or platform scheduler decides when to invoke it again.
    pub async fn sync_once(
        &self,
        storage: &dyn ClientStorage,
        scope: RecordScope,
        now: DateTime<Utc>,
        cancellation: CancellationToken,
    ) -> StorageResult<MemorySyncRunReport> {
        let claimed = claim_memory_sync_batch(
            storage,
            ClaimMemorySyncBatchRequest {
                scope: scope.clone(),
                now,
                policy: self.policy,
            },
        )
        .await?;
        let mut report = MemorySyncRunReport {
            claimed: claimed.records.len(),
            exhausted_before_send: claimed.exhausted_outbox_ids.len(),
            ..MemorySyncRunReport::default()
        };
        let mut groups = BTreeMap::<String, Vec<ClaimedMemorySyncRecord>>::new();
        for record in claimed.records {
            groups
                .entry(record.message.message.thread_id.clone())
                .or_default()
                .push(record);
        }
        for (thread_id, records) in groups {
            let claims = records
                .iter()
                .map(|record| MemorySyncClaimRef {
                    outbox_id: record.outbox.item.outbox_id.clone(),
                    attempt_count: record.outbox.item.attempt_count,
                })
                .collect::<Vec<_>>();
            let attempt = records
                .iter()
                .map(|record| record.outbox.item.attempt_count)
                .max()
                .unwrap_or(1);
            let api_result = self
                .api
                .batch_sync(
                    MemorySyncApiRequest {
                        tenant_id: self.tenant_id.clone(),
                        source_id: self.source_id.clone(),
                        thread_id,
                        records: records
                            .into_iter()
                            .map(|record| record.remote_record)
                            .collect(),
                    },
                    cancellation.clone(),
                )
                .await
                .and_then(|receipt| {
                    if receipt.received_count == claims.len() {
                        Ok(())
                    } else {
                        Err("Memory Engine acknowledged a partial batch".to_string())
                    }
                });
            let retry_at = retry_deadline(now, self.policy, attempt)?;
            if let Err(error) = &api_result {
                report.errors.push(error.clone());
            }
            let completion = complete_memory_sync(
                storage,
                CompleteMemorySyncRequest {
                    scope: scope.clone(),
                    claims,
                    result: api_result,
                    retry_at,
                    maximum_attempts: self.policy.maximum_attempts,
                },
            )
            .await?;
            report.synced += completion.synced;
            report.deferred += completion.deferred;
            report.permanently_failed += completion.permanently_failed;
        }
        Ok(report)
    }
}

fn retry_deadline(
    now: DateTime<Utc>,
    policy: MemorySyncPolicy,
    attempt: u32,
) -> StorageResult<DateTime<Utc>> {
    let exponent = attempt.saturating_sub(1).min(31);
    let factor = 1u32 << exponent;
    let delay = policy
        .base_retry_delay
        .checked_mul(factor)
        .unwrap_or(policy.maximum_retry_delay)
        .min(policy.maximum_retry_delay);
    let delay = chrono::Duration::from_std(delay).map_err(|_| StorageError::InvalidData {
        reason: "Memory Sync retry delay cannot be represented".to_string(),
    })?;
    now.checked_add_signed(delay)
        .ok_or_else(|| StorageError::InvalidData {
            reason: "Memory Sync retry deadline overflow".to_string(),
        })
}

fn invalid_data<T>(reason: impl Into<String>) -> StorageResult<T> {
    Err(StorageError::InvalidData {
        reason: reason.into(),
    })
}
