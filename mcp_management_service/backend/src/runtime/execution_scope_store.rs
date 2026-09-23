// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use chatos_mcp_management_sdk::WorkspaceProviderKind;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio::sync::RwLock;

#[path = "execution_scope_store_turns.rs"]
mod turns;

const ORPHAN_GRACE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeExecutionScopeStoreError {
    Terminal,
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeExecutionTurnState {
    Acquired,
    Waiting,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasedInvocationTurn {
    pub next_invocation_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RuntimeExecutionScopeInvocationRef {
    invocation_id: String,
    sequence: i64,
    batch_id: Option<String>,
    call_index: Option<usize>,
}

#[derive(Debug, Clone)]
struct RuntimeExecutionScopeDocument {
    generation: i64,
    status: String,
    session_refs: HashMap<String, i64>,
    next_invocation_sequence: i64,
    invocation_queue: Vec<RuntimeExecutionScopeInvocationRef>,
    running_invocation_id: Option<String>,
    expires_at_unix: i64,
}

impl std::fmt::Display for RuntimeExecutionScopeStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal => formatter.write_str("runtime run is already terminal"),
            Self::Unavailable(error) => formatter.write_str(error),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeExecutionScopeStore {
    backend: Arc<RuntimeExecutionScopeStoreBackend>,
}

enum RuntimeExecutionScopeStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeExecutionScopeDocument>>),
    Postgres(chatos_postgres::PgPool),
}

impl RuntimeExecutionScopeStore {
    #[cfg(test)]
    pub async fn queued_invocation_ids(&self) -> Vec<String> {
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => scopes
                .read()
                .await
                .values()
                .flat_map(|scope| {
                    scope
                        .invocation_queue
                        .iter()
                        .map(|reference| reference.invocation_id.clone())
                })
                .collect(),
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => sqlx::query_scalar::<_, String>(
                "SELECT invocation_id FROM mcp_management_runtime_execution_scope_queue_items \
                 WHERE status='queued' ORDER BY scope_id,sequence",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
        }
    }

    pub fn memory() -> Self {
        Self {
            backend: Arc::new(RuntimeExecutionScopeStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
        }
    }

    pub async fn connect(database_url: &str) -> Result<Self, String> {
        Ok(Self::from_pool(
            crate::postgres::connect(database_url).await?,
        ))
    }

    pub(crate) fn from_pool(pool: chatos_postgres::PgPool) -> Self {
        Self {
            backend: Arc::new(RuntimeExecutionScopeStoreBackend::Postgres(pool)),
        }
    }

    pub async fn attach_session(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        session_id: &str,
        session_expires_at_unix: i64,
    ) -> Result<i64, RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        let now = Utc::now().timestamp();
        let expires_at_unix = session_expires_at_unix.saturating_add(ORPHAN_GRACE_SECONDS);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                scopes.retain(|_, scope| scope.expires_at_unix > now);
                if scopes
                    .get(id.as_str())
                    .is_some_and(|scope| scope.status == "terminal")
                {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                let scope = scopes
                    .entry(id)
                    .or_insert_with(|| RuntimeExecutionScopeDocument {
                        generation: 1,
                        status: "active".to_string(),
                        session_refs: HashMap::new(),
                        next_invocation_sequence: 0,
                        invocation_queue: Vec::new(),
                        running_invocation_id: None,
                        expires_at_unix,
                    });
                scope
                    .session_refs
                    .retain(|_, expires_at_unix| *expires_at_unix > now);
                scope
                    .session_refs
                    .insert(session_id.to_string(), session_expires_at_unix);
                scope.expires_at_unix = scope.expires_at_unix.max(expires_at_unix);
                Ok(scope.generation)
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let expires_at = timestamp(expires_at_unix).map_err(unavailable)?;
                let mut tx = pool.begin().await.map_err(store_error)?;
                lock_scope_id(&mut tx, id.as_str())
                    .await
                    .map_err(store_error)?;
                sqlx::query(
                    "DELETE FROM mcp_management_runtime_execution_scopes \
                     WHERE id=$1 AND expires_at<=now()",
                )
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(store_error)?;
                let existing = sqlx::query_as::<_, (i64, String, Json<HashMap<String, i64>>, i64)>(
                    "SELECT generation,status,session_refs,expires_at_unix \
                         FROM mcp_management_runtime_execution_scopes WHERE id=$1 FOR UPDATE",
                )
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(store_error)?;
                let generation = if let Some((generation, status, Json(mut refs), old_expiry)) =
                    existing
                {
                    if status == "terminal" {
                        return Err(RuntimeExecutionScopeStoreError::Terminal);
                    }
                    refs.retain(|_, expiry| *expiry > now);
                    refs.insert(session_id.to_string(), session_expires_at_unix);
                    let new_expiry = old_expiry.max(expires_at_unix);
                    sqlx::query(
                        "UPDATE mcp_management_runtime_execution_scopes SET session_refs=$2, \
                         updated_at=now(),expires_at=$3,expires_at_unix=$4 WHERE id=$1",
                    )
                    .bind(&id)
                    .bind(Json(refs))
                    .bind(timestamp(new_expiry).map_err(unavailable)?)
                    .bind(new_expiry)
                    .execute(&mut *tx)
                    .await
                    .map_err(store_error)?;
                    generation
                } else {
                    let mut refs = HashMap::new();
                    refs.insert(session_id.to_string(), session_expires_at_unix);
                    sqlx::query(
                        "INSERT INTO mcp_management_runtime_execution_scopes \
                         (id,owner_user_id,scope_kind,project_id,run_id,provider,generation,status, \
                          terminal_status,next_invocation_sequence,running_invocation_id,session_refs, \
                          updated_at,expires_at,expires_at_unix) \
                         VALUES($1,$2,$3,$4,$5,$6,1,'active',NULL,0,NULL,$7,now(),$8,$9)",
                    )
                    .bind(&id)
                    .bind(owner_user_id)
                    .bind(execution_scope_kind(project_id))
                    .bind(project_id)
                    .bind(run_id)
                    .bind(provider.as_str())
                    .bind(Json(refs))
                    .bind(expires_at)
                    .bind(expires_at_unix)
                    .execute(&mut *tx)
                    .await
                    .map_err(store_error)?;
                    1
                };
                tx.commit().await.map_err(store_error)?;
                Ok(generation)
            }
        }
    }

    pub async fn detach_session(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        session_id: &str,
    ) -> Result<bool, String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let remove = if let Some(scope) = scopes.get_mut(id.as_str()) {
                    scope.session_refs.remove(session_id);
                    scope.session_refs.is_empty()
                        && scope.invocation_queue.is_empty()
                        && scope.running_invocation_id.is_none()
                } else {
                    false
                };
                if remove {
                    scopes.remove(id.as_str());
                }
                Ok(remove)
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let scope = sqlx::query_as::<_, (Json<HashMap<String, i64>>, Option<String>)>(
                    "SELECT session_refs,running_invocation_id \
                         FROM mcp_management_runtime_execution_scopes WHERE id=$1 FOR UPDATE",
                )
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                let Some((Json(mut refs), running)) = scope else {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(false);
                };
                refs.remove(session_id);
                let queue_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1",
                )
                .bind(&id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                let remove = refs.is_empty() && running.is_none() && queue_count == 0;
                if remove {
                    sqlx::query("DELETE FROM mcp_management_runtime_execution_scopes WHERE id=$1")
                        .bind(&id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|error| error.to_string())?;
                } else {
                    sqlx::query(
                        "UPDATE mcp_management_runtime_execution_scopes \
                         SET session_refs=$2,updated_at=now() WHERE id=$1",
                    )
                    .bind(&id)
                    .bind(Json(refs))
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?;
                }
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(remove)
            }
        }
    }

    pub async fn ensure_accepting_invocations(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
    ) -> Result<(), RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        let status = match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => scopes
                .read()
                .await
                .get(id.as_str())
                .map(|scope| scope.status.clone()),
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => sqlx::query_scalar::<_, String>(
                "SELECT status FROM mcp_management_runtime_execution_scopes WHERE id=$1",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await
            .map_err(store_error)?,
        };
        if status.as_deref() == Some("terminal") {
            Err(RuntimeExecutionScopeStoreError::Terminal)
        } else {
            Ok(())
        }
    }

    pub async fn enqueue_invocation(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<i64, RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let scope = scopes.get_mut(id.as_str()).ok_or_else(|| {
                    unavailable("execution scope is missing while enqueueing an invocation")
                })?;
                if scope.status == "terminal" {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                if let Some(reference) = scope
                    .invocation_queue
                    .iter()
                    .find(|reference| reference.invocation_id == invocation_id)
                {
                    return Ok(reference.sequence);
                }
                if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                    return Ok(scope.next_invocation_sequence);
                }
                scope.next_invocation_sequence = scope.next_invocation_sequence.saturating_add(1);
                let sequence = scope.next_invocation_sequence;
                scope
                    .invocation_queue
                    .push(RuntimeExecutionScopeInvocationRef {
                        invocation_id: invocation_id.to_string(),
                        sequence,
                        batch_id: None,
                        call_index: None,
                    });
                Ok(sequence)
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(store_error)?;
                let (_, status) = lock_scope(&mut tx, id.as_str())
                    .await
                    .map_err(store_error)?
                    .ok_or_else(|| {
                        unavailable("execution scope is missing while enqueueing an invocation")
                    })?;
                if status == "terminal" {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                if let Some((existing_scope, sequence)) = sqlx::query_as::<_, (String, i64)>(
                    "SELECT scope_id,sequence \
                         FROM mcp_management_runtime_execution_scope_queue_items \
                         WHERE invocation_id=$1",
                )
                .bind(invocation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(store_error)?
                {
                    if existing_scope == id {
                        tx.commit().await.map_err(store_error)?;
                        return Ok(sequence);
                    }
                    return Err(unavailable(
                        "execution scope invocation id is active in another scope",
                    ));
                }
                let sequence = increment_sequence(&mut tx, id.as_str(), 1)
                    .await
                    .map_err(store_error)?;
                sqlx::query(
                    "INSERT INTO mcp_management_runtime_execution_scope_queue_items \
                     (scope_id,invocation_id,sequence,batch_id,call_index,status) \
                     VALUES($1,$2,$3,NULL,NULL,'queued')",
                )
                .bind(&id)
                .bind(invocation_id)
                .bind(sequence)
                .execute(&mut *tx)
                .await
                .map_err(store_error)?;
                tx.commit().await.map_err(store_error)?;
                Ok(sequence)
            }
        }
    }

    pub async fn enqueue_invocation_batch(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        batch_id: &str,
        invocations: &[(String, usize)],
    ) -> Result<Vec<i64>, RuntimeExecutionScopeStoreError> {
        if invocations.is_empty() {
            return Ok(Vec::new());
        }
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let scope = scopes.get_mut(id.as_str()).ok_or_else(|| {
                    unavailable("execution scope is missing while enqueueing an invocation batch")
                })?;
                if scope.status == "terminal" {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                if let Some(sequences) = existing_batch_sequences(scope, batch_id, invocations) {
                    return Ok(sequences);
                }
                if invocations.iter().any(|(invocation_id, _)| {
                    scope.running_invocation_id.as_deref() == Some(invocation_id.as_str())
                        || scope
                            .invocation_queue
                            .iter()
                            .any(|reference| reference.invocation_id == *invocation_id)
                }) {
                    return Err(unavailable(
                        "execution scope invocation batch contains an active duplicate",
                    ));
                }
                let mut sequences = Vec::with_capacity(invocations.len());
                for (invocation_id, call_index) in invocations {
                    scope.next_invocation_sequence =
                        scope.next_invocation_sequence.saturating_add(1);
                    sequences.push(scope.next_invocation_sequence);
                    scope
                        .invocation_queue
                        .push(RuntimeExecutionScopeInvocationRef {
                            invocation_id: invocation_id.clone(),
                            sequence: scope.next_invocation_sequence,
                            batch_id: Some(batch_id.to_string()),
                            call_index: Some(*call_index),
                        });
                }
                Ok(sequences)
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(store_error)?;
                let (_, status) = lock_scope(&mut tx, id.as_str())
                    .await
                    .map_err(store_error)?
                    .ok_or_else(|| {
                        unavailable(
                            "execution scope is missing while enqueueing an invocation batch",
                        )
                    })?;
                if status == "terminal" {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                let invocation_ids = invocations
                    .iter()
                    .map(|(invocation_id, _)| invocation_id.clone())
                    .collect::<Vec<_>>();
                let existing =
                    sqlx::query_as::<_, (String, String, i64, Option<String>, Option<i64>)>(
                        "SELECT scope_id,invocation_id,sequence,batch_id,call_index \
                     FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE invocation_id=ANY($1)",
                    )
                    .bind(&invocation_ids)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(store_error)?;
                if !existing.is_empty() {
                    let replay = invocations
                        .iter()
                        .map(|(invocation_id, call_index)| {
                            existing
                                .iter()
                                .find(
                                    |(scope_id, existing_id, _, existing_batch, existing_index)| {
                                        scope_id == &id
                                            && existing_id == invocation_id
                                            && existing_batch.as_deref() == Some(batch_id)
                                            && *existing_index == i64::try_from(*call_index).ok()
                                    },
                                )
                                .map(|(_, _, sequence, _, _)| *sequence)
                        })
                        .collect::<Option<Vec<_>>>();
                    if let Some(sequences) = replay {
                        tx.commit().await.map_err(store_error)?;
                        return Ok(sequences);
                    }
                    return Err(unavailable(
                        "execution scope invocation batch contains an active duplicate",
                    ));
                }
                let count = i64::try_from(invocations.len())
                    .map_err(|_| unavailable("execution scope invocation batch is too large"))?;
                let last_sequence = increment_sequence(&mut tx, id.as_str(), count)
                    .await
                    .map_err(store_error)?;
                let first_sequence = last_sequence.saturating_sub(count).saturating_add(1);
                let mut sequences = Vec::with_capacity(invocations.len());
                for (offset, (invocation_id, call_index)) in invocations.iter().enumerate() {
                    let sequence =
                        first_sequence.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX));
                    let call_index = i64::try_from(*call_index)
                        .map_err(|_| unavailable("execution scope call index is too large"))?;
                    sqlx::query(
                        "INSERT INTO mcp_management_runtime_execution_scope_queue_items \
                         (scope_id,invocation_id,sequence,batch_id,call_index,status) \
                         VALUES($1,$2,$3,$4,$5,'queued')",
                    )
                    .bind(&id)
                    .bind(invocation_id)
                    .bind(sequence)
                    .bind(batch_id)
                    .bind(call_index)
                    .execute(&mut *tx)
                    .await
                    .map_err(store_error)?;
                    sequences.push(sequence);
                }
                tx.commit().await.map_err(store_error)?;
                Ok(sequences)
            }
        }
    }
}

async fn lock_scope(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<Option<(Option<String>, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT running_invocation_id,status \
         FROM mcp_management_runtime_execution_scopes WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

async fn lock_scope_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
}

async fn increment_sequence(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
    count: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "UPDATE mcp_management_runtime_execution_scopes \
         SET next_invocation_sequence=next_invocation_sequence+$2,updated_at=now() \
         WHERE id=$1 RETURNING next_invocation_sequence",
    )
    .bind(id)
    .bind(count)
    .fetch_one(&mut **tx)
    .await
}

fn existing_batch_sequences(
    scope: &RuntimeExecutionScopeDocument,
    batch_id: &str,
    invocations: &[(String, usize)],
) -> Option<Vec<i64>> {
    invocations
        .iter()
        .map(|(invocation_id, call_index)| {
            scope
                .invocation_queue
                .iter()
                .find(|reference| {
                    reference.invocation_id == *invocation_id
                        && reference.batch_id.as_deref() == Some(batch_id)
                        && reference.call_index == Some(*call_index)
                })
                .map(|reference| reference.sequence)
        })
        .collect()
}

fn scope_id(
    owner_user_id: &str,
    project_id: Option<&str>,
    run_id: &str,
    provider: WorkspaceProviderKind,
) -> String {
    let scope_identity = match project_id {
        Some(project_id) => format!("project:{}", project_id.trim()),
        None => "user_conversation".to_string(),
    };
    let identity = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        owner_user_id.trim(),
        scope_identity,
        run_id.trim(),
        provider.as_str()
    );
    format!(
        "execution_scope_{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    )
}

fn execution_scope_kind(project_id: Option<&str>) -> &'static str {
    if project_id.is_some() {
        "project"
    } else {
        "user_conversation"
    }
}

fn timestamp(value: i64) -> Result<DateTime<Utc>, String> {
    DateTime::<Utc>::from_timestamp(value, 0)
        .ok_or_else(|| "Runtime Execution Scope expiry is outside the supported range".to_string())
}

fn store_error(error: impl std::fmt::Display) -> RuntimeExecutionScopeStoreError {
    unavailable(format!("Runtime Execution Scope store failed: {error}"))
}

fn unavailable(message: impl Into<String>) -> RuntimeExecutionScopeStoreError {
    RuntimeExecutionScopeStoreError::Unavailable(message.into())
}

#[cfg(test)]
include!("execution_scope_store_inline_tests.rs");
