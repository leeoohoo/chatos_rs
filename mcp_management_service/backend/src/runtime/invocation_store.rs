// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, Weak};

use chatos_mcp::code_maintainer::{classify_file_modification_error, FileModificationOutcome};
use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, DateTime};
use mongodb::error::{ErrorKind as MongoErrorKind, WriteFailure};
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
    diagnostics: Arc<RuntimeInvocationDiagnostics>,
    result_event_notify: Arc<Notify>,
    cancellation_waiters: Arc<StdMutex<HashMap<String, Weak<Notify>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeInvocationRegisterError {
    DuplicateActiveId,
    CapacityExhausted { dimension: &'static str, limit: u32 },
    StoreUnavailable(String),
    SessionClosed,
    InvalidRecord(String),
}

impl std::fmt::Display for RuntimeInvocationRegisterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateActiveId => {
                formatter.write_str("JSON-RPC request id is already active in this Runtime Session")
            }
            Self::CapacityExhausted { dimension, limit } => write!(
                formatter,
                "Runtime Invocation {dimension} quota was exhausted at {limit} active calls"
            ),
            Self::StoreUnavailable(error) | Self::InvalidRecord(error) => {
                formatter.write_str(error)
            }
            Self::SessionClosed => formatter.write_str("Runtime Session is closed or expired"),
        }
    }
}

impl RuntimeInvocationRegisterError {
    pub fn category(&self) -> &'static str {
        match self {
            Self::DuplicateActiveId => "duplicate_active_id",
            Self::CapacityExhausted { .. } => "capacity_exhausted",
            Self::StoreUnavailable(_) => "store_unavailable",
            Self::SessionClosed => "session_closed",
            Self::InvalidRecord(_) => "invalid_record",
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
    pub registration: RuntimeInvocationRegistrationStats,
    pub session_closed_reclaimed_total: u64,
    pub quota_release_failures_total: u64,
    pub store_recoveries_total: u64,
    pub duration: RuntimeInvocationDurationStats,
    pub file_modifications: FileModificationOutcomeStats,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RuntimeInvocationRegistrationStats {
    pub duplicate_active_id: u64,
    pub capacity_exhausted: u64,
    pub store_unavailable: u64,
    pub session_closed: u64,
    pub invalid_record: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RuntimeInvocationDurationStats {
    pub completed_count: usize,
    pub total_ms: u64,
    pub max_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FileModificationOutcomeStats {
    pub total: usize,
    pub changed: usize,
    pub already_applied: usize,
    pub stale_context: usize,
    pub expected_match: usize,
    pub validation: usize,
    pub infrastructure: usize,
}

#[derive(Default)]
struct RuntimeInvocationDiagnostics {
    duplicate_active_id: AtomicU64,
    capacity_exhausted: AtomicU64,
    store_unavailable: AtomicU64,
    session_closed: AtomicU64,
    invalid_record: AtomicU64,
    session_closed_reclaimed: AtomicU64,
    quota_release_failures: AtomicU64,
    store_recoveries: AtomicU64,
    store_unavailable_observed: AtomicBool,
}

impl RuntimeInvocationDiagnostics {
    fn registration_stats(&self) -> RuntimeInvocationRegistrationStats {
        RuntimeInvocationRegistrationStats {
            duplicate_active_id: self.duplicate_active_id.load(Ordering::Relaxed),
            capacity_exhausted: self.capacity_exhausted.load(Ordering::Relaxed),
            store_unavailable: self.store_unavailable.load(Ordering::Relaxed),
            session_closed: self.session_closed.load(Ordering::Relaxed),
            invalid_record: self.invalid_record.load(Ordering::Relaxed),
        }
    }
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
            diagnostics: Arc::new(RuntimeInvocationDiagnostics::default()),
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
            diagnostics: Arc::new(RuntimeInvocationDiagnostics::default()),
            result_event_notify: Arc::new(Notify::new()),
            cancellation_waiters: Arc::new(StdMutex::new(HashMap::new())),
        })
    }

    pub async fn register(
        &self,
        record: RuntimeInvocationRecord,
    ) -> Result<(), RuntimeInvocationRegisterError> {
        let result = self.register_inner(record).await;
        self.observe_register_result(&result);
        result
    }

    async fn register_inner(
        &self,
        record: RuntimeInvocationRecord,
    ) -> Result<(), RuntimeInvocationRegisterError> {
        if !matches!(
            record.status,
            RuntimeInvocationStatus::Queued | RuntimeInvocationStatus::Running
        ) {
            return Err(RuntimeInvocationRegisterError::InvalidRecord(
                "new Runtime Invocation must start in queued or running state".to_string(),
            ));
        }
        if record.expires_at_unix <= chrono::Utc::now().timestamp() {
            return Err(RuntimeInvocationRegisterError::SessionClosed);
        }
        self.quota
            .reserve(&record)
            .await
            .map_err(|error| match error {
                RuntimeInvocationQuotaReserveError::CapacityExhausted { dimension, limit } => {
                    RuntimeInvocationRegisterError::CapacityExhausted { dimension, limit }
                }
                RuntimeInvocationQuotaReserveError::Infrastructure(error) => {
                    RuntimeInvocationRegisterError::StoreUnavailable(error)
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
                    Err(RuntimeInvocationRegisterError::CapacityExhausted {
                        dimension: "store",
                        limit: MAX_MEMORY_INVOCATIONS as u32,
                    })
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
                    Err(RuntimeInvocationRegisterError::DuplicateActiveId)
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
                        RuntimeInvocationRegisterError::StoreUnavailable(format!(
                            "remove prior terminal Runtime Invocation failed: {error}"
                        ))
                    })?;
                match collection.insert_one(record.clone(), None).await {
                    Ok(_) => Ok(()),
                    Err(error) if is_mongodb_duplicate_key(&error) => {
                        match collection
                            .find_one(
                                doc! {
                                    "session_id": record.session_id.as_str(),
                                    "request_id_key": record.request_id_key.as_str(),
                                },
                                None,
                            )
                            .await
                        {
                            Ok(Some(existing))
                                if existing.invocation_id == record.invocation_id =>
                            {
                                Ok(())
                            }
                            Ok(Some(_)) => Err(RuntimeInvocationRegisterError::DuplicateActiveId),
                            Ok(None) => {
                                match collection
                                    .find_one(
                                        doc! { "_id": record.invocation_id.as_str() },
                                        None,
                                    )
                                    .await
                                {
                                    Ok(Some(_)) => Err(
                                        RuntimeInvocationRegisterError::InvalidRecord(
                                            "Runtime Invocation id is already in use".to_string(),
                                        ),
                                    ),
                                    Ok(None) => Err(RuntimeInvocationRegisterError::StoreUnavailable(
                                        "MongoDB reported a duplicate Runtime Invocation key without a matching record"
                                            .to_string(),
                                    )),
                                    Err(lookup_error) => Err(
                                        RuntimeInvocationRegisterError::StoreUnavailable(format!(
                                            "verify duplicate Runtime Invocation id failed: {lookup_error}"
                                        )),
                                    ),
                                }
                            }
                            Err(lookup_error) => {
                                Err(RuntimeInvocationRegisterError::StoreUnavailable(format!(
                                    "verify duplicate Runtime Invocation failed: {lookup_error}"
                                )))
                            }
                        }
                    }
                    Err(error) => Err(RuntimeInvocationRegisterError::StoreUnavailable(format!(
                        "register Runtime Invocation failed: {error}"
                    ))),
                }
            }
        };
        if result.is_err() {
            if let Err(error) = self.quota.release(&record).await {
                self.diagnostics
                    .quota_release_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release rejected Runtime Invocation quota reservation failed"
                );
            }
        }
        result
    }

    pub fn observe_register_error(&self, error: &RuntimeInvocationRegisterError) {
        self.observe_register_result(&Err(error.clone()));
    }

    fn observe_register_result(&self, result: &Result<(), RuntimeInvocationRegisterError>) {
        match result {
            Ok(()) => {
                if self
                    .diagnostics
                    .store_unavailable_observed
                    .swap(false, Ordering::Relaxed)
                {
                    self.diagnostics
                        .store_recoveries
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(RuntimeInvocationRegisterError::DuplicateActiveId) => {
                self.diagnostics
                    .duplicate_active_id
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(RuntimeInvocationRegisterError::CapacityExhausted { .. }) => {
                self.diagnostics
                    .capacity_exhausted
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(RuntimeInvocationRegisterError::StoreUnavailable(_)) => {
                self.diagnostics
                    .store_unavailable
                    .fetch_add(1, Ordering::Relaxed);
                self.diagnostics
                    .store_unavailable_observed
                    .store(true, Ordering::Relaxed);
            }
            Err(RuntimeInvocationRegisterError::SessionClosed) => {
                self.diagnostics
                    .session_closed
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(RuntimeInvocationRegisterError::InvalidRecord(_)) => {
                self.diagnostics
                    .invalid_record
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
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
}

fn active_runtime_invocation_statuses() -> &'static [RuntimeInvocationStatus] {
    &[
        RuntimeInvocationStatus::Queued,
        RuntimeInvocationStatus::Running,
        RuntimeInvocationStatus::WaitingForUser,
        RuntimeInvocationStatus::CancelRequested,
    ]
}

fn is_mongodb_duplicate_key(error: &mongodb::error::Error) -> bool {
    match error.kind.as_ref() {
        MongoErrorKind::Write(WriteFailure::WriteError(error)) => error.code == 11_000,
        MongoErrorKind::BulkWrite(failure) => failure
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|error| error.code == 11_000)),
        _ => false,
    }
}

#[path = "invocation_store/coordination.rs"]
mod coordination;
#[path = "invocation_store/lifecycle.rs"]
mod lifecycle;
#[path = "invocation_store/stats.rs"]
mod stats;
#[cfg(test)]
#[path = "invocation_store/tests.rs"]
mod tests;
