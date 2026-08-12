// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chatos_mcp_management_sdk::{RuntimeRunTerminalStatus, WorkspaceProviderKind};
use mongodb::bson::{doc, DateTime};
use mongodb::error::{ErrorKind as MongoErrorKind, WriteFailure};
use mongodb::options::{FindOneAndUpdateOptions, IndexOptions, ReturnDocument};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

const ORPHAN_GRACE_SECONDS: i64 = 60;
const TERMINAL_TOMBSTONE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeExecutionScopeInvocationRef {
    invocation_id: String,
    sequence: i64,
    #[serde(default)]
    batch_id: Option<String>,
    #[serde(default)]
    call_index: Option<usize>,
}

impl std::fmt::Display for RuntimeExecutionScopeStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal => formatter.write_str("runtime run is already terminal"),
            Self::Unavailable(error) => formatter.write_str(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeExecutionScopeDocument {
    #[serde(rename = "_id")]
    id: String,
    owner_user_id: String,
    project_id: String,
    run_id: String,
    provider: String,
    generation: i64,
    status: String,
    #[serde(default)]
    session_refs: HashMap<String, i64>,
    #[serde(default)]
    terminal_status: Option<String>,
    #[serde(default)]
    next_invocation_sequence: i64,
    #[serde(default)]
    invocation_queue: Vec<RuntimeExecutionScopeInvocationRef>,
    #[serde(default)]
    running_invocation_id: Option<String>,
    updated_at: DateTime,
    expires_at: DateTime,
    expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeExecutionScopeStore {
    backend: Arc<RuntimeExecutionScopeStoreBackend>,
}

enum RuntimeExecutionScopeStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeExecutionScopeDocument>>),
    Mongo(Collection<RuntimeExecutionScopeDocument>),
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
            RuntimeExecutionScopeStoreBackend::Mongo(_) => Vec::new(),
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
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect MCP execution scope MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection = database
            .collection::<RuntimeExecutionScopeDocument>("mcp_management_runtime_execution_scopes");
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_execution_scope_expiry_ttl".to_string())
                            .expire_after(Some(Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("initialize execution scope TTL index failed: {error}"))?;
        Ok(Self {
            backend: Arc::new(RuntimeExecutionScopeStoreBackend::Mongo(collection)),
        })
    }

    pub async fn attach_session(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        session_id: &str,
        session_expires_at_unix: i64,
    ) -> Result<i64, RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        let now = chrono::Utc::now().timestamp();
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
                let scope =
                    scopes
                        .entry(id.clone())
                        .or_insert_with(|| RuntimeExecutionScopeDocument {
                            id,
                            owner_user_id: owner_user_id.to_string(),
                            project_id: project_id.to_string(),
                            run_id: run_id.to_string(),
                            provider: provider.as_str().to_string(),
                            generation: 1,
                            status: "active".to_string(),
                            session_refs: HashMap::new(),
                            terminal_status: None,
                            next_invocation_sequence: 0,
                            invocation_queue: Vec::new(),
                            running_invocation_id: None,
                            updated_at: DateTime::now(),
                            expires_at: DateTime::from_millis(
                                expires_at_unix.saturating_mul(1_000),
                            ),
                            expires_at_unix,
                        });
                scope
                    .session_refs
                    .retain(|_, expires_at_unix| *expires_at_unix > now);
                scope
                    .session_refs
                    .insert(session_id.to_string(), session_expires_at_unix);
                scope.updated_at = DateTime::now();
                scope.expires_at_unix = scope.expires_at_unix.max(expires_at_unix);
                scope.expires_at =
                    DateTime::from_millis(scope.expires_at_unix.saturating_mul(1_000));
                Ok(scope.generation)
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => {
                if collection
                    .find_one(doc! { "_id": id.as_str(), "status": "terminal" }, None)
                    .await
                    .map_err(store_error)?
                    .is_some()
                {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                let session_ref_path = format!("session_refs.{session_id}");
                let mut set_document = doc! { "updated_at": DateTime::now() };
                set_document.insert(session_ref_path, session_expires_at_unix);
                let result = collection
                    .find_one_and_update(
                        doc! { "_id": id.as_str(), "status": { "$ne": "terminal" } },
                        doc! {
                            "$setOnInsert": {
                                "owner_user_id": owner_user_id,
                                "project_id": project_id,
                                "run_id": run_id,
                                "provider": provider.as_str(),
                                "generation": 1_i64,
                                "status": "active",
                                "terminal_status": mongodb::bson::Bson::Null,
                                "next_invocation_sequence": 0_i64,
                                "invocation_queue": [],
                                "running_invocation_id": mongodb::bson::Bson::Null,
                            },
                            "$set": set_document,
                            "$max": {
                                "expires_at": DateTime::from_millis(expires_at_unix.saturating_mul(1_000)),
                                "expires_at_unix": expires_at_unix,
                            },
                        },
                        FindOneAndUpdateOptions::builder()
                            .upsert(true)
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await;
                match result {
                    Ok(Some(scope)) => Ok(scope.generation),
                    Ok(None) => Err(RuntimeExecutionScopeStoreError::Unavailable(
                        "attach Runtime Session did not return an execution scope".to_string(),
                    )),
                    Err(error) if is_duplicate_key(&error) => {
                        Err(RuntimeExecutionScopeStoreError::Terminal)
                    }
                    Err(error) => Err(store_error(error)),
                }
            }
        }
    }

    pub async fn detach_session(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        session_id: &str,
    ) -> Result<(), String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                if let Some(scope) = scopes.write().await.get_mut(id.as_str()) {
                    scope.session_refs.remove(session_id);
                    scope.updated_at = DateTime::now();
                }
                Ok(())
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => collection
                .update_one(
                    doc! { "_id": id },
                    {
                        let mut unset = mongodb::bson::Document::new();
                        unset.insert(format!("session_refs.{session_id}"), "");
                        doc! {
                            "$unset": unset,
                            "$set": { "updated_at": DateTime::now() },
                        }
                    },
                    None,
                )
                .await
                .map(|_| ())
                .map_err(|error| {
                    format!("detach Runtime Session from execution scope failed: {error}")
                }),
        }
    }

    pub async fn ensure_accepting_invocations(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
    ) -> Result<(), RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        let scope = match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                scopes.read().await.get(id.as_str()).cloned()
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => collection
                .find_one(doc! { "_id": id }, None)
                .await
                .map_err(store_error)?,
        };
        if scope.is_some_and(|scope| scope.status == "terminal") {
            Err(RuntimeExecutionScopeStoreError::Terminal)
        } else {
            Ok(())
        }
    }

    pub async fn enqueue_invocation(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<i64, RuntimeExecutionScopeStoreError> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let scope = scopes.get_mut(id.as_str()).ok_or_else(|| {
                    RuntimeExecutionScopeStoreError::Unavailable(
                        "execution scope is missing while enqueueing an invocation".to_string(),
                    )
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
                scope.updated_at = DateTime::now();
                Ok(sequence)
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => {
                let invocation = invocation_id.to_string();
                let updated = collection
                    .find_one_and_update(
                        doc! {
                            "_id": id,
                            "status": "active",
                            "invocation_queue.invocation_id": { "$ne": invocation_id },
                            "running_invocation_id": { "$ne": invocation_id },
                        },
                        vec![doc! {
                            "$set": {
                                "next_invocation_sequence": {
                                    "$add": [{ "$ifNull": ["$next_invocation_sequence", 0_i64] }, 1_i64]
                                },
                                "invocation_queue": {
                                    "$concatArrays": [
                                        { "$ifNull": ["$invocation_queue", []] },
                                        [{
                                            "invocation_id": invocation,
                                            "sequence": {
                                                "$add": [{ "$ifNull": ["$next_invocation_sequence", 0_i64] }, 1_i64]
                                            },
                                            "batch_id": mongodb::bson::Bson::Null,
                                            "call_index": mongodb::bson::Bson::Null,
                                        }]
                                    ]
                                },
                                "updated_at": DateTime::now(),
                            }
                        }],
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(store_error)?;
                if let Some(scope) = updated {
                    return scope
                        .invocation_queue
                        .iter()
                        .find(|reference| reference.invocation_id == invocation_id)
                        .map(|reference| reference.sequence)
                        .ok_or_else(|| {
                            RuntimeExecutionScopeStoreError::Unavailable(
                                "enqueued invocation is missing from execution scope queue"
                                    .to_string(),
                            )
                        });
                }
                let scope = collection
                    .find_one(
                        doc! { "_id": scope_id(owner_user_id, project_id, run_id, provider) },
                        None,
                    )
                    .await
                    .map_err(store_error)?;
                match scope {
                    Some(scope) if scope.status == "terminal" => {
                        Err(RuntimeExecutionScopeStoreError::Terminal)
                    }
                    Some(scope) => scope
                        .invocation_queue
                        .iter()
                        .find(|reference| reference.invocation_id == invocation_id)
                        .map(|reference| reference.sequence)
                        .ok_or_else(|| {
                            RuntimeExecutionScopeStoreError::Unavailable(
                                "execution scope rejected invocation queue insertion".to_string(),
                            )
                        }),
                    None => Err(RuntimeExecutionScopeStoreError::Unavailable(
                        "execution scope is missing while enqueueing an invocation".to_string(),
                    )),
                }
            }
        }
    }

    pub async fn enqueue_invocation_batch(
        &self,
        owner_user_id: &str,
        project_id: &str,
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
                    RuntimeExecutionScopeStoreError::Unavailable(
                        "execution scope is missing while enqueueing an invocation batch"
                            .to_string(),
                    )
                })?;
                if scope.status == "terminal" {
                    return Err(RuntimeExecutionScopeStoreError::Terminal);
                }
                if invocations.iter().any(|(invocation_id, _)| {
                    scope.running_invocation_id.as_deref() == Some(invocation_id.as_str())
                        || scope
                            .invocation_queue
                            .iter()
                            .any(|reference| reference.invocation_id == *invocation_id)
                }) {
                    return Err(RuntimeExecutionScopeStoreError::Unavailable(
                        "execution scope invocation batch contains an active duplicate".to_string(),
                    ));
                }
                let mut sequences = Vec::with_capacity(invocations.len());
                for (invocation_id, call_index) in invocations {
                    scope.next_invocation_sequence =
                        scope.next_invocation_sequence.saturating_add(1);
                    let sequence = scope.next_invocation_sequence;
                    sequences.push(sequence);
                    scope
                        .invocation_queue
                        .push(RuntimeExecutionScopeInvocationRef {
                            invocation_id: invocation_id.clone(),
                            sequence,
                            batch_id: Some(batch_id.to_string()),
                            call_index: Some(*call_index),
                        });
                }
                scope.updated_at = DateTime::now();
                Ok(sequences)
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => {
                let invocation_ids = invocations
                    .iter()
                    .map(|(invocation_id, _)| invocation_id.clone())
                    .collect::<Vec<_>>();
                let count = i64::try_from(invocations.len()).map_err(|_| {
                    RuntimeExecutionScopeStoreError::Unavailable(
                        "execution scope invocation batch is too large".to_string(),
                    )
                })?;
                let appended = invocations
                    .iter()
                    .enumerate()
                    .map(|(sequence_offset, (invocation_id, call_index))| {
                        mongodb::bson::Bson::Document(doc! {
                            "invocation_id": invocation_id,
                            "sequence": {
                                "$add": [
                                    { "$ifNull": ["$next_invocation_sequence", 0_i64] },
                                    i64::try_from(sequence_offset).unwrap_or(i64::MAX).saturating_add(1),
                                ]
                            },
                            "batch_id": batch_id,
                            "call_index": i64::try_from(*call_index).unwrap_or(i64::MAX),
                        })
                    })
                    .collect::<Vec<_>>();
                let updated = collection
                    .find_one_and_update(
                        doc! {
                            "_id": id.as_str(),
                            "status": "active",
                            "invocation_queue.invocation_id": { "$nin": invocation_ids.as_slice() },
                            "running_invocation_id": { "$nin": invocation_ids.as_slice() },
                        },
                        vec![doc! {
                            "$set": {
                                "next_invocation_sequence": {
                                    "$add": [
                                        { "$ifNull": ["$next_invocation_sequence", 0_i64] },
                                        count,
                                    ]
                                },
                                "invocation_queue": {
                                    "$concatArrays": [
                                        { "$ifNull": ["$invocation_queue", []] },
                                        appended,
                                    ]
                                },
                                "updated_at": DateTime::now(),
                            }
                        }],
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(store_error)?;
                let Some(scope) = updated else {
                    let scope = collection
                        .find_one(doc! { "_id": id }, None)
                        .await
                        .map_err(store_error)?;
                    return if scope.is_some_and(|scope| scope.status == "terminal") {
                        Err(RuntimeExecutionScopeStoreError::Terminal)
                    } else {
                        Err(RuntimeExecutionScopeStoreError::Unavailable(
                            "execution scope rejected invocation batch insertion".to_string(),
                        ))
                    };
                };
                invocation_ids
                    .iter()
                    .map(|invocation_id| {
                        scope
                            .invocation_queue
                            .iter()
                            .find(|reference| reference.invocation_id == *invocation_id)
                            .map(|reference| reference.sequence)
                            .ok_or_else(|| {
                                RuntimeExecutionScopeStoreError::Unavailable(
                                    "enqueued invocation is missing from execution scope batch"
                                        .to_string(),
                                )
                            })
                    })
                    .collect()
            }
        }
    }

    pub async fn try_acquire_invocation_turn(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<RuntimeExecutionTurnState, String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let Some(scope) = scopes.get_mut(id.as_str()) else {
                    return Err("execution scope is missing while acquiring a turn".to_string());
                };
                if scope.status == "terminal" {
                    return Ok(RuntimeExecutionTurnState::Terminal);
                }
                if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                    return Ok(RuntimeExecutionTurnState::Acquired);
                }
                if scope.running_invocation_id.is_some()
                    || scope
                        .invocation_queue
                        .first()
                        .is_none_or(|reference| reference.invocation_id != invocation_id)
                {
                    return Ok(RuntimeExecutionTurnState::Waiting);
                }
                scope.invocation_queue.remove(0);
                scope.running_invocation_id = Some(invocation_id.to_string());
                scope.updated_at = DateTime::now();
                Ok(RuntimeExecutionTurnState::Acquired)
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => {
                let updated = collection
                    .find_one_and_update(
                        doc! {
                            "_id": id.as_str(),
                            "status": "active",
                            "$or": [
                                { "running_invocation_id": mongodb::bson::Bson::Null },
                                { "running_invocation_id": { "$exists": false } },
                            ],
                            "invocation_queue.0.invocation_id": invocation_id,
                        },
                        doc! {
                            "$set": {
                                "running_invocation_id": invocation_id,
                                "updated_at": DateTime::now(),
                            },
                            "$pop": { "invocation_queue": -1_i32 },
                        },
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(|error| {
                        format!("acquire execution scope invocation turn failed: {error}")
                    })?;
                if updated.is_some() {
                    return Ok(RuntimeExecutionTurnState::Acquired);
                }
                let scope = collection
                    .find_one(doc! { "_id": id }, None)
                    .await
                    .map_err(|error| {
                        format!("load execution scope invocation turn failed: {error}")
                    })?
                    .ok_or_else(|| {
                        "execution scope is missing while acquiring a turn".to_string()
                    })?;
                if scope.status == "terminal" {
                    Ok(RuntimeExecutionTurnState::Terminal)
                } else if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                    Ok(RuntimeExecutionTurnState::Acquired)
                } else {
                    Ok(RuntimeExecutionTurnState::Waiting)
                }
            }
        }
    }

    pub async fn release_invocation_turn(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<(), String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                if let Some(scope) = scopes.write().await.get_mut(id.as_str()) {
                    if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                        scope.running_invocation_id = None;
                    }
                    scope
                        .invocation_queue
                        .retain(|reference| reference.invocation_id != invocation_id);
                    scope.updated_at = DateTime::now();
                }
                Ok(())
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => collection
                .update_one(
                    doc! { "_id": id },
                    vec![doc! {
                        "$set": {
                            "running_invocation_id": {
                                "$cond": [
                                    { "$eq": ["$running_invocation_id", invocation_id] },
                                    mongodb::bson::Bson::Null,
                                    "$running_invocation_id",
                                ]
                            },
                            "invocation_queue": {
                                "$filter": {
                                    "input": { "$ifNull": ["$invocation_queue", []] },
                                    "as": "queued",
                                    "cond": { "$ne": ["$$queued.invocation_id", invocation_id] },
                                }
                            },
                            "updated_at": DateTime::now(),
                        }
                    }],
                    None,
                )
                .await
                .map(|_| ())
                .map_err(|error| {
                    format!("release execution scope invocation turn failed: {error}")
                }),
        }
    }

    pub async fn finalize_run(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        provider: WorkspaceProviderKind,
        status: RuntimeRunTerminalStatus,
    ) -> Result<i64, String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        let now = chrono::Utc::now().timestamp();
        let expires_at_unix = now.saturating_add(TERMINAL_TOMBSTONE_TTL_SECONDS);
        let terminal_status = serde_json::to_value(status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "failed".to_string());
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let scope = scopes.entry(id.clone()).or_insert_with(|| {
                    RuntimeExecutionScopeDocument {
                        id,
                        owner_user_id: owner_user_id.to_string(),
                        project_id: project_id.to_string(),
                        run_id: run_id.to_string(),
                        provider: provider.as_str().to_string(),
                        generation: 1,
                        status: "terminal".to_string(),
                        session_refs: HashMap::new(),
                        terminal_status: Some(terminal_status.clone()),
                        next_invocation_sequence: 0,
                        invocation_queue: Vec::new(),
                        running_invocation_id: None,
                        updated_at: DateTime::now(),
                        expires_at: DateTime::from_millis(expires_at_unix.saturating_mul(1_000)),
                        expires_at_unix,
                    }
                });
                scope.status = "terminal".to_string();
                scope.terminal_status = Some(terminal_status);
                scope.invocation_queue.clear();
                scope.running_invocation_id = None;
                scope.updated_at = DateTime::now();
                scope.expires_at_unix = expires_at_unix;
                scope.expires_at = DateTime::from_millis(expires_at_unix.saturating_mul(1_000));
                Ok(scope.generation)
            }
            RuntimeExecutionScopeStoreBackend::Mongo(collection) => collection
                .find_one_and_update(
                    doc! { "_id": id.as_str() },
                    doc! {
                        "$setOnInsert": {
                            "owner_user_id": owner_user_id,
                            "project_id": project_id,
                            "run_id": run_id,
                            "provider": provider.as_str(),
                            "generation": 1_i64,
                            "session_refs": {},
                            "next_invocation_sequence": 0_i64,
                            "invocation_queue": [],
                            "running_invocation_id": mongodb::bson::Bson::Null,
                        },
                        "$set": {
                            "status": "terminal",
                            "terminal_status": terminal_status,
                            "invocation_queue": [],
                            "running_invocation_id": mongodb::bson::Bson::Null,
                            "updated_at": DateTime::now(),
                            "expires_at": DateTime::from_millis(expires_at_unix.saturating_mul(1_000)),
                            "expires_at_unix": expires_at_unix,
                        },
                    },
                    FindOneAndUpdateOptions::builder()
                        .upsert(true)
                        .return_document(ReturnDocument::After)
                        .build(),
                )
                .await
                .map_err(|error| format!("persist terminal execution scope failed: {error}"))?
                .map(|scope| scope.generation)
                .ok_or_else(|| {
                    "persist terminal execution scope did not return the scope generation"
                        .to_string()
                }),
        }
    }
}

fn scope_id(
    owner_user_id: &str,
    project_id: &str,
    run_id: &str,
    provider: WorkspaceProviderKind,
) -> String {
    let identity = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        owner_user_id.trim(),
        project_id.trim(),
        run_id.trim(),
        provider.as_str()
    );
    format!(
        "execution_scope_{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    )
}

fn store_error(error: mongodb::error::Error) -> RuntimeExecutionScopeStoreError {
    RuntimeExecutionScopeStoreError::Unavailable(format!(
        "Runtime Execution Scope store failed: {error}"
    ))
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

    #[tokio::test]
    async fn terminal_tombstone_rejects_new_sessions_and_invocations() {
        let store = RuntimeExecutionScopeStore::memory();
        store
            .attach_session(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "session-1",
                chrono::Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
        store
            .finalize_run(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                RuntimeRunTerminalStatus::Succeeded,
            )
            .await
            .unwrap();
        assert_eq!(
            store
                .attach_session(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                    "session-2",
                    chrono::Utc::now().timestamp() + 300,
                )
                .await,
            Err(RuntimeExecutionScopeStoreError::Terminal)
        );
        assert_eq!(
            store
                .ensure_accepting_invocations(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                )
                .await,
            Err(RuntimeExecutionScopeStoreError::Terminal)
        );
    }

    #[tokio::test]
    async fn session_renewal_updates_one_reference_and_preserves_generation() {
        let store = RuntimeExecutionScopeStore::memory();
        let renewed_expiry = chrono::Utc::now().timestamp() + 300;
        let first_generation = store
            .attach_session(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "session-1",
                chrono::Utc::now().timestamp() + 60,
            )
            .await
            .unwrap();
        let renewed_generation = store
            .attach_session(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "session-1",
                renewed_expiry,
            )
            .await
            .unwrap();
        assert_eq!(first_generation, 1);
        assert_eq!(renewed_generation, first_generation);

        let RuntimeExecutionScopeStoreBackend::Memory(scopes) = store.backend.as_ref() else {
            panic!("expected memory scope store");
        };
        let scopes = scopes.read().await;
        let scope = scopes.values().next().unwrap();
        assert_eq!(scope.session_refs.len(), 1);
        assert_eq!(
            scope.session_refs.get("session-1").copied(),
            Some(renewed_expiry)
        );
    }

    #[tokio::test]
    async fn finalization_is_idempotent_for_one_generation() {
        let store = RuntimeExecutionScopeStore::memory();
        let first = store
            .finalize_run(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                RuntimeRunTerminalStatus::Succeeded,
            )
            .await
            .unwrap();
        let repeated = store
            .finalize_run(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                RuntimeRunTerminalStatus::Succeeded,
            )
            .await
            .unwrap();
        assert_eq!(first, repeated);
    }

    #[tokio::test]
    async fn one_run_executes_invocations_in_fifo_order() {
        let store = RuntimeExecutionScopeStore::memory();
        store
            .attach_session(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "session-1",
                chrono::Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
        store
            .enqueue_invocation_batch(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "batch-1",
                &[
                    ("invocation-1".to_string(), 0),
                    ("invocation-2".to_string(), 1),
                ],
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .try_acquire_invocation_turn(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                    "invocation-2",
                )
                .await
                .unwrap(),
            RuntimeExecutionTurnState::Waiting
        );
        assert_eq!(
            store
                .try_acquire_invocation_turn(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                    "invocation-1",
                )
                .await
                .unwrap(),
            RuntimeExecutionTurnState::Acquired
        );
        assert_eq!(
            store
                .try_acquire_invocation_turn(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                    "invocation-2",
                )
                .await
                .unwrap(),
            RuntimeExecutionTurnState::Waiting
        );
        store
            .release_invocation_turn(
                "user-1",
                "project-1",
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            store
                .try_acquire_invocation_turn(
                    "user-1",
                    "project-1",
                    "run-1",
                    WorkspaceProviderKind::LocalConnector,
                    "invocation-2",
                )
                .await
                .unwrap(),
            RuntimeExecutionTurnState::Acquired
        );
    }

    #[tokio::test]
    async fn different_runs_acquire_turns_independently() {
        let store = RuntimeExecutionScopeStore::memory();
        for run_id in ["run-1", "run-2"] {
            store
                .attach_session(
                    "user-1",
                    "project-1",
                    run_id,
                    WorkspaceProviderKind::LocalConnector,
                    format!("session-{run_id}").as_str(),
                    chrono::Utc::now().timestamp() + 300,
                )
                .await
                .unwrap();
            store
                .enqueue_invocation(
                    "user-1",
                    "project-1",
                    run_id,
                    WorkspaceProviderKind::LocalConnector,
                    format!("invocation-{run_id}").as_str(),
                )
                .await
                .unwrap();
        }
        for run_id in ["run-1", "run-2"] {
            assert_eq!(
                store
                    .try_acquire_invocation_turn(
                        "user-1",
                        "project-1",
                        run_id,
                        WorkspaceProviderKind::LocalConnector,
                        format!("invocation-{run_id}").as_str(),
                    )
                    .await
                    .unwrap(),
                RuntimeExecutionTurnState::Acquired
            );
        }
    }
}
