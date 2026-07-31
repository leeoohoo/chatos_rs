// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use mongodb::bson::{doc, DateTime};
use mongodb::options::{FindOneAndUpdateOptions, IndexOptions, ReturnDocument};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const MAX_MEMORY_INVOCATIONS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInvocationStatus {
    Running,
    CancelRequested,
    Completed,
    Cancelled,
    UnknownExecutionState,
}

impl RuntimeInvocationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::CancelRequested => "cancel_requested",
            Self::Completed => "completed",
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
    pub created_at_unix_ms: i64,
    pub expires_at: DateTime,
    pub expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeInvocationStore {
    backend: Arc<RuntimeInvocationStoreBackend>,
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
        if record.status != RuntimeInvocationStatus::Running {
            return Err("new Runtime Invocation must start in running state".to_string());
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
                            RuntimeInvocationStatus::Running
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
                            RuntimeInvocationStatus::Running
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
                    if record.status == RuntimeInvocationStatus::Running {
                        record.status = RuntimeInvocationStatus::CancelRequested;
                    }
                    return Ok(Some(record.clone()));
                }
                Ok(None)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut running_filter = identity_filter.clone();
                running_filter.insert("status", RuntimeInvocationStatus::Running.as_str());
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

    pub async fn finish_if_running(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition(
            invocation_id,
            RuntimeInvocationStatus::Running,
            RuntimeInvocationStatus::Completed,
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
        self.transition(
            invocation_id,
            RuntimeInvocationStatus::CancelRequested,
            status,
        )
        .await
    }

    async fn transition(
        &self,
        invocation_id: &str,
        from: RuntimeInvocationStatus,
        to: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if record.status != from {
                    return Ok(false);
                }
                record.status = to;
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .update_one(
                    doc! { "_id": invocation_id, "status": from.as_str() },
                    doc! { "$set": { "status": to.as_str() } },
                    None,
                )
                .await
                .map(|result| result.modified_count == 1)
                .map_err(|error| format!("finish Runtime Invocation failed: {error}")),
        }
    }
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
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
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
        assert!(store.finish_if_running("invocation-1").await.unwrap());
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
        assert!(store.finish_if_running("invocation-1").await.unwrap());
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
}
