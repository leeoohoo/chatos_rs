// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use mongodb::bson::{self, doc, DateTime};
use mongodb::options::{FindOneAndUpdateOptions, IndexOptions, ReturnDocument};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

const MAX_MEMORY_INVOCATIONS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInvocationStatus {
    Queued,
    Running,
    CancelRequested,
    Completed,
    Failed,
    Cancelled,
    UnknownExecutionState,
}

impl RuntimeInvocationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::CancelRequested => "cancel_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::UnknownExecutionState => "unknown_execution_state",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInvocationRecord {
    #[serde(rename = "_id")]
    pub invocation_id: String,
    pub session_id: String,
    pub request_id_key: String,
    pub caller_service: String,
    pub resource_id: String,
    pub exposed_tool_name: String,
    pub mutation_may_have_started: bool,
    pub cancel_supported: bool,
    pub status: RuntimeInvocationStatus,
    #[serde(default)]
    pub async_execution: bool,
    pub created_at_unix_ms: i64,
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub completed_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub terminal_result: Option<Value>,
    #[serde(default)]
    pub terminal_error_code: Option<i32>,
    #[serde(default)]
    pub terminal_error_message: Option<String>,
    pub expires_at: DateTime,
    pub expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeInvocationStore {
    backend: Arc<RuntimeInvocationStoreBackend>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeInvocationStoreStats {
    pub backend: &'static str,
    pub total_active: usize,
    pub queued: usize,
    pub running: usize,
    pub cancel_requested: usize,
    pub terminal: usize,
}

enum RuntimeInvocationStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeInvocationRecord>>),
    Mongo(Collection<RuntimeInvocationRecord>),
}

impl RuntimeInvocationStore {
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(RuntimeInvocationStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
        }
    }

    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect MCP invocation MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection =
            database.collection::<RuntimeInvocationRecord>("mcp_management_runtime_invocations");
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_invocation_expiry_ttl".to_string())
                            .expire_after(Some(std::time::Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("initialize Runtime Invocation TTL index failed: {error}"))?;
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "session_id": 1, "request_id_key": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_invocation_session_request".to_string())
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| {
                format!("initialize Runtime Invocation identity index failed: {error}")
            })?;
        Ok(Self {
            backend: Arc::new(RuntimeInvocationStoreBackend::Mongo(collection)),
        })
    }

    pub async fn register(&self, record: RuntimeInvocationRecord) -> Result<(), String> {
        if !matches!(
            record.status,
            RuntimeInvocationStatus::Queued | RuntimeInvocationStatus::Running
        ) {
            return Err("new Runtime Invocation must start in queued or running state".to_string());
        }
        if record.expires_at_unix <= chrono::Utc::now().timestamp() {
            return Err("cannot register an expired Runtime Invocation".to_string());
        }
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let now = chrono::Utc::now().timestamp();
                let mut invocations = invocations.write().await;
                invocations.retain(|_, value| value.expires_at_unix > now);
                invocations.retain(|_, value| {
                    value.session_id != record.session_id
                        || value.request_id_key != record.request_id_key
                        || matches!(
                            value.status,
                            RuntimeInvocationStatus::Queued
                                | RuntimeInvocationStatus::Running
                                | RuntimeInvocationStatus::CancelRequested
                        )
                });
                if invocations.len() >= MAX_MEMORY_INVOCATIONS {
                    return Err("Runtime Invocation store capacity was reached".to_string());
                }
                if invocations.values().any(|value| {
                    value.session_id == record.session_id
                        && value.request_id_key == record.request_id_key
                        && matches!(
                            value.status,
                            RuntimeInvocationStatus::Queued
                                | RuntimeInvocationStatus::Running
                                | RuntimeInvocationStatus::CancelRequested
                        )
                }) {
                    return Err(
                        "JSON-RPC request id is already active in this Runtime Session".to_string(),
                    );
                }
                invocations.insert(record.invocation_id.clone(), record);
                Ok(())
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                collection
                    .delete_many(
                        doc! {
                            "session_id": record.session_id.as_str(),
                            "request_id_key": record.request_id_key.as_str(),
                            "status": { "$in": [
                                RuntimeInvocationStatus::Completed.as_str(),
                                RuntimeInvocationStatus::Failed.as_str(),
                                RuntimeInvocationStatus::Cancelled.as_str(),
                                RuntimeInvocationStatus::UnknownExecutionState.as_str(),
                            ] },
                        },
                        None,
                    )
                    .await
                    .map_err(|error| {
                        format!("remove prior terminal Runtime Invocation failed: {error}")
                    })?;
                collection
                    .insert_one(record, None)
                    .await
                    .map(|_| ())
                    .map_err(|error| format!("register Runtime Invocation failed: {error}"))
            }
        }
    }

    pub async fn request_cancel_by_request(
        &self,
        session_id: &str,
        request_id_key: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        self.request_cancel(
            doc! { "session_id": session_id, "request_id_key": request_id_key },
            |record| record.session_id == session_id && record.request_id_key == request_id_key,
        )
        .await
    }

    pub async fn request_cancel_by_invocation(
        &self,
        invocation_id: &str,
        caller_service: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        self.request_cancel(
            doc! { "_id": invocation_id, "caller_service": caller_service },
            |record| {
                record.invocation_id == invocation_id && record.caller_service == caller_service
            },
        )
        .await
    }

    pub async fn get_for_caller(
        &self,
        invocation_id: &str,
        caller_service: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(invocations
                    .get(invocation_id)
                    .filter(|record| record.caller_service == caller_service)
                    .cloned())
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! {
                        "_id": invocation_id,
                        "caller_service": caller_service,
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
                .await
                .map_err(|error| format!("load Runtime Invocation failed: {error}")),
        }
    }

    async fn request_cancel<F>(
        &self,
        mut identity_filter: mongodb::bson::Document,
        memory_matches: F,
    ) -> Result<Option<RuntimeInvocationRecord>, String>
    where
        F: Fn(&RuntimeInvocationRecord) -> bool,
    {
        let now = chrono::Utc::now().timestamp();
        identity_filter.insert("expires_at_unix", doc! { "$gt": now });
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                let record = invocations
                    .values_mut()
                    .find(|record| memory_matches(record));
                if let Some(record) = record {
                    if matches!(
                        record.status,
                        RuntimeInvocationStatus::Queued | RuntimeInvocationStatus::Running
                    ) {
                        record.status = RuntimeInvocationStatus::CancelRequested;
                    }
                    return Ok(Some(record.clone()));
                }
                Ok(None)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut running_filter = identity_filter.clone();
                running_filter.insert(
                    "status",
                    doc! {
                        "$in": [
                            RuntimeInvocationStatus::Queued.as_str(),
                            RuntimeInvocationStatus::Running.as_str(),
                        ]
                    },
                );
                let updated = collection
                    .find_one_and_update(
                        running_filter,
                        doc! { "$set": { "status": RuntimeInvocationStatus::CancelRequested.as_str() } },
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(|error| format!("request Runtime Invocation cancellation failed: {error}"))?;
                if updated.is_some() {
                    return Ok(updated);
                }
                collection
                    .find_one(identity_filter, None)
                    .await
                    .map_err(|error| {
                        format!("load Runtime Invocation cancellation state failed: {error}")
                    })
            }
        }
    }

    pub async fn cancellation_requested(&self, invocation_id: &str) -> Result<bool, String> {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(invocations.get(invocation_id).is_some_and(|record| {
                    record.status == RuntimeInvocationStatus::CancelRequested
                }))
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! {
                        "_id": invocation_id,
                        "status": RuntimeInvocationStatus::CancelRequested.as_str(),
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
                .await
                .map(|record| record.is_some())
                .map_err(|error| format!("poll Runtime Invocation cancellation failed: {error}")),
        }
    }

    pub async fn mark_running(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Queued],
            RuntimeInvocationStatus::Running,
        )
        .await
    }

    pub async fn complete(&self, invocation_id: &str, result: Value) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[RuntimeInvocationStatus::Running],
            RuntimeInvocationStatus::Completed,
            Some(result),
            None,
            None,
        )
        .await
    }

    pub async fn fail(
        &self,
        invocation_id: &str,
        error_code: i32,
        error_message: impl Into<String>,
    ) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[RuntimeInvocationStatus::Running],
            RuntimeInvocationStatus::Failed,
            None,
            Some(error_code),
            Some(error_message.into()),
        )
        .await
    }

    pub async fn finish_cancellation(
        &self,
        invocation_id: &str,
        status: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        if !matches!(
            status,
            RuntimeInvocationStatus::Cancelled | RuntimeInvocationStatus::UnknownExecutionState
        ) {
            return Err("invalid terminal Runtime Invocation cancellation state".to_string());
        }
        self.transition_terminal(
            invocation_id,
            &[RuntimeInvocationStatus::CancelRequested],
            status,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn cancel_without_start(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::CancelRequested,
            ],
            RuntimeInvocationStatus::Cancelled,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn stats(&self) -> Result<RuntimeInvocationStoreStats, String> {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(summarize_runtime_invocations(
                    "memory",
                    invocations.values().map(|record| record.status),
                ))
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let total_active = count_runtime_invocations(
                    collection,
                    doc! { "expires_at_unix": { "$gt": now } },
                )
                .await?;
                let queued = count_runtime_invocations(
                    collection,
                    doc! {
                        "expires_at_unix": { "$gt": now },
                        "status": RuntimeInvocationStatus::Queued.as_str(),
                    },
                )
                .await?;
                let running = count_runtime_invocations(
                    collection,
                    doc! {
                        "expires_at_unix": { "$gt": now },
                        "status": RuntimeInvocationStatus::Running.as_str(),
                    },
                )
                .await?;
                let cancel_requested = count_runtime_invocations(
                    collection,
                    doc! {
                        "expires_at_unix": { "$gt": now },
                        "status": RuntimeInvocationStatus::CancelRequested.as_str(),
                    },
                )
                .await?;
                let terminal = count_runtime_invocations(
                    collection,
                    doc! {
                        "expires_at_unix": { "$gt": now },
                        "status": {
                            "$in": [
                                RuntimeInvocationStatus::Completed.as_str(),
                                RuntimeInvocationStatus::Failed.as_str(),
                                RuntimeInvocationStatus::Cancelled.as_str(),
                                RuntimeInvocationStatus::UnknownExecutionState.as_str(),
                            ]
                        },
                    },
                )
                .await?;
                Ok(RuntimeInvocationStoreStats {
                    backend: "mongo",
                    total_active,
                    queued,
                    running,
                    cancel_requested,
                    terminal,
                })
            }
        }
    }

    async fn transition_status(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                if to == RuntimeInvocationStatus::Running && record.started_at_unix_ms.is_none() {
                    record.started_at_unix_ms = Some(chrono::Utc::now().timestamp_millis());
                }
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut set_doc = doc! { "status": to.as_str() };
                if to == RuntimeInvocationStatus::Running {
                    set_doc.insert(
                        "started_at_unix_ms",
                        bson::to_bson(&chrono::Utc::now().timestamp_millis())
                            .map_err(|error| error.to_string())?,
                    );
                }
                collection
                    .update_one(
                        doc! {
                            "_id": invocation_id,
                            "status": { "$in": from.iter().map(|status| status.as_str()).collect::<Vec<_>>() }
                        },
                        doc! { "$set": set_doc },
                        None,
                    )
                    .await
                    .map(|result| result.modified_count == 1)
                    .map_err(|error| format!("finish Runtime Invocation failed: {error}"))
            }
        }
    }

    async fn transition_terminal(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
        result: Option<Value>,
        error_code: Option<i32>,
        error_message: Option<String>,
    ) -> Result<bool, String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let result = sanitize_terminal_result(result)?;
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = result;
                record.terminal_error_code = error_code;
                record.terminal_error_message = error_message;
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut set_doc = doc! {
                    "status": to.as_str(),
                    "completed_at_unix_ms": now_ms,
                };
                match result {
                    Some(value) => {
                        set_doc.insert(
                            "terminal_result",
                            bson::to_bson(&value).map_err(|error| error.to_string())?,
                        );
                    }
                    None => {
                        set_doc.insert("terminal_result", bson::Bson::Null);
                    }
                }
                match error_code {
                    Some(value) => {
                        set_doc.insert("terminal_error_code", value);
                    }
                    None => {
                        set_doc.insert("terminal_error_code", bson::Bson::Null);
                    }
                }
                match error_message {
                    Some(value) => {
                        set_doc.insert("terminal_error_message", value);
                    }
                    None => {
                        set_doc.insert("terminal_error_message", bson::Bson::Null);
                    }
                }
                collection
                    .update_one(
                        doc! {
                            "_id": invocation_id,
                            "status": { "$in": from.iter().map(|status| status.as_str()).collect::<Vec<_>>() }
                        },
                        doc! { "$set": set_doc },
                        None,
                    )
                    .await
                    .map(|result| result.modified_count == 1)
                    .map_err(|error| format!("finish Runtime Invocation failed: {error}"))
            }
        }
    }
}

fn sanitize_terminal_result(result: Option<Value>) -> Result<Option<Value>, String> {
    const MAX_INLINE_RESULT_BYTES: usize = 256 * 1024;

    let Some(result) = result else {
        return Ok(None);
    };
    let encoded = serde_json::to_vec(&result).map_err(|error| error.to_string())?;
    if encoded.len() <= MAX_INLINE_RESULT_BYTES {
        return Ok(Some(result));
    }
    Ok(Some(serde_json::json!({
        "status": "result_truncated",
        "result_bytes": encoded.len(),
    })))
}

fn summarize_runtime_invocations(
    backend: &'static str,
    statuses: impl IntoIterator<Item = RuntimeInvocationStatus>,
) -> RuntimeInvocationStoreStats {
    let mut stats = RuntimeInvocationStoreStats {
        backend,
        total_active: 0,
        queued: 0,
        running: 0,
        cancel_requested: 0,
        terminal: 0,
    };
    for status in statuses {
        stats.total_active = stats.total_active.saturating_add(1);
        match status {
            RuntimeInvocationStatus::Queued => stats.queued = stats.queued.saturating_add(1),
            RuntimeInvocationStatus::Running => stats.running = stats.running.saturating_add(1),
            RuntimeInvocationStatus::CancelRequested => {
                stats.cancel_requested = stats.cancel_requested.saturating_add(1)
            }
            RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState => {
                stats.terminal = stats.terminal.saturating_add(1)
            }
        }
    }
    stats
}

async fn count_runtime_invocations(
    collection: &Collection<RuntimeInvocationRecord>,
    filter: mongodb::bson::Document,
) -> Result<usize, String> {
    collection
        .count_documents(filter, None)
        .await
        .map_err(|error| format!("count Runtime Invocations failed: {error}"))
        .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> RuntimeInvocationRecord {
        RuntimeInvocationRecord {
            invocation_id: "invocation-1".to_string(),
            session_id: "session-1".to_string(),
            request_id_key: "\"request-1\"".to_string(),
            caller_service: "task-runner".to_string(),
            resource_id: "mcp-1".to_string(),
            exposed_tool_name: "demo_read".to_string(),
            mutation_may_have_started: false,
            cancel_supported: true,
            status: RuntimeInvocationStatus::Running,
            async_execution: false,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            expires_at: DateTime::from_millis((chrono::Utc::now().timestamp() + 60) * 1_000),
            expires_at_unix: chrono::Utc::now().timestamp() + 60,
        }
    }

    #[tokio::test]
    async fn cloned_store_coordinates_cancel_and_terminal_transition() {
        let writer = RuntimeInvocationStore::memory();
        let reader = writer.clone();
        writer.register(record()).await.unwrap();
        let cancelled = reader
            .request_cancel_by_request("session-1", "\"request-1\"")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.status, RuntimeInvocationStatus::CancelRequested);
        assert!(writer.cancellation_requested("invocation-1").await.unwrap());
        assert!(reader
            .finish_cancellation("invocation-1", RuntimeInvocationStatus::Cancelled)
            .await
            .unwrap());
        assert!(!writer.cancellation_requested("invocation-1").await.unwrap());
    }

    #[tokio::test]
    async fn completed_invocation_cannot_be_changed_to_cancel_requested() {
        let store = RuntimeInvocationStore::memory();
        store.register(record()).await.unwrap();
        assert!(store
            .complete("invocation-1", serde_json::json!({"ok": true}))
            .await
            .unwrap());
        let completed = store
            .request_cancel_by_invocation("invocation-1", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completed.status, RuntimeInvocationStatus::Completed);
    }

    #[tokio::test]
    async fn completed_request_id_can_be_reused_after_the_prior_call_is_terminal() {
        let store = RuntimeInvocationStore::memory();
        store.register(record()).await.unwrap();
        assert!(store
            .complete("invocation-1", serde_json::json!({"ok": true}))
            .await
            .unwrap());
        let mut reused = record();
        reused.invocation_id = "invocation-2".to_string();
        store.register(reused).await.unwrap();
        assert!(store
            .request_cancel_by_invocation("invocation-2", "task-runner")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL"]
    async fn mongodb_store_coordinates_cancellation_across_service_instances() {
        let database_url = std::env::var("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL")
            .expect("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL");
        let invocation_id = format!("shared-invocation-test-{}", uuid::Uuid::new_v4());
        let mut invocation = record();
        invocation.invocation_id = invocation_id.clone();
        invocation.session_id = format!("shared-session-test-{}", uuid::Uuid::new_v4());
        let first = RuntimeInvocationStore::connect(database_url.as_str())
            .await
            .unwrap();
        let second = RuntimeInvocationStore::connect(database_url.as_str())
            .await
            .unwrap();
        first.register(invocation.clone()).await.unwrap();
        let cancelled = second
            .request_cancel_by_request(
                invocation.session_id.as_str(),
                invocation.request_id_key.as_str(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.invocation_id, invocation_id);
        assert!(first
            .cancellation_requested(cancelled.invocation_id.as_str())
            .await
            .unwrap());
        assert!(first
            .finish_cancellation(
                cancelled.invocation_id.as_str(),
                RuntimeInvocationStatus::Cancelled,
            )
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn queued_invocation_can_be_marked_running_and_failed_with_queryable_error() {
        let store = RuntimeInvocationStore::memory();
        let mut queued = record();
        queued.invocation_id = "invocation-queued".to_string();
        queued.status = RuntimeInvocationStatus::Queued;
        queued.async_execution = true;
        queued.started_at_unix_ms = None;
        store.register(queued).await.unwrap();
        assert!(store.mark_running("invocation-queued").await.unwrap());
        assert!(store
            .fail("invocation-queued", -32000, "provider timed out")
            .await
            .unwrap());
        let record = store
            .get_for_caller("invocation-queued", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, RuntimeInvocationStatus::Failed);
        assert_eq!(record.terminal_error_code, Some(-32000));
        assert_eq!(
            record.terminal_error_message.as_deref(),
            Some("provider timed out")
        );
    }

    #[tokio::test]
    async fn stats_summarize_memory_store_by_status() {
        let store = RuntimeInvocationStore::memory();

        let queued = RuntimeInvocationRecord {
            invocation_id: "invocation-stats-queued".to_string(),
            session_id: "session-stats-queued".to_string(),
            request_id_key: "\"request-stats-queued\"".to_string(),
            status: RuntimeInvocationStatus::Queued,
            async_execution: true,
            started_at_unix_ms: None,
            ..record()
        };
        let running = RuntimeInvocationRecord {
            invocation_id: "invocation-stats-running".to_string(),
            session_id: "session-stats-running".to_string(),
            request_id_key: "\"request-stats-running\"".to_string(),
            status: RuntimeInvocationStatus::Running,
            ..record()
        };
        let terminal = RuntimeInvocationRecord {
            invocation_id: "invocation-stats-terminal".to_string(),
            session_id: "session-stats-terminal".to_string(),
            request_id_key: "\"request-stats-terminal\"".to_string(),
            status: RuntimeInvocationStatus::Completed,
            completed_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
            ..record()
        };

        store.register(queued).await.unwrap();
        store.register(running).await.unwrap();
        let mut terminal_ready = terminal.clone();
        terminal_ready.status = RuntimeInvocationStatus::Running;
        terminal_ready.completed_at_unix_ms = None;
        store.register(terminal_ready).await.unwrap();
        store
            .complete("invocation-stats-terminal", serde_json::json!({"ok": true}))
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.total_active, 3);
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.running, 1);
        assert_eq!(stats.cancel_requested, 0);
        assert_eq!(stats.terminal, 1);
    }
}
