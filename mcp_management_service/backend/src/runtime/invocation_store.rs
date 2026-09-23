// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, Weak};

use chatos_mcp::code_maintainer::{classify_file_modification_error, FileModificationOutcome};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;
use tokio::sync::{Notify, RwLock};

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
    #[serde(default)]
    pub project_id: Option<String>,
    pub device_id: Option<String>,
    pub resource_id: String,
    pub exposed_tool_name: String,
    #[serde(default)]
    pub original_tool_name: String,
    pub mutation_may_have_started: bool,
    pub cancel_supported: bool,
    pub status: RuntimeInvocationStatus,
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
    pub expires_at: DateTime<Utc>,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ExpiredRuntimeInvocationClaim {
    pub record: RuntimeInvocationRecord,
    pub claim_token: String,
    pub cancellation_required: bool,
}

#[derive(Clone)]
pub struct RuntimeInvocationStore {
    backend: Arc<RuntimeInvocationStoreBackend>,
    quota: RuntimeInvocationQuota,
    diagnostics: Arc<RuntimeInvocationDiagnostics>,
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
    Postgres(chatos_postgres::PgPool),
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
            cancellation_waiters: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    pub async fn connect(
        database_url: &str,
        quota: RuntimeInvocationQuota,
    ) -> Result<Self, String> {
        let pool = crate::postgres::connect(database_url).await?;
        Ok(Self::from_pool(pool, quota))
    }

    pub(crate) fn from_pool(pool: chatos_postgres::PgPool, quota: RuntimeInvocationQuota) -> Self {
        Self {
            backend: Arc::new(RuntimeInvocationStoreBackend::Postgres(pool)),
            quota,
            diagnostics: Arc::new(RuntimeInvocationDiagnostics::default()),
            cancellation_waiters: Arc::new(StdMutex::new(HashMap::new())),
        }
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
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| {
                    RuntimeInvocationRegisterError::StoreUnavailable(error.to_string())
                })?;
                sqlx::query(
                    "DELETE FROM mcp_management_runtime_invocations WHERE session_id=$1 AND request_id_key=$2 \
                     AND status IN ('completed','failed','cancelled','unknown_execution_state')",
                )
                .bind(&record.session_id)
                .bind(&record.request_id_key)
                .execute(&mut *tx)
                .await
                .map_err(|error| RuntimeInvocationRegisterError::StoreUnavailable(format!(
                    "remove prior terminal Runtime Invocation failed: {error}"
                )))?;
                match insert_invocation(&mut *tx, &record).await {
                    Ok(()) => {
                        tx.commit().await.map_err(|error| {
                            RuntimeInvocationRegisterError::StoreUnavailable(error.to_string())
                        })?;
                        Ok(())
                    }
                    Err(error) if is_postgres_unique_violation(&error) => {
                        tx.rollback().await.ok();
                        match load_invocation_by_session_request(
                            pool,
                            &record.session_id,
                            &record.request_id_key,
                        )
                        .await
                        {
                            Ok(Some(existing)) if existing.invocation_id == record.invocation_id => {
                                Ok(())
                            }
                            Ok(Some(_)) => Err(RuntimeInvocationRegisterError::DuplicateActiveId),
                            Ok(None) => match load_invocation(pool, &record.invocation_id).await {
                                Ok(Some(_)) => Err(RuntimeInvocationRegisterError::InvalidRecord(
                                    "Runtime Invocation id is already in use".to_string(),
                                )),
                                Ok(None) => Err(RuntimeInvocationRegisterError::StoreUnavailable(
                                    "PostgreSQL reported a duplicate Runtime Invocation without a matching record".to_string(),
                                )),
                                Err(error) => Err(RuntimeInvocationRegisterError::StoreUnavailable(error)),
                            },
                            Err(error) => Err(RuntimeInvocationRegisterError::StoreUnavailable(error)),
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
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                load_invocation_by(
                    pool,
                    "SELECT data FROM mcp_management_runtime_invocations \
                 WHERE invocation_id=$1 AND caller_service=$2 AND expires_at_unix>$3",
                    invocation_id,
                    caller_service,
                    now,
                )
                .await
            }
        }
    }

    pub(crate) async fn get_for_recovery(
        &self,
        invocation_id: &str,
        caller_service: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => Ok(invocations
                .read()
                .await
                .get(invocation_id)
                .filter(|record| record.caller_service == caller_service)
                .cloned()),
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations \
                     WHERE invocation_id=$1 AND caller_service=$2",
                )
                .bind(invocation_id)
                .bind(caller_service)
                .fetch_optional(pool)
                .await
                .map_err(|error| error.to_string())?
                .map(decode_invocation)
                .transpose()
            }
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

fn is_postgres_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
        == Some("23505")
}

async fn insert_invocation<'e, E>(
    executor: E,
    record: &RuntimeInvocationRecord,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let data =
        Json(serde_json::to_value(record).map_err(|error| sqlx::Error::Encode(error.into()))?);
    sqlx::query(
        "INSERT INTO mcp_management_runtime_invocations \
         (invocation_id,session_id,request_id_key,caller_service,tenant_id,owner_user_id,project_id,device_id, \
          resource_id,status,mutation_may_have_started,cancel_supported,created_at_unix_ms,started_at_unix_ms, \
          completed_at_unix_ms,file_modification_outcome,expires_at,expires_at_unix,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
    )
    .bind(&record.invocation_id)
    .bind(&record.session_id)
    .bind(&record.request_id_key)
    .bind(&record.caller_service)
    .bind(&record.tenant_id)
    .bind(&record.owner_user_id)
    .bind(&record.project_id)
    .bind(&record.device_id)
    .bind(&record.resource_id)
    .bind(record.status.as_str())
    .bind(record.mutation_may_have_started)
    .bind(record.cancel_supported)
    .bind(record.created_at_unix_ms)
    .bind(record.started_at_unix_ms)
    .bind(record.completed_at_unix_ms)
    .bind(record.file_modification_outcome.map(|outcome| outcome.as_str()))
    .bind(record.expires_at)
    .bind(record.expires_at_unix)
    .bind(data)
    .execute(executor)
    .await
    .map(|_| ())
}

pub(super) async fn persist_invocation<'e, E>(
    executor: E,
    record: &RuntimeInvocationRecord,
) -> Result<(), String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let data = serde_json::to_value(record)
        .map(Json)
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE mcp_management_runtime_invocations SET status=$1,mutation_may_have_started=$2, \
         started_at_unix_ms=$3,completed_at_unix_ms=$4,file_modification_outcome=$5,data=$6 \
         WHERE invocation_id=$7",
    )
    .bind(record.status.as_str())
    .bind(record.mutation_may_have_started)
    .bind(record.started_at_unix_ms)
    .bind(record.completed_at_unix_ms)
    .bind(
        record
            .file_modification_outcome
            .map(|outcome| outcome.as_str()),
    )
    .bind(data)
    .bind(&record.invocation_id)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub(super) async fn load_invocation(
    pool: &chatos_postgres::PgPool,
    invocation_id: &str,
) -> Result<Option<RuntimeInvocationRecord>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM mcp_management_runtime_invocations WHERE invocation_id=$1",
    )
    .bind(invocation_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .map(decode_invocation)
    .transpose()
}

async fn load_invocation_by_session_request(
    pool: &chatos_postgres::PgPool,
    session_id: &str,
    request_id_key: &str,
) -> Result<Option<RuntimeInvocationRecord>, String> {
    load_invocation_by(
        pool,
        "SELECT data FROM mcp_management_runtime_invocations \
         WHERE session_id=$1 AND request_id_key=$2 AND expires_at_unix>$3",
        session_id,
        request_id_key,
        chrono::Utc::now().timestamp(),
    )
    .await
}

async fn load_invocation_by(
    pool: &chatos_postgres::PgPool,
    query: &str,
    first: &str,
    second: &str,
    now: i64,
) -> Result<Option<RuntimeInvocationRecord>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(query)
        .bind(first)
        .bind(second)
        .bind(now)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .map(decode_invocation)
        .transpose()
}

pub(super) fn decode_invocation(
    value: Json<serde_json::Value>,
) -> Result<RuntimeInvocationRecord, String> {
    serde_json::from_value(value.0).map_err(|error| error.to_string())
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
