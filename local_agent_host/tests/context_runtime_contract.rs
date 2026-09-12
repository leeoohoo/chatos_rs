// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ProviderContextStateRecord, PutRecord, RecordMetadata, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentContextRuntime, ProviderContextEncryptionKey, StandardLocalAgentContextRuntime,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, LocalAgentRun, LocalAgentRunStatus,
    MemorySyncStatus, MessageMode, ModelProtocol, ModelRuntimeDescriptor, ProviderContextItem,
};
use chatos_local_agent_runtime::{
    record_semantic_message, MemoryEngineContextAdapter, MemoryEngineContextApi, MemorySyncApi,
    MemorySyncApiReceipt, MemorySyncApiRequest, MemorySyncPolicy, MemorySynchronizer,
    ModelStepContext, ProviderNativeContextCommit, RecordSemanticMessageRequest,
};
use chrono::Utc;
use memory_engine_sdk::{
    ComposeContextResponse, RunThreadActiveSummaryResponse, SdkComposeContextRequest,
};
use tokio_util::sync::CancellationToken;

struct UnusedMemoryEngine;

#[async_trait]
impl MemoryEngineContextApi for UnusedMemoryEngine {
    async fn compose_context(
        &self,
        _request: &SdkComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        unreachable!("provider-native test must not call Memory Engine")
    }

    async fn run_active_summary(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        unreachable!("provider-native test must not call Memory Engine")
    }

    async fn get_active_summary_status(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        unreachable!("provider-native test must not call Memory Engine")
    }
}

#[async_trait]
impl MemorySyncApi for UnusedMemoryEngine {
    async fn batch_sync(
        &self,
        _request: MemorySyncApiRequest,
        _cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String> {
        unreachable!("empty context test has no Memory Engine outbox")
    }
}

struct RecordingMemoryEngine {
    requests: Mutex<Vec<MemorySyncApiRequest>>,
}

#[async_trait]
impl MemoryEngineContextApi for RecordingMemoryEngine {
    async fn compose_context(
        &self,
        _request: &SdkComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        unreachable!("context composition belongs to the model executor")
    }

    async fn run_active_summary(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        unreachable!("context composition belongs to the model executor")
    }

    async fn get_active_summary_status(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        unreachable!("context composition belongs to the model executor")
    }
}

#[async_trait]
impl MemorySyncApi for RecordingMemoryEngine {
    async fn batch_sync(
        &self,
        request: MemorySyncApiRequest,
        _cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String> {
        let receipt = MemorySyncApiReceipt {
            thread_id: request.thread_id.clone(),
            received_count: request.records.len(),
            upserted_count: request.records.len(),
        };
        self.requests.lock().unwrap().push(request);
        Ok(receipt)
    }
}

struct SeedProviderContext {
    records: Vec<ProviderContextStateRecord>,
}

#[async_trait]
impl StorageTransaction for SeedProviderContext {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        for record in self.records.drain(..) {
            repositories
                .provider_context()
                .put(PutRecord {
                    record,
                    expected_revision: None,
                })
                .await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn standard_runtime_authenticates_and_restores_provider_native_items() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:context-storage-key").unwrap(),
        },
        &StorageEncryptionKey::new([31; 32]),
    )
    .await
    .unwrap();
    let runtime = runtime([41; 32]);
    let run = run(ContextStrategy::ProviderNative, now);
    let raw_items = vec![
        serde_json::json!({"type": "message", "id": "message-1", "content": []}),
        serde_json::json!({
            "type": "compaction",
            "id": "compaction-1",
            "encrypted_content": "provider-opaque"
        }),
    ];
    let sealed = runtime
        .seal_provider_context_commit(
            &run,
            ProviderNativeContextCommit {
                generation: 1,
                retained_items: raw_items.clone(),
                dropped_item_count: 0,
                newest_compaction_id: Some("compaction-1".to_string()),
            },
            now,
        )
        .await
        .unwrap();
    assert!(sealed
        .retained_items
        .iter()
        .all(|item| !item.encrypted_payload.contains("provider-opaque")));
    let records = sealed
        .retained_items
        .into_iter()
        .enumerate()
        .map(|(index, item)| ProviderContextStateRecord {
            metadata: RecordMetadata {
                id: format!("provider-item-{index}"),
                scope: scope(),
                origin_device_id: "device-1".to_string(),
                revision: 0,
                created_at: now,
                updated_at: now,
            },
            item: ProviderContextItem {
                item_id: format!("provider-item-{index}"),
                run_id: run.run_id.clone(),
                generation: sealed.generation,
                sequence: item.sequence,
                provider: run.model_runtime_snapshot.provider.clone(),
                item_type: item.item_type,
                encrypted_payload: item.encrypted_payload,
                payload_digest: item.payload_digest,
                created_at: item.created_at,
            },
        })
        .collect();
    storage
        .transaction(&mut SeedProviderContext { records })
        .await
        .unwrap();

    let context = runtime
        .prepare_model_step_context(&storage, &scope(), &run, &CancellationToken::new())
        .await
        .unwrap();
    let ModelStepContext::ProviderNative(window) = context else {
        panic!("frozen strategy was not respected");
    };
    assert_eq!(window.generation(), 1);
    assert_eq!(window.items(), raw_items);
}

#[tokio::test]
async fn standard_runtime_builds_memory_scope_from_frozen_run_identity() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:memory-context-key").unwrap(),
        },
        &StorageEncryptionKey::new([51; 32]),
    )
    .await
    .unwrap();
    let runtime = runtime([61; 32]);
    let run = run(ContextStrategy::MemoryEngine, now);
    let context = runtime
        .prepare_model_step_context(&storage, &scope(), &run, &CancellationToken::new())
        .await
        .unwrap();
    let ModelStepContext::MemoryEngine { scope, .. } = context else {
        panic!("frozen strategy was not respected");
    };
    assert_eq!(scope.tenant_id, "tenant-1");
    assert_eq!(scope.source_id, "local-agent");
    assert_eq!(scope.thread_id, "thread-1");
    assert_eq!(scope.subject_id.as_deref(), Some("user-1"));
}

#[tokio::test]
async fn memory_strategy_flushes_local_semantic_messages_before_returning_context() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:memory-sync-key").unwrap(),
        },
        &StorageEncryptionKey::new([71; 32]),
    )
    .await
    .unwrap();
    record_semantic_message(
        &storage,
        RecordSemanticMessageRequest {
            scope: scope(),
            message: AgentMessage {
                record_id: "message-1".to_string(),
                run_id: "run-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                sequence: 1,
                role: AgentMessageRole::User,
                content: Some("Design the first section.".to_string()),
                reasoning: None,
                structured_payload: None,
                tool_call_id: None,
                response_id: None,
                message_mode: MessageMode::Semantic,
                message_source: "main_chat_user".to_string(),
                memory_sync_status: MemorySyncStatus::Pending,
                created_at: now,
            },
            origin_device_id: "device-1".to_string(),
            now,
        },
    )
    .await
    .unwrap();
    let memory = Arc::new(RecordingMemoryEngine {
        requests: Mutex::new(Vec::new()),
    });
    let runtime = StandardLocalAgentContextRuntime::new(
        &ProviderContextEncryptionKey::new([81; 32]),
        MemoryEngineContextAdapter::new(memory.clone(), "local-agent").unwrap(),
        MemorySynchronizer::new(
            memory.clone(),
            "tenant-1",
            "local-agent",
            MemorySyncPolicy::default(),
        )
        .unwrap(),
        "tenant-1",
    )
    .unwrap();

    runtime
        .prepare_model_step_context(
            &storage,
            &scope(),
            &run(ContextStrategy::MemoryEngine, now),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = memory.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].thread_id, "thread-1");
    assert_eq!(requests[0].records.len(), 1);
    assert_eq!(requests[0].records[0].id, "message-1");
}

fn runtime(key: [u8; 32]) -> StandardLocalAgentContextRuntime {
    let memory = Arc::new(UnusedMemoryEngine);
    StandardLocalAgentContextRuntime::new(
        &ProviderContextEncryptionKey::new(key),
        MemoryEngineContextAdapter::new(memory.clone(), "local-agent").unwrap(),
        MemorySynchronizer::new(
            memory,
            "tenant-1",
            "local-agent",
            MemorySyncPolicy::default(),
        )
        .unwrap(),
        "tenant-1",
    )
    .unwrap()
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn run(strategy: ContextStrategy, now: chrono::DateTime<Utc>) -> LocalAgentRun {
    let provider_native = strategy == ContextStrategy::ProviderNative;
    let descriptor = ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: if provider_native {
            "openai"
        } else {
            "deepseek"
        }
        .to_string(),
        model: "model-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: strategy,
        supports_streaming: true,
        supports_native_compaction: provider_native,
        supports_input_token_count: true,
    };
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "thread-1".to_string(),
        project_id: Some("project-1".to_string()),
        status: LocalAgentRunStatus::ModelRunning,
        version: 1,
        step_seq: 1,
        iteration: 1,
        retry_count: 0,
        model_config_id: descriptor.model_config_id.clone(),
        model_config_revision: descriptor.revision,
        model_runtime_snapshot: descriptor,
        context_strategy: strategy,
        prompt_revision: "prompt-1".to_string(),
        capability_snapshot_ref: "capabilities-1".to_string(),
        pending_batch_id: None,
        pending_interaction: None,
        terminal_outcome: None,
        deadline_at: None,
        created_at: now,
        updated_at: now,
    }
}
