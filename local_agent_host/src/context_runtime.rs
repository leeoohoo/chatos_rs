// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chatos_client_storage::{
    ClientStorage, ListQuery, ProviderContextStateRecord, RecordScope, StorageError,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{ContextStrategy, LocalAgentRun, SyncOutboxStatus};
use chatos_local_agent_runtime::{
    DurableProviderContextCommit, DurableProviderContextItem, MemoryEngineContextAdapter,
    MemoryEngineContextScope, MemorySynchronizer, ModelStepContext, ProviderNativeContextCommit,
    ProviderNativeContextWindow,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const PROVIDER_CONTEXT_PREFIX: &str = "chatos-provider-context-v1:";
const PROVIDER_CONTEXT_AAD: &[u8] = b"chatos-local-agent-provider-context-v1";
const NONCE_LENGTH: usize = 12;
const MAX_BLOCKING_MEMORY_SYNC_BATCHES: usize = 1_000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentContextRuntimeError {
    #[error("local Agent context preparation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("local Agent context identity is invalid: {0}")]
    InvalidIdentity(&'static str),
    #[error("provider context does not match the frozen Run: {0}")]
    ProviderContextMismatch(&'static str),
    #[error("provider context payload failed authentication")]
    ProviderContextAuthentication,
    #[error("provider context payload is malformed")]
    MalformedProviderContext,
    #[error("local Agent context runtime failed: {0}")]
    Runtime(String),
}

pub struct ProviderContextEncryptionKey(Zeroizing<[u8; 32]>);

impl ProviderContextEncryptionKey {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
}

impl fmt::Debug for ProviderContextEncryptionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderContextEncryptionKey([REDACTED])")
    }
}

#[derive(Clone)]
pub struct AuthenticatedProviderContextCipher {
    cipher: Aes256Gcm,
}

impl AuthenticatedProviderContextCipher {
    pub fn new(key: &ProviderContextEncryptionKey) -> Self {
        Self {
            cipher: Aes256Gcm::new((&*key.0).into()),
        }
    }

    fn seal(&self, plaintext: &[u8]) -> Result<String, LocalAgentContextRuntimeError> {
        let mut nonce_bytes = [0_u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: PROVIDER_CONTEXT_AAD,
                },
            )
            .map_err(|_| LocalAgentContextRuntimeError::ProviderContextAuthentication)?;
        let mut envelope = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(format!(
            "{PROVIDER_CONTEXT_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(envelope)
        ))
    }

    fn open(&self, envelope: &str) -> Result<Vec<u8>, LocalAgentContextRuntimeError> {
        let encoded = envelope
            .strip_prefix(PROVIDER_CONTEXT_PREFIX)
            .ok_or(LocalAgentContextRuntimeError::MalformedProviderContext)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?;
        if bytes.len() <= NONCE_LENGTH {
            return Err(LocalAgentContextRuntimeError::MalformedProviderContext);
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LENGTH);
        let nonce = <[u8; NONCE_LENGTH]>::try_from(nonce)
            .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?;
        self.cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: PROVIDER_CONTEXT_AAD,
                },
            )
            .map_err(|_| LocalAgentContextRuntimeError::ProviderContextAuthentication)
    }
}

/// The concrete context runtime shared by Main Chat and Task Runner.
/// Strategy selection is taken only from the frozen Run descriptor.
#[derive(Clone)]
pub struct StandardLocalAgentContextRuntime {
    provider_cipher: AuthenticatedProviderContextCipher,
    memory_engine: MemoryEngineContextAdapter,
    memory_synchronizer: MemorySynchronizer,
    memory_tenant_id: String,
}

impl StandardLocalAgentContextRuntime {
    pub fn new(
        provider_key: &ProviderContextEncryptionKey,
        memory_engine: MemoryEngineContextAdapter,
        memory_synchronizer: MemorySynchronizer,
        memory_tenant_id: impl Into<String>,
    ) -> Result<Self, LocalAgentContextRuntimeError> {
        let memory_tenant_id = memory_tenant_id.into();
        if memory_tenant_id.trim().is_empty() || memory_tenant_id.trim() != memory_tenant_id {
            return Err(LocalAgentContextRuntimeError::InvalidIdentity(
                "memory_tenant_id",
            ));
        }
        if memory_synchronizer.tenant_id() != memory_tenant_id
            || memory_synchronizer.source_id() != memory_engine.source_id()
        {
            return Err(LocalAgentContextRuntimeError::InvalidIdentity(
                "memory synchronizer scope",
            ));
        }
        Ok(Self {
            provider_cipher: AuthenticatedProviderContextCipher::new(provider_key),
            memory_engine,
            memory_synchronizer,
            memory_tenant_id,
        })
    }

    async fn load_provider_context(
        &self,
        storage: &dyn ClientStorage,
        scope: &RecordScope,
        run: &LocalAgentRun,
    ) -> Result<ProviderNativeContextWindow, LocalAgentContextRuntimeError> {
        let mut operation = LoadProviderContextOperation {
            scope: scope.clone(),
            run_id: run.run_id.clone(),
            records: Vec::new(),
        };
        storage.transaction(&mut operation).await?;
        let mut records = operation.records;
        if records.is_empty() {
            return ProviderNativeContextWindow::empty(1)
                .map_err(|error| LocalAgentContextRuntimeError::Runtime(error.to_string()));
        }
        records.sort_by_key(|record| (record.item.generation, record.item.sequence));
        let generation = records[0].item.generation;
        let mut items = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            let expected_sequence = u64::try_from(index)
                .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?
                + 1;
            if record.item.generation != generation {
                return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                    "multiple active generations",
                ));
            }
            if record.item.sequence != expected_sequence {
                return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                    "provider item sequence is not contiguous",
                ));
            }
            if record.item.provider != run.model_runtime_snapshot.provider {
                return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                    "provider identity changed",
                ));
            }
            let plaintext = self.provider_cipher.open(&record.item.encrypted_payload)?;
            if payload_digest(plaintext.as_slice()) != record.item.payload_digest {
                return Err(LocalAgentContextRuntimeError::ProviderContextAuthentication);
            }
            let item: serde_json::Value = serde_json::from_slice(plaintext.as_slice())
                .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?;
            if item.get("type").and_then(serde_json::Value::as_str)
                != Some(record.item.item_type.as_str())
            {
                return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                    "provider item type changed",
                ));
            }
            items.push(item);
        }
        ProviderNativeContextWindow::new(generation, items)
            .map_err(|error| LocalAgentContextRuntimeError::Runtime(error.to_string()))
    }
}

/// Owns the strategy-specific context boundary used by the local Agent Host.
///
/// Implementations load and decrypt provider-native items or construct the
/// authoritative Memory Engine adapter and scope. Provider-native output is
/// sealed here before the Host gives it to durable storage. Profiles and UI
/// clients never load, merge, encrypt, or persist context themselves.
#[async_trait]
pub trait LocalAgentContextRuntime: Send + Sync {
    async fn prepare_model_step_context(
        &self,
        storage: &dyn ClientStorage,
        scope: &RecordScope,
        run: &LocalAgentRun,
        cancellation: &CancellationToken,
    ) -> Result<ModelStepContext, LocalAgentContextRuntimeError>;

    async fn seal_provider_context_commit(
        &self,
        run: &LocalAgentRun,
        commit: ProviderNativeContextCommit,
        now: DateTime<Utc>,
    ) -> Result<DurableProviderContextCommit, LocalAgentContextRuntimeError>;
}

#[async_trait]
impl LocalAgentContextRuntime for StandardLocalAgentContextRuntime {
    async fn prepare_model_step_context(
        &self,
        storage: &dyn ClientStorage,
        scope: &RecordScope,
        run: &LocalAgentRun,
        cancellation: &CancellationToken,
    ) -> Result<ModelStepContext, LocalAgentContextRuntimeError> {
        if cancellation.is_cancelled() {
            return Err(LocalAgentContextRuntimeError::Cancelled);
        }
        if scope.owner_user_id != run.owner_user_id {
            return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                "owner identity changed",
            ));
        }
        match run.context_strategy {
            ContextStrategy::ProviderNative => {
                let context = self.load_provider_context(storage, scope, run).await?;
                if cancellation.is_cancelled() {
                    return Err(LocalAgentContextRuntimeError::Cancelled);
                }
                Ok(ModelStepContext::ProviderNative(context))
            }
            ContextStrategy::MemoryEngine => {
                let mut drained = false;
                for _ in 0..MAX_BLOCKING_MEMORY_SYNC_BATCHES {
                    if cancellation.is_cancelled() {
                        return Err(LocalAgentContextRuntimeError::Cancelled);
                    }
                    let report = self
                        .memory_synchronizer
                        .sync_once(storage, scope.clone(), Utc::now(), cancellation.clone())
                        .await?;
                    if cancellation.is_cancelled() {
                        return Err(LocalAgentContextRuntimeError::Cancelled);
                    }
                    if report.deferred > 0
                        || report.permanently_failed > 0
                        || report.exhausted_before_send > 0
                        || !report.errors.is_empty()
                    {
                        return Err(LocalAgentContextRuntimeError::Runtime(format!(
                            "Memory Engine sync blocked model context: deferred={}, permanently_failed={}, exhausted={}, errors={}",
                            report.deferred,
                            report.permanently_failed,
                            report.exhausted_before_send,
                            report.errors.len()
                        )));
                    }
                    if report.claimed == 0 {
                        drained = true;
                        break;
                    }
                }
                if !drained {
                    return Err(LocalAgentContextRuntimeError::Runtime(
                        "Memory Engine sync did not drain within the bounded batch limit"
                            .to_string(),
                    ));
                }
                let mut inspection = InspectUnsyncedMemoryOperation {
                    scope: scope.clone(),
                    unsynced_count: 0,
                };
                storage.transaction(&mut inspection).await?;
                if inspection.unsynced_count > 0 {
                    return Err(LocalAgentContextRuntimeError::Runtime(format!(
                        "Memory Engine sync still has {} pending, in-flight, or failed records",
                        inspection.unsynced_count
                    )));
                }
                let memory_scope = MemoryEngineContextScope::thread(
                    self.memory_tenant_id.clone(),
                    self.memory_engine.source_id().to_string(),
                    run.owner_entity_id.clone(),
                )
                .and_then(|scope| scope.with_subject_id(run.owner_user_id.clone()))
                .map_err(|error| LocalAgentContextRuntimeError::Runtime(error.to_string()))?;
                Ok(ModelStepContext::MemoryEngine {
                    adapter: self.memory_engine.clone(),
                    scope: memory_scope,
                })
            }
        }
    }

    async fn seal_provider_context_commit(
        &self,
        run: &LocalAgentRun,
        commit: ProviderNativeContextCommit,
        now: DateTime<Utc>,
    ) -> Result<DurableProviderContextCommit, LocalAgentContextRuntimeError> {
        if run.context_strategy != ContextStrategy::ProviderNative {
            return Err(LocalAgentContextRuntimeError::ProviderContextMismatch(
                "Memory Engine Run attempted a provider-native commit",
            ));
        }
        let retained_items = commit
            .retained_items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let item_type = item
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or(LocalAgentContextRuntimeError::MalformedProviderContext)?
                    .to_string();
                let plaintext = serde_json::to_vec(&item)
                    .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?;
                Ok(DurableProviderContextItem {
                    sequence: u64::try_from(index)
                        .map_err(|_| LocalAgentContextRuntimeError::MalformedProviderContext)?
                        + 1,
                    item_type,
                    encrypted_payload: self.provider_cipher.seal(plaintext.as_slice())?,
                    payload_digest: payload_digest(plaintext.as_slice()),
                    created_at: now,
                })
            })
            .collect::<Result<Vec<_>, LocalAgentContextRuntimeError>>()?;
        Ok(DurableProviderContextCommit {
            generation: commit.generation,
            retained_items,
        })
    }
}

struct LoadProviderContextOperation {
    scope: RecordScope,
    run_id: String,
    records: Vec<ProviderContextStateRecord>,
}

struct InspectUnsyncedMemoryOperation {
    scope: RecordScope,
    unsynced_count: usize,
}

#[async_trait]
impl StorageTransaction for InspectUnsyncedMemoryOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> Result<(), StorageError> {
        let mut cursor = None;
        loop {
            let page = repositories
                .sync_outbox()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            self.unsynced_count += page
                .records
                .into_iter()
                .filter(|record| record.item.status != SyncOutboxStatus::Succeeded)
                .count();
            match page.next_cursor {
                Some(next) if Some(next.as_str()) != cursor.as_deref() => cursor = Some(next),
                Some(_) => {
                    return Err(StorageError::InvalidData {
                        reason: "Memory Sync pagination cursor did not advance".to_string(),
                    });
                }
                None => break,
            }
        }
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for LoadProviderContextOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> Result<(), StorageError> {
        let mut cursor = None;
        loop {
            let page = repositories
                .provider_context()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            self.records.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.item.run_id == self.run_id),
            );
            match page.next_cursor {
                Some(next) if Some(next.as_str()) != cursor.as_deref() => cursor = Some(next),
                Some(_) => {
                    return Err(StorageError::InvalidData {
                        reason: "provider context pagination cursor did not advance".to_string(),
                    });
                }
                None => break,
            }
        }
        Ok(())
    }
}

fn payload_digest(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}
