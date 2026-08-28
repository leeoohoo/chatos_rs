// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use chatos_mcp_service::{McpToolCallCommand, McpToolCallResult, McpToolCallResultItem};
use futures_util::{StreamExt, TryStreamExt};
use mongodb::bson::{doc, DateTime};
use mongodb::error::{ErrorKind as MongoErrorKind, WriteFailure};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const MAX_CAS_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolBatchStatus {
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum RuntimeToolBatchPendingEvent {
    InvocationReady { call_index: usize },
    AggregateResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolBatchRecord {
    #[serde(rename = "_id")]
    pub batch_id: String,
    pub command: McpToolCallCommand,
    pub session_id: String,
    pub status: RuntimeToolBatchStatus,
    pub next_call_index: usize,
    pub items: Vec<Option<McpToolCallResultItem>>,
    pub invocation_ids: Vec<String>,
    pub waiting_user_prompt_ids: Vec<Option<String>>,
    pub pending_event: Option<RuntimeToolBatchPendingEvent>,
    pub revision: i64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub expires_at: DateTime,
    pub expires_at_unix: i64,
}

impl RuntimeToolBatchRecord {
    pub fn aggregate_result(&self) -> Option<McpToolCallResult> {
        if self.status != RuntimeToolBatchStatus::Completed {
            return None;
        }
        let items = self.items.iter().cloned().collect::<Option<Vec<_>>>()?;
        Some(McpToolCallResult {
            event_id: format!("mcp_batch_result_{}", self.batch_id),
            owner_service: self.command.owner_service.clone(),
            agent_run_id: self.command.agent_run_id.clone(),
            agent_key: self.command.agent_key.clone(),
            ordering_lane_key: self.command.ordering_lane_key.clone(),
            lane_seq: self.command.lane_seq,
            generation: self.command.generation,
            source_step_seq: self.command.source_step_seq,
            batch_id: self.batch_id.clone(),
            session_id: self.session_id.clone(),
            items,
        })
    }

    fn normalize_progress(&mut self) {
        while self.next_call_index < self.items.len() && self.items[self.next_call_index].is_some()
        {
            self.next_call_index += 1;
        }
        if self.next_call_index == self.items.len() {
            self.status = RuntimeToolBatchStatus::Completed;
            self.pending_event = Some(RuntimeToolBatchPendingEvent::AggregateResult);
        } else {
            self.status = RuntimeToolBatchStatus::Active;
            self.pending_event = Some(RuntimeToolBatchPendingEvent::InvocationReady {
                call_index: self.next_call_index,
            });
        }
    }
}

#[derive(Clone)]
pub struct RuntimeToolBatchStore {
    backend: Arc<RuntimeToolBatchStoreBackend>,
}

enum RuntimeToolBatchStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeToolBatchRecord>>),
    Mongo(Collection<RuntimeToolBatchRecord>),
}

impl RuntimeToolBatchStore {
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(RuntimeToolBatchStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
        }
    }

    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect MCP tool batch MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection =
            database.collection::<RuntimeToolBatchRecord>("mcp_management_runtime_tool_batches");
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_tool_batch_expiry_ttl".to_string())
                            .expire_after(Some(std::time::Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("initialize Runtime Tool Batch TTL index failed: {error}"))?;
        for (name, keys) in [
            (
                "runtime_tool_batch_pending_event",
                doc! { "pending_event": 1, "updated_at_unix_ms": 1 },
            ),
            (
                "runtime_tool_batch_invocation",
                doc! { "invocation_ids": 1, "expires_at_unix": 1 },
            ),
            (
                "runtime_tool_batch_waiting_user_prompt",
                doc! { "waiting_user_prompt_ids": 1, "expires_at_unix": 1 },
            ),
        ] {
            collection
                .create_index(
                    IndexModel::builder()
                        .keys(keys)
                        .options(IndexOptions::builder().name(name.to_string()).build())
                        .build(),
                    None,
                )
                .await
                .map_err(|error| {
                    format!("initialize Runtime Tool Batch index {name} failed: {error}")
                })?;
        }
        Ok(Self {
            backend: Arc::new(RuntimeToolBatchStoreBackend::Mongo(collection)),
        })
    }

    pub async fn insert_or_get(
        &self,
        mut record: RuntimeToolBatchRecord,
    ) -> Result<RuntimeToolBatchRecord, String> {
        record.normalize_progress();
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => {
                let mut records = records.write().await;
                if let Some(existing) = records.get(record.batch_id.as_str()) {
                    ensure_same_command(existing, &record)?;
                    return Ok(existing.clone());
                }
                records.insert(record.batch_id.clone(), record.clone());
                Ok(record)
            }
            RuntimeToolBatchStoreBackend::Mongo(collection) => {
                match collection.insert_one(record.clone(), None).await {
                    Ok(_) => Ok(record),
                    Err(error) if is_duplicate_key(&error) => {
                        let existing = collection
                            .find_one(doc! { "_id": record.batch_id.as_str() }, None)
                            .await
                            .map_err(|error| format!("load duplicate Runtime Tool Batch failed: {error}"))?
                            .ok_or_else(|| {
                                "MongoDB reported a duplicate Runtime Tool Batch without returning it"
                                    .to_string()
                            })?;
                        ensure_same_command(&existing, &record)?;
                        Ok(existing)
                    }
                    Err(error) => Err(format!("insert Runtime Tool Batch failed: {error}")),
                }
            }
        }
    }

    pub async fn get(&self, batch_id: &str) -> Result<Option<RuntimeToolBatchRecord>, String> {
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => {
                Ok(records.read().await.get(batch_id).cloned())
            }
            RuntimeToolBatchStoreBackend::Mongo(collection) => collection
                .find_one(doc! { "_id": batch_id }, None)
                .await
                .map_err(|error| format!("load Runtime Tool Batch failed: {error}")),
        }
    }

    pub async fn find_by_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<Option<RuntimeToolBatchRecord>, String> {
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => Ok(records
                .read()
                .await
                .values()
                .find(|record| record.invocation_ids.iter().any(|id| id == invocation_id))
                .cloned()),
            RuntimeToolBatchStoreBackend::Mongo(collection) => collection
                .find_one(doc! { "invocation_ids": invocation_id }, None)
                .await
                .map_err(|error| format!("load Runtime Tool Batch by invocation failed: {error}")),
        }
    }

    pub async fn record_terminal_item(
        &self,
        batch_id: &str,
        call_index: usize,
        item: McpToolCallResultItem,
    ) -> Result<RuntimeToolBatchRecord, String> {
        self.mutate(batch_id, move |record| {
            if call_index >= record.items.len() {
                return Err("Runtime Tool Batch call_index is out of range".to_string());
            }
            if let Some(existing) = record.items[call_index].as_ref() {
                if existing != &item {
                    return Err(
                        "Runtime Tool Batch terminal item conflicts with persisted result"
                            .to_string(),
                    );
                }
                return Ok(false);
            }
            if record.next_call_index != call_index {
                return Err(format!(
                    "Runtime Tool Batch expected call {} but received terminal call {call_index}",
                    record.next_call_index
                ));
            }
            record.items[call_index] = Some(item.clone());
            record.normalize_progress();
            Ok(true)
        })
        .await
    }

    pub async fn mark_waiting_for_user(
        &self,
        batch_id: &str,
        call_index: usize,
        prompt_id: String,
    ) -> Result<RuntimeToolBatchRecord, String> {
        self.mutate(batch_id, move |record| {
            if call_index >= record.waiting_user_prompt_ids.len()
                || record.next_call_index != call_index
            {
                return Err("Runtime Tool Batch waiting-user call_index is invalid".to_string());
            }
            if let Some(existing) = record.waiting_user_prompt_ids[call_index].as_ref() {
                if existing != &prompt_id {
                    return Err(
                        "Runtime Tool Batch waiting-user prompt conflicts with persisted prompt"
                            .to_string(),
                    );
                }
                return Ok(false);
            }
            record.waiting_user_prompt_ids[call_index] = Some(prompt_id.clone());
            Ok(true)
        })
        .await
    }

    pub async fn find_by_waiting_user_prompt(
        &self,
        prompt_id: &str,
    ) -> Result<Option<RuntimeToolBatchRecord>, String> {
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => Ok(records
                .read()
                .await
                .values()
                .find(|record| {
                    record
                        .waiting_user_prompt_ids
                        .iter()
                        .flatten()
                        .any(|id| id == prompt_id)
                })
                .cloned()),
            RuntimeToolBatchStoreBackend::Mongo(collection) => collection
                .find_one(doc! { "waiting_user_prompt_ids": prompt_id }, None)
                .await
                .map_err(|error| {
                    format!("load Runtime Tool Batch by waiting-user prompt failed: {error}")
                }),
        }
    }

    pub async fn ensure_result_pending(
        &self,
        batch_id: &str,
    ) -> Result<RuntimeToolBatchRecord, String> {
        self.mutate(batch_id, |record| {
            if record.status != RuntimeToolBatchStatus::Completed {
                return Ok(false);
            }
            if record.pending_event == Some(RuntimeToolBatchPendingEvent::AggregateResult) {
                return Ok(false);
            }
            record.pending_event = Some(RuntimeToolBatchPendingEvent::AggregateResult);
            Ok(true)
        })
        .await
    }

    pub async fn ensure_invocation_ready_for(
        &self,
        invocation_id: &str,
    ) -> Result<RuntimeToolBatchRecord, String> {
        let batch = self
            .find_by_invocation(invocation_id)
            .await?
            .ok_or_else(|| {
                "Runtime Tool Batch for next FIFO invocation was not found".to_string()
            })?;
        let call_index = batch
            .command
            .calls
            .iter()
            .position(|call| call.invocation_id == invocation_id)
            .ok_or_else(|| {
                "next FIFO invocation is missing from its Runtime Tool Batch".to_string()
            })?;
        self.mutate(batch.batch_id.as_str(), move |record| {
            if record.status == RuntimeToolBatchStatus::Completed {
                return Ok(false);
            }
            let expected = RuntimeToolBatchPendingEvent::InvocationReady { call_index };
            if record.pending_event.as_ref() == Some(&expected) {
                return Ok(false);
            }
            record.pending_event = Some(expected);
            Ok(true)
        })
        .await
    }

    pub async fn ensure_invocation_ready_for_event(
        &self,
        batch_id: &str,
        call_index: usize,
    ) -> Result<RuntimeToolBatchRecord, String> {
        self.mutate(batch_id, move |record| {
            if record.status == RuntimeToolBatchStatus::Completed
                || record.next_call_index != call_index
            {
                return Ok(false);
            }

            let expected = RuntimeToolBatchPendingEvent::InvocationReady { call_index };
            if record.pending_event.as_ref() == Some(&expected) {
                return Ok(false);
            }
            record.pending_event = Some(expected);
            Ok(true)
        })
        .await
    }

    pub async fn acknowledge_pending_event(
        &self,
        batch_id: &str,
        expected: &RuntimeToolBatchPendingEvent,
    ) -> Result<(), String> {
        let expected = expected.clone();
        self.mutate(batch_id, move |record| {
            if record.pending_event.as_ref() != Some(&expected) {
                return Ok(false);
            }
            record.pending_event = None;
            Ok(true)
        })
        .await
        .map(|_| ())
    }

    pub async fn list_pending(&self, limit: usize) -> Result<Vec<RuntimeToolBatchRecord>, String> {
        let limit = limit.clamp(1, 1_000);
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => Ok(records
                .read()
                .await
                .values()
                .filter(|record| record.pending_event.is_some())
                .take(limit)
                .cloned()
                .collect()),
            RuntimeToolBatchStoreBackend::Mongo(collection) => collection
                .find(
                    doc! { "pending_event": { "$ne": mongodb::bson::Bson::Null } },
                    None,
                )
                .await
                .map_err(|error| format!("list pending Runtime Tool Batches failed: {error}"))?
                .take(limit)
                .try_collect()
                .await
                .map_err(|error| format!("read pending Runtime Tool Batches failed: {error}")),
        }
    }

    pub async fn list_active(&self, limit: usize) -> Result<Vec<RuntimeToolBatchRecord>, String> {
        let limit = limit.clamp(1, 1_000);
        let mut records = match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => records
                .read()
                .await
                .values()
                .filter(|record| record.status == RuntimeToolBatchStatus::Active)
                .cloned()
                .collect::<Vec<_>>(),
            RuntimeToolBatchStoreBackend::Mongo(collection) => collection
                .find(doc! { "status": "active" }, None)
                .await
                .map_err(|error| format!("list active Runtime Tool Batches failed: {error}"))?
                .take(limit)
                .try_collect()
                .await
                .map_err(|error| format!("read active Runtime Tool Batches failed: {error}"))?,
        };
        records.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.batch_id.cmp(&right.batch_id))
        });
        records.truncate(limit);
        Ok(records)
    }

    async fn mutate<F>(
        &self,
        batch_id: &str,
        mut mutation: F,
    ) -> Result<RuntimeToolBatchRecord, String>
    where
        F: FnMut(&mut RuntimeToolBatchRecord) -> Result<bool, String>,
    {
        match self.backend.as_ref() {
            RuntimeToolBatchStoreBackend::Memory(records) => {
                let mut records = records.write().await;
                let record = records
                    .get_mut(batch_id)
                    .ok_or_else(|| "Runtime Tool Batch was not found".to_string())?;
                if mutation(record)? {
                    record.revision = record.revision.saturating_add(1);
                    record.updated_at_unix_ms = chrono::Utc::now().timestamp_millis();
                }
                Ok(record.clone())
            }
            RuntimeToolBatchStoreBackend::Mongo(collection) => {
                for _ in 0..MAX_CAS_ATTEMPTS {
                    let mut record = collection
                        .find_one(doc! { "_id": batch_id }, None)
                        .await
                        .map_err(|error| {
                            format!("load Runtime Tool Batch for CAS failed: {error}")
                        })?
                        .ok_or_else(|| "Runtime Tool Batch was not found".to_string())?;
                    let previous_revision = record.revision;
                    if !mutation(&mut record)? {
                        return Ok(record);
                    }
                    record.revision = record.revision.saturating_add(1);
                    record.updated_at_unix_ms = chrono::Utc::now().timestamp_millis();
                    let result = collection
                        .replace_one(
                            doc! { "_id": batch_id, "revision": previous_revision },
                            record.clone(),
                            None,
                        )
                        .await
                        .map_err(|error| format!("CAS Runtime Tool Batch failed: {error}"))?;
                    if result.modified_count == 1 {
                        return Ok(record);
                    }
                }
                Err("Runtime Tool Batch CAS conflict limit was exceeded".to_string())
            }
        }
    }
}

fn ensure_same_command(
    existing: &RuntimeToolBatchRecord,
    incoming: &RuntimeToolBatchRecord,
) -> Result<(), String> {
    let existing = serde_json::to_value(&existing.command).map_err(|error| error.to_string())?;
    let incoming = serde_json::to_value(&incoming.command).map_err(|error| error.to_string())?;
    if existing == incoming {
        Ok(())
    } else {
        Err("Runtime Tool Batch id conflicts with a different command".to_string())
    }
}

fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    match error.kind.as_ref() {
        MongoErrorKind::Write(WriteFailure::WriteError(error)) => error.code == 11_000,
        MongoErrorKind::BulkWrite(failure) => failure
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|error| error.code == 11_000)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_service::{
        McpToolCallCommandItem, McpToolCallResultStatus, MCP_ERROR_INVALID_PARAMS,
    };
    use serde_json::json;

    fn command(call_count: usize) -> McpToolCallCommand {
        McpToolCallCommand {
            owner_service: "task_runner_service".to_string(),
            agent_run_id: "agent-run-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            ordering_lane_key: "task-run-1".to_string(),
            lane_seq: 1,
            generation: 1,
            source_step_seq: 1,
            batch_id: "batch-1".to_string(),
            mcp_runtime_session_ref: "session-1".to_string(),
            result_routing_key: "task_runner.cloud_agent".to_string(),
            calls: (0..call_count)
                .map(|call_index| McpToolCallCommandItem {
                    invocation_id: format!("batch-1:{call_index}"),
                    tool_call_id: format!("tool-call-{call_index}"),
                    call_index,
                    name: format!("tool-{call_index}"),
                    arguments: json!({}),
                    preflight_error: None,
                })
                .collect(),
            delivery_attempt: 1,
        }
    }

    fn record(
        call_count: usize,
        items: Vec<Option<McpToolCallResultItem>>,
    ) -> RuntimeToolBatchRecord {
        let command = command(call_count);
        RuntimeToolBatchRecord {
            batch_id: command.batch_id.clone(),
            session_id: command.mcp_runtime_session_ref.clone(),
            invocation_ids: command
                .calls
                .iter()
                .map(|call| call.invocation_id.clone())
                .collect(),
            waiting_user_prompt_ids: vec![None; call_count],
            command,
            status: RuntimeToolBatchStatus::Active,
            next_call_index: 0,
            items,
            pending_event: None,
            revision: 0,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            expires_at: DateTime::from_millis(10_000),
            expires_at_unix: 10,
        }
    }

    fn result_item(call_index: usize) -> McpToolCallResultItem {
        McpToolCallResultItem {
            invocation_id: format!("batch-1:{call_index}"),
            tool_call_id: format!("tool-call-{call_index}"),
            call_index,
            name: format!("tool-{call_index}"),
            status: McpToolCallResultStatus::Completed,
            result: Some(json!({"call_index": call_index})),
            error_code: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn single_call_uses_the_same_batch_state_machine() {
        let store = RuntimeToolBatchStore::memory();
        let batch = store
            .insert_or_get(record(1, vec![None]))
            .await
            .expect("insert batch");
        assert_eq!(
            batch.pending_event,
            Some(RuntimeToolBatchPendingEvent::InvocationReady { call_index: 0 })
        );
        let batch = store
            .record_terminal_item("batch-1", 0, result_item(0))
            .await
            .expect("complete call");
        assert_eq!(batch.status, RuntimeToolBatchStatus::Completed);
        assert_eq!(
            batch.pending_event,
            Some(RuntimeToolBatchPendingEvent::AggregateResult)
        );
        assert_eq!(batch.aggregate_result().unwrap().items.len(), 1);
    }

    #[tokio::test]
    async fn multi_call_batch_advances_strictly_by_call_index() {
        let store = RuntimeToolBatchStore::memory();
        store
            .insert_or_get(record(3, vec![None, None, None]))
            .await
            .expect("insert batch");
        let batch = store
            .record_terminal_item("batch-1", 0, result_item(0))
            .await
            .expect("complete first");
        assert_eq!(
            batch.pending_event,
            Some(RuntimeToolBatchPendingEvent::InvocationReady { call_index: 1 })
        );
        assert!(store
            .record_terminal_item("batch-1", 2, result_item(2))
            .await
            .is_err());
        let batch = store
            .record_terminal_item("batch-1", 1, result_item(1))
            .await
            .expect("complete second");
        assert_eq!(batch.next_call_index, 2);
    }

    #[tokio::test]
    async fn redelivery_is_idempotent_and_conflicting_result_is_rejected() {
        let store = RuntimeToolBatchStore::memory();
        let batch = record(1, vec![None]);
        store
            .insert_or_get(batch.clone())
            .await
            .expect("insert batch");
        store
            .insert_or_get(batch)
            .await
            .expect("redelivery should reuse batch");
        let item = result_item(0);
        store
            .record_terminal_item("batch-1", 0, item.clone())
            .await
            .expect("complete call");
        store
            .record_terminal_item("batch-1", 0, item)
            .await
            .expect("duplicate terminal result is idempotent");
        let mut conflict = result_item(0);
        conflict.status = McpToolCallResultStatus::Failed;
        conflict.error_code = Some(MCP_ERROR_INVALID_PARAMS);
        assert!(store
            .record_terminal_item("batch-1", 0, conflict)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn waiting_user_prompt_can_resume_the_owning_batch() {
        let store = RuntimeToolBatchStore::memory();
        store
            .insert_or_get(record(2, vec![None, None]))
            .await
            .expect("insert batch");
        store
            .mark_waiting_for_user("batch-1", 0, "prompt-1".to_string())
            .await
            .expect("mark waiting user");
        let batch = store
            .find_by_waiting_user_prompt("prompt-1")
            .await
            .expect("lookup batch")
            .expect("batch exists");
        assert_eq!(batch.next_call_index, 0);
        let batch = store
            .record_terminal_item("batch-1", 0, result_item(0))
            .await
            .expect("resume resolved prompt");
        assert_eq!(
            batch.pending_event,
            Some(RuntimeToolBatchPendingEvent::InvocationReady { call_index: 1 })
        );
    }

    #[tokio::test]
    async fn active_batch_scan_includes_acked_running_batch_without_pending_event() {
        let store = RuntimeToolBatchStore::memory();
        store
            .insert_or_get(record(1, vec![None]))
            .await
            .expect("insert batch");
        store
            .acknowledge_pending_event(
                "batch-1",
                &RuntimeToolBatchPendingEvent::InvocationReady { call_index: 0 },
            )
            .await
            .expect("ack ready event");

        assert!(store.list_pending(10).await.unwrap().is_empty());
        let active = store.list_active(10).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].next_call_index, 0);
        assert_eq!(active[0].pending_event, None);
    }

    #[tokio::test]
    async fn failed_execution_can_restore_the_exact_ready_event_after_early_broker_ack() {
        let store = RuntimeToolBatchStore::memory();
        store
            .insert_or_get(record(2, vec![None, None]))
            .await
            .expect("insert batch");
        store
            .acknowledge_pending_event(
                "batch-1",
                &RuntimeToolBatchPendingEvent::InvocationReady { call_index: 0 },
            )
            .await
            .expect("ack ready event");

        let restored = store
            .ensure_invocation_ready_for_event("batch-1", 0)
            .await
            .expect("restore current ready event");
        assert_eq!(
            restored.pending_event,
            Some(RuntimeToolBatchPendingEvent::InvocationReady { call_index: 0 })
        );

        let unchanged = store
            .ensure_invocation_ready_for_event("batch-1", 1)
            .await
            .expect("ignore non-current ready event");
        assert_eq!(
            unchanged.pending_event,
            Some(RuntimeToolBatchPendingEvent::InvocationReady { call_index: 0 })
        );
    }
}
