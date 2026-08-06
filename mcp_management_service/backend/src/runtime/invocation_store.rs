// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, Weak};

use chatos_mcp::code_maintainer::{classify_file_modification_error, FileModificationOutcome};
use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, DateTime};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, IndexOptions, ReturnDocument};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

use super::{
    RuntimeInvocationQuota, RuntimeInvocationQuotaLimits, RuntimeInvocationQuotaReserveError,
};

const MAX_MEMORY_INVOCATIONS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInvocationStatus {
    Queued,
    Running,
    WaitingForUser,
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
            Self::WaitingForUser => "waiting_for_user",
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
    pub tenant_id: String,
    pub owner_user_id: String,
    pub project_id: String,
    pub device_id: Option<String>,
    pub resource_id: String,
    pub exposed_tool_name: String,
    #[serde(default)]
    pub original_tool_name: String,
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
    #[serde(default)]
    pub file_modification_outcome: Option<FileModificationOutcome>,
    #[serde(default)]
    pub result_reply_to: Option<String>,
    #[serde(default)]
    pub result_event_id: Option<String>,
    #[serde(default)]
    pub result_event_pending: bool,
    pub expires_at: DateTime,
    pub expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeInvocationStore {
    backend: Arc<RuntimeInvocationStoreBackend>,
    quota: RuntimeInvocationQuota,
    result_event_notify: Arc<Notify>,
    cancellation_waiters: Arc<StdMutex<HashMap<String, Weak<Notify>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeInvocationRegisterError {
    CapacityExhausted { dimension: &'static str, limit: u32 },
    Store(String),
}

impl std::fmt::Display for RuntimeInvocationRegisterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapacityExhausted { dimension, limit } => write!(
                formatter,
                "Runtime Invocation {dimension} quota was exhausted at {limit} active calls"
            ),
            Self::Store(error) => formatter.write_str(error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingRuntimeInvocationResultEvent {
    pub reply_to: String,
    pub event: chatos_mcp_management_sdk::RuntimeInvocationResultEvent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeInvocationStoreStats {
    pub backend: &'static str,
    pub quota_limits: RuntimeInvocationQuotaLimits,
    pub total_active: usize,
    pub queued: usize,
    pub running: usize,
    pub waiting_for_user: usize,
    pub cancel_requested: usize,
    pub terminal: usize,
    pub pending_result_events: usize,
    pub file_modifications: FileModificationOutcomeStats,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FileModificationOutcomeStats {
    pub total: usize,
    pub changed: usize,
    pub already_applied: usize,
    pub stale: usize,
    pub validation: usize,
    pub infrastructure: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
enum RuntimeInvocationStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeInvocationRecord>>),
    Mongo(Collection<RuntimeInvocationRecord>),
}

impl RuntimeInvocationStore {
    #[cfg(test)]
    pub fn memory() -> Self {
        let limits = RuntimeInvocationQuotaLimits::new(100_000, 100_000, 100_000, 100_000)
            .expect("test Runtime Invocation quota limits are valid");
        Self::memory_with_quota(limits)
    }

    #[cfg(test)]
    fn memory_with_quota(limits: RuntimeInvocationQuotaLimits) -> Self {
        Self {
            backend: Arc::new(RuntimeInvocationStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
            quota: RuntimeInvocationQuota::memory(limits),
            result_event_notify: Arc::new(Notify::new()),
            cancellation_waiters: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    pub async fn connect(
        database_url: &str,
        quota: RuntimeInvocationQuota,
    ) -> Result<Self, String> {
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
        for (name, keys) in [
            (
                "runtime_invocation_status_expiry",
                doc! { "expires_at": 1, "status": 1 },
            ),
            (
                "runtime_invocation_result_outbox",
                doc! {
                    "result_event_pending": 1,
                    "expires_at": 1,
                    "completed_at_unix_ms": 1,
                },
            ),
            (
                "runtime_invocation_tenant_status",
                doc! { "tenant_id": 1, "status": 1, "expires_at": 1 },
            ),
            (
                "runtime_invocation_owner_status",
                doc! { "owner_user_id": 1, "status": 1, "expires_at": 1 },
            ),
            (
                "runtime_invocation_project_status",
                doc! { "project_id": 1, "status": 1, "expires_at": 1 },
            ),
            (
                "runtime_invocation_device_status",
                doc! { "device_id": 1, "status": 1, "expires_at": 1 },
            ),
            (
                "runtime_invocation_file_modification_outcome",
                doc! { "expires_at": 1, "file_modification_outcome": 1 },
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
                    format!("initialize Runtime Invocation quota index {name} failed: {error}")
                })?;
        }
        Ok(Self {
            backend: Arc::new(RuntimeInvocationStoreBackend::Mongo(collection)),
            quota,
            result_event_notify: Arc::new(Notify::new()),
            cancellation_waiters: Arc::new(StdMutex::new(HashMap::new())),
        })
    }

    pub async fn register(
        &self,
        record: RuntimeInvocationRecord,
    ) -> Result<(), RuntimeInvocationRegisterError> {
        if !matches!(
            record.status,
            RuntimeInvocationStatus::Queued | RuntimeInvocationStatus::Running
        ) {
            return Err(RuntimeInvocationRegisterError::Store(
                "new Runtime Invocation must start in queued or running state".to_string(),
            ));
        }
        if record.expires_at_unix <= chrono::Utc::now().timestamp() {
            return Err(RuntimeInvocationRegisterError::Store(
                "cannot register an expired Runtime Invocation".to_string(),
            ));
        }
        self.quota
            .reserve(&record)
            .await
            .map_err(|error| match error {
                RuntimeInvocationQuotaReserveError::CapacityExhausted { dimension, limit } => {
                    RuntimeInvocationRegisterError::CapacityExhausted { dimension, limit }
                }
                RuntimeInvocationQuotaReserveError::Infrastructure(error) => {
                    RuntimeInvocationRegisterError::Store(error)
                }
            })?;
        let result = match self.backend.as_ref() {
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
                                | RuntimeInvocationStatus::WaitingForUser
                                | RuntimeInvocationStatus::CancelRequested
                        )
                });
                if invocations.len() >= MAX_MEMORY_INVOCATIONS {
                    Err(RuntimeInvocationRegisterError::Store(
                        "Runtime Invocation store capacity was reached".to_string(),
                    ))
                } else if invocations.values().any(|value| {
                    value.session_id == record.session_id
                        && value.request_id_key == record.request_id_key
                        && matches!(
                            value.status,
                            RuntimeInvocationStatus::Queued
                                | RuntimeInvocationStatus::Running
                                | RuntimeInvocationStatus::WaitingForUser
                                | RuntimeInvocationStatus::CancelRequested
                        )
                }) {
                    Err(RuntimeInvocationRegisterError::Store(
                        "JSON-RPC request id is already active in this Runtime Session".to_string(),
                    ))
                } else {
                    invocations.insert(record.invocation_id.clone(), record.clone());
                    Ok(())
                }
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
                        RuntimeInvocationRegisterError::Store(format!(
                            "remove prior terminal Runtime Invocation failed: {error}"
                        ))
                    })?;
                collection
                    .insert_one(record.clone(), None)
                    .await
                    .map(|_| ())
                    .map_err(|error| {
                        RuntimeInvocationRegisterError::Store(format!(
                            "register Runtime Invocation failed: {error}"
                        ))
                    })
            }
        };
        if result.is_err() {
            if let Err(error) = self.quota.release(&record).await {
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release rejected Runtime Invocation quota reservation failed"
                );
            }
        }
        result
    }

    pub async fn request_cancel_by_request(
        &self,
        session_id: &str,
        request_id_key: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let record = self
            .request_cancel(
                doc! { "session_id": session_id, "request_id_key": request_id_key },
                |record| record.session_id == session_id && record.request_id_key == request_id_key,
            )
            .await?;
        self.signal_cancelled_record(record.as_ref())?;
        Ok(record)
    }

    pub async fn request_cancel_by_invocation(
        &self,
        invocation_id: &str,
        caller_service: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let record = self
            .request_cancel(
                doc! { "_id": invocation_id, "caller_service": caller_service },
                |record| {
                    record.invocation_id == invocation_id && record.caller_service == caller_service
                },
            )
            .await?;
        self.signal_cancelled_record(record.as_ref())?;
        Ok(record)
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

    pub async fn dead_letter_archive_candidate(
        &self,
        invocation_id: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let now = chrono::Utc::now().timestamp();
        let eligible = |record: &RuntimeInvocationRecord| {
            record.invocation_id == invocation_id
                && record.expires_at_unix > now
                && record.async_execution
                && record.status == RuntimeInvocationStatus::Failed
                && !record.result_event_pending
                && record.completed_at_unix_ms.is_some()
                && record
                    .terminal_error_message
                    .as_deref()
                    .is_some_and(|message| message.starts_with("async tool dispatch failed after "))
        };
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(invocations
                    .get(invocation_id)
                    .filter(|record| eligible(record))
                    .cloned())
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! {
                        "_id": invocation_id,
                        "expires_at_unix": { "$gt": now },
                        "async_execution": true,
                        "status": RuntimeInvocationStatus::Failed.as_str(),
                        "result_event_pending": false,
                        "completed_at_unix_ms": { "$type": "number" },
                        "terminal_error_message": {
                            "$regex": "^async tool dispatch failed after "
                        },
                    },
                    None,
                )
                .await
                .map_err(|error| format!("load dead-lettered Runtime Invocation failed: {error}")),
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
                        RuntimeInvocationStatus::Queued
                            | RuntimeInvocationStatus::Running
                            | RuntimeInvocationStatus::WaitingForUser
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
                            RuntimeInvocationStatus::WaitingForUser.as_str(),
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
                .map_err(|error| {
                    format!("load Runtime Invocation cancellation state failed: {error}")
                }),
        }
    }

    pub async fn wait_for_cancellation(&self, invocation_id: &str) -> Result<(), String> {
        let notify = Arc::new(Notify::new());
        {
            let mut waiters = self
                .cancellation_waiters
                .lock()
                .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
            waiters.insert(invocation_id.to_string(), Arc::downgrade(&notify));
        }
        let result = match self.cancellation_requested(invocation_id).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                notify.notified().await;
                Ok(())
            }
            Err(error) => Err(error),
        };
        self.remove_cancellation_waiter(invocation_id, &notify)?;
        result
    }

    pub fn signal_cancellation(&self, invocation_id: &str) -> Result<(), String> {
        let mut waiters = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
        let Some(waiter) = waiters.get(invocation_id) else {
            return Ok(());
        };
        let Some(notify) = waiter.upgrade() else {
            waiters.remove(invocation_id);
            return Ok(());
        };
        notify.notify_one();
        Ok(())
    }

    pub async fn reconcile_cancellation_waiters(&self) -> Result<(), String> {
        let invocation_ids = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for invocation_id in invocation_ids {
            if self.cancellation_requested(invocation_id.as_str()).await? {
                self.signal_cancellation(invocation_id.as_str())?;
            }
        }
        Ok(())
    }

    fn signal_cancelled_record(
        &self,
        record: Option<&RuntimeInvocationRecord>,
    ) -> Result<(), String> {
        if let Some(record) =
            record.filter(|record| record.status == RuntimeInvocationStatus::CancelRequested)
        {
            self.signal_cancellation(record.invocation_id.as_str())?;
        }
        Ok(())
    }

    fn remove_cancellation_waiter(
        &self,
        invocation_id: &str,
        notify: &Arc<Notify>,
    ) -> Result<(), String> {
        let mut waiters = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
        if waiters
            .get(invocation_id)
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, notify))
        {
            waiters.remove(invocation_id);
        }
        Ok(())
    }

    pub async fn mark_running(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Queued],
            RuntimeInvocationStatus::Running,
        )
        .await
    }

    pub async fn mark_waiting_for_user(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Running],
            RuntimeInvocationStatus::WaitingForUser,
        )
        .await
    }

    pub async fn complete(&self, invocation_id: &str, result: Value) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
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
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
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
                    self.quota.limits(),
                    invocations.values().map(|record| {
                        (
                            record.status,
                            record.result_event_pending,
                            record.file_modification_outcome,
                        )
                    }),
                ))
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                aggregate_runtime_invocation_stats(collection, DateTime::now(), self.quota.limits())
                    .await
            }
        }
    }

    pub async fn pending_result_events(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingRuntimeInvocationResultEvent>, String> {
        let now = chrono::Utc::now().timestamp();
        let records = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                invocations
                    .values()
                    .filter(|record| record.result_event_pending)
                    .take(limit.max(1) as usize)
                    .cloned()
                    .collect::<Vec<_>>()
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find(
                    doc! {
                        "result_event_pending": true,
                        "expires_at": { "$gt": DateTime::now() },
                    },
                    FindOptions::builder()
                        .sort(doc! { "completed_at_unix_ms": 1 })
                        .limit(limit.max(1))
                        .build(),
                )
                .await
                .map_err(|error| {
                    format!("load pending Runtime Invocation result events failed: {error}")
                })?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| {
                    format!("read pending Runtime Invocation result events failed: {error}")
                })?,
        };
        records
            .into_iter()
            .map(pending_result_event_from_record)
            .collect()
    }

    pub async fn acknowledge_result_event(
        &self,
        invocation_id: &str,
        event_id: &str,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !record.result_event_pending
                    || record.result_event_id.as_deref() != Some(event_id)
                {
                    return Ok(false);
                }
                record.result_event_pending = false;
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .update_one(
                    doc! {
                        "_id": invocation_id,
                        "result_event_id": event_id,
                        "result_event_pending": true,
                    },
                    doc! { "$set": { "result_event_pending": false } },
                    None,
                )
                .await
                .map(|result| result.modified_count == 1)
                .map_err(|error| {
                    format!("acknowledge Runtime Invocation result event failed: {error}")
                }),
        }
    }

    pub async fn wait_for_result_event_signal(&self) {
        self.result_event_notify.notified().await;
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
        let result_event_id = format!("mcp_result_{}", Uuid::new_v4().simple());
        let file_modification_outcome =
            terminal_file_modification_outcome(to, result.as_ref(), error_message.as_deref());
        let result = sanitize_terminal_result(result)?;
        let transitioned_record = match self.backend.as_ref() {
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
                record.file_modification_outcome =
                    if is_file_modification_tool(record.original_tool_name.as_str()) {
                        file_modification_outcome
                    } else {
                        None
                    };
                if record.async_execution {
                    record.result_event_id = Some(result_event_id);
                    record.result_event_pending = true;
                }
                Ok(Some(record.clone()))
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
                let outcome_value = file_modification_outcome
                    .map(|outcome| bson::Bson::String(outcome.as_str().to_string()))
                    .unwrap_or(bson::Bson::Null);
                set_doc.insert(
                    "file_modification_outcome",
                    doc! {
                        "$cond": [
                            { "$in": ["$original_tool_name", ["edit_file", "apply_patch", "patch"]] },
                            outcome_value,
                            bson::Bson::Null,
                        ]
                    },
                );
                set_doc.insert(
                    "result_event_id",
                    doc! {
                        "$cond": [
                            "$async_execution",
                            result_event_id,
                            bson::Bson::Null,
                        ]
                    },
                );
                set_doc.insert("result_event_pending", "$async_execution");
                collection
                    .find_one_and_update(
                        doc! {
                            "_id": invocation_id,
                            "status": { "$in": from.iter().map(|status| status.as_str()).collect::<Vec<_>>() }
                        },
                        vec![doc! { "$set": set_doc }],
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(|error| format!("finish Runtime Invocation failed: {error}"))
            }
        }?;
        if let Some(record) = transitioned_record.as_ref() {
            if let Err(error) = self.quota.release(record).await {
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release terminal Runtime Invocation quota reservation failed"
                );
            }
            self.result_event_notify.notify_one();
        }
        Ok(transitioned_record.is_some())
    }
}

fn pending_result_event_from_record(
    record: RuntimeInvocationRecord,
) -> Result<PendingRuntimeInvocationResultEvent, String> {
    let reply_to = record
        .result_reply_to
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "pending Runtime Invocation {} is missing result_reply_to",
                record.invocation_id
            )
        })?;
    let event_id = record.result_event_id.clone().ok_or_else(|| {
        format!(
            "pending Runtime Invocation {} is missing result_event_id",
            record.invocation_id
        )
    })?;
    let status = match record.status {
        RuntimeInvocationStatus::Completed => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Completed
        }
        RuntimeInvocationStatus::Failed => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Failed
        }
        RuntimeInvocationStatus::Cancelled => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Cancelled
        }
        RuntimeInvocationStatus::UnknownExecutionState => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::UnknownExecutionState
        }
        other => {
            return Err(format!(
                "pending Runtime Invocation {} has non-terminal status {}",
                record.invocation_id,
                other.as_str()
            ))
        }
    };
    Ok(PendingRuntimeInvocationResultEvent {
        reply_to,
        event: chatos_mcp_management_sdk::RuntimeInvocationResultEvent {
            event_id,
            correlation_id: record.request_id_key,
            invocation_id: record.invocation_id,
            session_id: record.session_id,
            caller_service: record.caller_service,
            resource_id: record.resource_id,
            exposed_tool_name: record.exposed_tool_name,
            status,
            occurred_at_unix_ms: record
                .completed_at_unix_ms
                .unwrap_or(record.created_at_unix_ms),
            terminal_result: record.terminal_result,
            terminal_error_code: record.terminal_error_code,
            terminal_error_message: record.terminal_error_message,
        },
    })
}

fn terminal_file_modification_outcome(
    status: RuntimeInvocationStatus,
    result: Option<&Value>,
    error_message: Option<&str>,
) -> Option<FileModificationOutcome> {
    match status {
        RuntimeInvocationStatus::Completed => result
            .and_then(file_modification_outcome_from_result)
            .or(Some(FileModificationOutcome::Changed)),
        RuntimeInvocationStatus::Failed => error_message.map(classify_file_modification_error),
        RuntimeInvocationStatus::Queued
        | RuntimeInvocationStatus::Running
        | RuntimeInvocationStatus::WaitingForUser
        | RuntimeInvocationStatus::CancelRequested
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => None,
    }
}

fn file_modification_outcome_from_result(result: &Value) -> Option<FileModificationOutcome> {
    let payload = result.get("_structured_result").unwrap_or(result);
    if let Some(outcome) = payload.get("outcome").and_then(Value::as_str) {
        return match outcome {
            "changed" => Some(FileModificationOutcome::Changed),
            "already_applied" => Some(FileModificationOutcome::AlreadyApplied),
            "stale" => Some(FileModificationOutcome::Stale),
            "validation" => Some(FileModificationOutcome::Validation),
            "infrastructure" => Some(FileModificationOutcome::Infrastructure),
            _ => None,
        };
    }
    let result_payload = payload.get("result").unwrap_or(payload);
    if result_payload
        .get("already_applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some(FileModificationOutcome::AlreadyApplied);
    }
    result_payload
        .get("changed")
        .and_then(Value::as_bool)
        .map(FileModificationOutcome::from_changed)
}

fn is_file_modification_tool(original_tool_name: &str) -> bool {
    matches!(original_tool_name, "edit_file" | "apply_patch" | "patch")
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
    quota_limits: RuntimeInvocationQuotaLimits,
    records: impl IntoIterator<
        Item = (
            RuntimeInvocationStatus,
            bool,
            Option<FileModificationOutcome>,
        ),
    >,
) -> RuntimeInvocationStoreStats {
    let mut stats = RuntimeInvocationStoreStats {
        backend,
        quota_limits,
        total_active: 0,
        queued: 0,
        running: 0,
        waiting_for_user: 0,
        cancel_requested: 0,
        terminal: 0,
        pending_result_events: 0,
        file_modifications: FileModificationOutcomeStats::default(),
    };
    for (status, result_event_pending, file_modification_outcome) in records {
        stats.total_active = stats.total_active.saturating_add(1);
        if result_event_pending {
            stats.pending_result_events = stats.pending_result_events.saturating_add(1);
        }
        match status {
            RuntimeInvocationStatus::Queued => stats.queued = stats.queued.saturating_add(1),
            RuntimeInvocationStatus::Running => stats.running = stats.running.saturating_add(1),
            RuntimeInvocationStatus::WaitingForUser => {
                stats.waiting_for_user = stats.waiting_for_user.saturating_add(1)
            }
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
        if let Some(outcome) = file_modification_outcome {
            stats.file_modifications.record(outcome);
        }
    }
    stats
}

impl FileModificationOutcomeStats {
    fn record(&mut self, outcome: FileModificationOutcome) {
        self.total = self.total.saturating_add(1);
        let counter = match outcome {
            FileModificationOutcome::Changed => &mut self.changed,
            FileModificationOutcome::AlreadyApplied => &mut self.already_applied,
            FileModificationOutcome::Stale => &mut self.stale,
            FileModificationOutcome::Validation => &mut self.validation,
            FileModificationOutcome::Infrastructure => &mut self.infrastructure,
        };
        *counter = counter.saturating_add(1);
    }
}

async fn aggregate_runtime_invocation_stats(
    collection: &Collection<RuntimeInvocationRecord>,
    now: DateTime,
    quota_limits: RuntimeInvocationQuotaLimits,
) -> Result<RuntimeInvocationStoreStats, String> {
    let terminal_statuses = vec![
        RuntimeInvocationStatus::Completed.as_str(),
        RuntimeInvocationStatus::Failed.as_str(),
        RuntimeInvocationStatus::Cancelled.as_str(),
        RuntimeInvocationStatus::UnknownExecutionState.as_str(),
    ];
    let mut cursor = collection
        .aggregate(
            vec![
                doc! { "$match": { "expires_at": { "$gt": now } } },
                doc! {
                    "$group": {
                        "_id": bson::Bson::Null,
                        "total_active": { "$sum": 1 },
                        "queued": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::Queued.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "running": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::Running.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "waiting_for_user": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::WaitingForUser.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "cancel_requested": {
                            "$sum": { "$cond": [
                                { "$eq": ["$status", RuntimeInvocationStatus::CancelRequested.as_str()] },
                                1,
                                0,
                            ] }
                        },
                        "terminal": {
                            "$sum": { "$cond": [
                                { "$in": ["$status", terminal_statuses] },
                                1,
                                0,
                            ] }
                        },
                        "pending_result_events": {
                            "$sum": { "$cond": [
                                { "$eq": ["$result_event_pending", true] },
                                1,
                                0,
                            ] }
                        },
                        "file_modification_total": {
                            "$sum": { "$cond": [
                                { "$in": ["$file_modification_outcome", [
                                    "changed",
                                    "already_applied",
                                    "stale",
                                    "validation",
                                    "infrastructure",
                                ]] },
                                1,
                                0,
                            ] }
                        },
                        "file_modification_changed": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "changed"] }, 1, 0
                            ] }
                        },
                        "file_modification_already_applied": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "already_applied"] }, 1, 0
                            ] }
                        },
                        "file_modification_stale": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "stale"] }, 1, 0
                            ] }
                        },
                        "file_modification_validation": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "validation"] }, 1, 0
                            ] }
                        },
                        "file_modification_infrastructure": {
                            "$sum": { "$cond": [
                                { "$eq": ["$file_modification_outcome", "infrastructure"] }, 1, 0
                            ] }
                        },
                    }
                },
            ],
            None,
        )
        .await
        .map_err(|error| format!("aggregate Runtime Invocation stats failed: {error}"))?;
    let Some(document) = cursor
        .try_next()
        .await
        .map_err(|error| format!("read Runtime Invocation stats failed: {error}"))?
    else {
        return Ok(RuntimeInvocationStoreStats {
            backend: "mongo",
            quota_limits,
            total_active: 0,
            queued: 0,
            running: 0,
            waiting_for_user: 0,
            cancel_requested: 0,
            terminal: 0,
            pending_result_events: 0,
            file_modifications: FileModificationOutcomeStats::default(),
        });
    };
    Ok(RuntimeInvocationStoreStats {
        backend: "mongo",
        quota_limits,
        total_active: runtime_stat_count(&document, "total_active"),
        queued: runtime_stat_count(&document, "queued"),
        running: runtime_stat_count(&document, "running"),
        waiting_for_user: runtime_stat_count(&document, "waiting_for_user"),
        cancel_requested: runtime_stat_count(&document, "cancel_requested"),
        terminal: runtime_stat_count(&document, "terminal"),
        pending_result_events: runtime_stat_count(&document, "pending_result_events"),
        file_modifications: FileModificationOutcomeStats {
            total: runtime_stat_count(&document, "file_modification_total"),
            changed: runtime_stat_count(&document, "file_modification_changed"),
            already_applied: runtime_stat_count(&document, "file_modification_already_applied"),
            stale: runtime_stat_count(&document, "file_modification_stale"),
            validation: runtime_stat_count(&document, "file_modification_validation"),
            infrastructure: runtime_stat_count(&document, "file_modification_infrastructure"),
        },
    })
}

fn runtime_stat_count(document: &mongodb::bson::Document, key: &str) -> usize {
    let value = match document.get(key) {
        Some(bson::Bson::Int32(value)) => i64::from(*value),
        Some(bson::Bson::Int64(value)) => *value,
        Some(bson::Bson::Double(value)) if value.is_finite() => *value as i64,
        _ => 0,
    };
    usize::try_from(value.max(0)).unwrap_or(usize::MAX)
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
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            device_id: None,
            resource_id: "mcp-1".to_string(),
            exposed_tool_name: "demo_read".to_string(),
            original_tool_name: "demo_read".to_string(),
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
            file_modification_outcome: None,
            result_reply_to: None,
            result_event_id: None,
            result_event_pending: false,
            expires_at: DateTime::from_millis((chrono::Utc::now().timestamp() + 60) * 1_000),
            expires_at_unix: chrono::Utc::now().timestamp() + 60,
        }
    }

    #[tokio::test]
    async fn cloned_store_coordinates_cancel_and_terminal_transition() {
        let writer = RuntimeInvocationStore::memory();
        let reader = writer.clone();
        writer.register(record()).await.unwrap();
        assert!(writer.mark_waiting_for_user("invocation-1").await.unwrap());
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
    async fn cancellation_waiter_is_released_by_event_signal_without_polling() {
        let store = RuntimeInvocationStore::memory();
        store.register(record()).await.unwrap();
        let waiter_store = store.clone();
        let waiter =
            tokio::spawn(async move { waiter_store.wait_for_cancellation("invocation-1").await });
        tokio::task::yield_now().await;

        store
            .request_cancel_by_invocation("invocation-1", "task-runner")
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancellation waiter should be event-driven")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn async_terminal_transition_creates_acknowledgeable_result_event() {
        let store = RuntimeInvocationStore::memory();
        let mut invocation = record();
        invocation.async_execution = true;
        invocation.result_reply_to = Some("task_runner.mcp.results.worker-1".to_string());
        store.register(invocation).await.unwrap();

        assert!(store
            .complete("invocation-1", serde_json::json!({"ok": true}))
            .await
            .unwrap());
        let pending = store.pending_result_events(10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event.correlation_id, "\"request-1\"");
        assert_eq!(
            pending[0].event.terminal_result,
            Some(serde_json::json!({"ok": true}))
        );
        assert!(store
            .acknowledge_result_event(
                pending[0].event.invocation_id.as_str(),
                pending[0].event.event_id.as_str(),
            )
            .await
            .unwrap());
        assert!(store.pending_result_events(10).await.unwrap().is_empty());
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
    async fn atomic_quota_rejects_excess_calls_and_terminal_state_releases_capacity() {
        let store = RuntimeInvocationStore::memory_with_quota(
            RuntimeInvocationQuotaLimits::new(1, 1, 1, 1).unwrap(),
        );
        let first = record();
        store.register(first.clone()).await.unwrap();

        let mut second = record();
        second.invocation_id = "invocation-2".to_string();
        second.session_id = "session-2".to_string();
        second.request_id_key = "\"request-2\"".to_string();
        assert_eq!(
            store.register(second.clone()).await.unwrap_err(),
            RuntimeInvocationRegisterError::CapacityExhausted {
                dimension: "tenant",
                limit: 1,
            }
        );

        assert!(store
            .complete(
                first.invocation_id.as_str(),
                serde_json::json!({"ok": true})
            )
            .await
            .unwrap());
        store.register(second).await.unwrap();
    }

    #[tokio::test]
    async fn waiting_for_user_keeps_quota_reserved_until_completion() {
        let store = RuntimeInvocationStore::memory_with_quota(
            RuntimeInvocationQuotaLimits::new(1, 1, 1, 1).unwrap(),
        );
        let first = record();
        store.register(first.clone()).await.unwrap();
        assert!(store
            .mark_waiting_for_user(first.invocation_id.as_str())
            .await
            .unwrap());

        let mut second = record();
        second.invocation_id = "invocation-waiting-quota-2".to_string();
        second.session_id = "session-waiting-quota-2".to_string();
        second.request_id_key = "\"request-waiting-quota-2\"".to_string();
        assert!(matches!(
            store.register(second.clone()).await,
            Err(RuntimeInvocationRegisterError::CapacityExhausted { .. })
        ));

        assert!(store
            .complete(
                first.invocation_id.as_str(),
                serde_json::json!({"answer": "yes"})
            )
            .await
            .unwrap());
        store.register(second).await.unwrap();
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
        let quota = RuntimeInvocationQuota::memory(
            RuntimeInvocationQuotaLimits::new(100, 100, 100, 100).unwrap(),
        );
        let first = RuntimeInvocationStore::connect(database_url.as_str(), quota.clone())
            .await
            .unwrap();
        let second = RuntimeInvocationStore::connect(database_url.as_str(), quota)
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
    async fn queued_invocation_can_fail_before_provider_execution_starts() {
        let store = RuntimeInvocationStore::memory();
        let mut queued = record();
        queued.invocation_id = "invocation-queued-dispatch-failure".to_string();
        queued.status = RuntimeInvocationStatus::Queued;
        queued.started_at_unix_ms = None;
        queued.async_execution = true;
        store.register(queued).await.unwrap();

        assert!(store
            .fail(
                "invocation-queued-dispatch-failure",
                -32000,
                "dispatch retries exhausted",
            )
            .await
            .unwrap());
        let record = store
            .get_for_caller("invocation-queued-dispatch-failure", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, RuntimeInvocationStatus::Failed);
        assert_eq!(
            record.terminal_error_message.as_deref(),
            Some("dispatch retries exhausted")
        );
    }

    #[tokio::test]
    async fn only_confirmed_dispatch_failure_is_eligible_for_dlq_archive() {
        let store = RuntimeInvocationStore::memory();
        let mut queued = record();
        queued.invocation_id = "invocation-dlq-archive".to_string();
        queued.status = RuntimeInvocationStatus::Queued;
        queued.async_execution = true;
        queued.started_at_unix_ms = None;
        queued.result_reply_to = Some("mcp.results.test".to_string());
        store.register(queued).await.unwrap();
        store
            .fail(
                "invocation-dlq-archive",
                -32603,
                "async tool dispatch failed after 5 attempts: unavailable",
            )
            .await
            .unwrap();

        assert!(store
            .dead_letter_archive_candidate("invocation-dlq-archive")
            .await
            .unwrap()
            .is_none());
        let pending = store.pending_result_events(1).await.unwrap();
        store
            .acknowledge_result_event(
                pending[0].event.invocation_id.as_str(),
                pending[0].event.event_id.as_str(),
            )
            .await
            .unwrap();
        assert!(store
            .dead_letter_archive_candidate("invocation-dlq-archive")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn file_modification_outcomes_are_persisted_and_aggregated() {
        let store = RuntimeInvocationStore::memory();

        let mut already_applied = record();
        already_applied.invocation_id = "invocation-edit-already-applied".to_string();
        already_applied.session_id = "session-edit-already-applied".to_string();
        already_applied.request_id_key = "\"request-edit-already-applied\"".to_string();
        already_applied.exposed_tool_name = "harness_code_edit_file".to_string();
        already_applied.original_tool_name = "edit_file".to_string();
        store.register(already_applied).await.unwrap();
        assert!(store
            .complete(
                "invocation-edit-already-applied",
                serde_json::json!({
                    "_structured_result": {
                        "outcome": "already_applied",
                        "changed": false
                    }
                }),
            )
            .await
            .unwrap());

        let mut stale = record();
        stale.invocation_id = "invocation-patch-stale".to_string();
        stale.session_id = "session-patch-stale".to_string();
        stale.request_id_key = "\"request-patch-stale\"".to_string();
        stale.exposed_tool_name = "harness_code_apply_patch".to_string();
        stale.original_tool_name = "apply_patch".to_string();
        store.register(stale).await.unwrap();
        assert!(store
            .fail(
                "invocation-patch-stale",
                -32000,
                "Patch context not found in file.",
            )
            .await
            .unwrap());

        let completed = store
            .get_for_caller("invocation-edit-already-applied", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            completed.file_modification_outcome,
            Some(FileModificationOutcome::AlreadyApplied)
        );
        let failed = store
            .get_for_caller("invocation-patch-stale", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            failed.file_modification_outcome,
            Some(FileModificationOutcome::Stale)
        );

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.file_modifications.total, 2);
        assert_eq!(stats.file_modifications.already_applied, 1);
        assert_eq!(stats.file_modifications.stale, 1);
        assert_eq!(stats.file_modifications.changed, 0);
        assert_eq!(stats.file_modifications.validation, 0);
        assert_eq!(stats.file_modifications.infrastructure, 0);
    }

    #[tokio::test]
    async fn non_file_tool_errors_are_not_counted_as_file_modifications() {
        let store = RuntimeInvocationStore::memory();
        store.register(record()).await.unwrap();
        assert!(store
            .fail("invocation-1", -32000, "connection reset")
            .await
            .unwrap());

        let stored = store
            .get_for_caller("invocation-1", "task-runner")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.file_modification_outcome, None);
        assert_eq!(store.stats().await.unwrap().file_modifications.total, 0);
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
        let waiting = RuntimeInvocationRecord {
            invocation_id: "invocation-stats-waiting".to_string(),
            session_id: "session-stats-waiting".to_string(),
            request_id_key: "\"request-stats-waiting\"".to_string(),
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
        store.register(waiting).await.unwrap();
        assert!(store
            .mark_waiting_for_user("invocation-stats-waiting")
            .await
            .unwrap());
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
        assert_eq!(stats.total_active, 4);
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.running, 1);
        assert_eq!(stats.waiting_for_user, 1);
        assert_eq!(stats.cancel_requested, 0);
        assert_eq!(stats.terminal, 1);
        assert_eq!(stats.pending_result_events, 0);
    }
}
