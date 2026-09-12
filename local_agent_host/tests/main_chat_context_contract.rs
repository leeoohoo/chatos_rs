// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_agent_profiles::{
    MainChatAgentProfile, MainChatCapabilitySnapshot, MainChatContextProvider,
    MainChatProjectSnapshot, MainChatPromptSnapshot,
};
use chatos_client_storage::{
    ClientStorage, ListQuery, PutRecord, RecordQuery, RecordScope, SecretReference,
    SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey, StorageResult,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAttachmentLocator, LocalAttachmentResolver, StoredMainChatContextProvider,
};
use chatos_local_agent_protocol::{
    ContextStrategy, FrozenSnapshot, LocalAttachmentReference, ModelProtocol,
    ModelRuntimeDescriptor,
};
use chatos_local_agent_runtime::{
    create_local_agent_run, CreateLocalAgentRunRequest, InitialRunMessage, LocalAgentProfile,
};
use chrono::Utc;
use sha2::{Digest, Sha256};

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn descriptor(strategy: ContextStrategy) -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-main-1".to_string(),
        revision: 7,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: strategy,
        supports_streaming: true,
        supports_native_compaction: strategy == ContextStrategy::ProviderNative,
        supports_input_token_count: true,
    }
}

fn snapshots() -> (FrozenSnapshot, FrozenSnapshot, FrozenSnapshot) {
    let prompt = FrozenSnapshot::new(
        "main-prompt-snapshot-1",
        "main-prompt-revision-1",
        serde_json::to_value(MainChatPromptSnapshot {
            prompt_revision: "main-prompt-revision-1".to_string(),
            base_system_prompt: "Work as a visual design collaborator.".to_string(),
            contact_system_prompt: Some("Preserve the user's design intent.".to_string()),
            skill_catalog_prompt: Some("Use the frozen visual design skill catalog.".to_string()),
        })
        .unwrap(),
    )
    .unwrap();
    let capability = FrozenSnapshot::new(
        "main-capabilities-1",
        "main-capabilities-revision-1",
        serde_json::to_value(MainChatCapabilitySnapshot {
            snapshot_ref: "main-capabilities-1".to_string(),
            allowed_tools: vec!["ask_user".to_string(), "create_local_task".to_string()],
        })
        .unwrap(),
    )
    .unwrap();
    let project = FrozenSnapshot::new(
        "main-project-snapshot-1",
        "project-revision-1",
        serde_json::to_value(MainChatProjectSnapshot {
            project_id: "project-1".to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            project_name: "Website redesign".to_string(),
            design_context: serde_json::json!({
                "design_system": "editorial",
                "target_surface": "marketing website"
            }),
        })
        .unwrap(),
    )
    .unwrap();
    (prompt, capability, project)
}

#[derive(Default)]
struct BytesResolver {
    bytes: Vec<u8>,
    received: Mutex<Vec<LocalAttachmentLocator>>,
}

#[async_trait]
impl LocalAttachmentResolver for BytesResolver {
    async fn resolve(&self, attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String> {
        self.received.lock().unwrap().push(attachment.clone());
        Ok(self.bytes.clone())
    }
}

async fn storage() -> (tempfile::TempDir, Arc<SqliteClientStorage>) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:main-chat-context-key").unwrap(),
            },
            &StorageEncryptionKey::new([71; 32]),
        )
        .await
        .unwrap(),
    );
    (directory, storage)
}

async fn create_run(
    storage: &dyn ClientStorage,
    strategy: ContextStrategy,
    bytes: &[u8],
    payload_reference: &str,
) -> chatos_local_agent_protocol::LocalAgentRun {
    let now = Utc::now();
    let (prompt_snapshot, capability_snapshot, project_snapshot) = snapshots();
    let attachment = LocalAttachmentReference {
        attachment_id: "visual-1".to_string(),
        media_type: "image/png".to_string(),
        payload_reference: payload_reference.to_string(),
        payload_digest: format!("sha256:{:x}", Sha256::digest(bytes)),
        byte_size: u64::try_from(bytes.len()).unwrap(),
    };
    let created = create_local_agent_run(
        storage,
        CreateLocalAgentRunRequest {
            scope: scope(),
            run_id: "main-run-1".to_string(),
            profile_key: "main_chat".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: "thread-1".to_string(),
            project_id: Some("project-1".to_string()),
            model_runtime_snapshot: descriptor(strategy),
            prompt_revision: prompt_snapshot.revision.clone(),
            capability_snapshot_ref: capability_snapshot.snapshot_id.clone(),
            origin_device_id: "device-1".to_string(),
            causation_id: "turn-1".to_string(),
            deadline_at: None,
            initial_message: Some(InitialRunMessage {
                record_id: "message-1".to_string(),
                turn_id: "turn-1".to_string(),
                content: Some("Refine this reference into a beautiful website.".to_string()),
                structured_payload: Some(serde_json::json!({
                    "type": "main_chat_turn",
                    "prompt_snapshot": prompt_snapshot,
                    "capability_snapshot": capability_snapshot,
                    "project_snapshot": project_snapshot,
                    "attachments": [{
                        "attachment_id": attachment.attachment_id,
                        "media_type": attachment.media_type,
                        "payload_digest": attachment.payload_digest,
                        "byte_size": attachment.byte_size,
                    }],
                })),
                message_source: "main_chat".to_string(),
            }),
            initial_attachments: vec![attachment],
            now,
        },
    )
    .await
    .unwrap();
    created.run_record.run
}

#[tokio::test]
async fn first_provider_step_rebuilds_text_and_verified_visual_without_local_references() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let run = create_run(
        storage.as_ref(),
        ContextStrategy::ProviderNative,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    let resolver = Arc::new(BytesResolver {
        bytes,
        ..BytesResolver::default()
    });
    let provider = Arc::new(StoredMainChatContextProvider::new(
        storage,
        scope(),
        resolver.clone(),
    ));
    let context = provider.load_step_context(&run).await.unwrap();
    assert_eq!(context.model_input_items.len(), 1);
    let serialized_input = context.model_input_items[0].to_string();
    assert!(serialized_input.contains("Refine this reference"));
    assert!(serialized_input.contains("data:image/png;base64,"));
    assert!(!serialized_input.contains("attachment-grant"));
    assert!(!serialized_input.contains("payload_reference"));
    assert_eq!(resolver.received.lock().unwrap().len(), 1);

    let profile = MainChatAgentProfile::new(provider);
    let step = profile.prepare_model_step(&run).await.unwrap();
    let instructions = step.instructions.unwrap();
    assert!(instructions.contains("Website redesign"));
    assert!(instructions.contains("editorial"));
    assert!(!instructions.contains("authority"));
    assert!(!instructions.contains("payload_reference"));
}

#[tokio::test]
async fn provider_continuation_does_not_repeat_the_initial_user_input() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let mut run = create_run(
        storage.as_ref(),
        ContextStrategy::ProviderNative,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    run.iteration = 1;
    let provider = StoredMainChatContextProvider::new(
        storage,
        scope(),
        Arc::new(BytesResolver {
            bytes,
            ..BytesResolver::default()
        }),
    );
    let context = provider.load_step_context(&run).await.unwrap();
    assert!(context.model_input_items.is_empty());
}

#[tokio::test]
async fn memory_engine_strategy_does_not_duplicate_user_text_in_the_overlay() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let run = create_run(
        storage.as_ref(),
        ContextStrategy::MemoryEngine,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    let provider = StoredMainChatContextProvider::new(
        storage,
        scope(),
        Arc::new(BytesResolver {
            bytes,
            ..BytesResolver::default()
        }),
    );
    let context = provider.load_step_context(&run).await.unwrap();
    let serialized = context.model_input_items[0].to_string();
    assert!(serialized.contains("input_image"));
    assert!(!serialized.contains("Refine this reference"));
}

struct TamperInitialPayload {
    field: &'static str,
    value: serde_json::Value,
}

struct TamperAttachmentReference;

#[async_trait]
impl StorageTransaction for TamperAttachmentReference {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut records = repositories
            .media()
            .list(&ListQuery {
                scope: scope(),
                cursor: None,
                limit: ListQuery::MAX_LIMIT,
            })
            .await?
            .records;
        let mut record = records.pop().unwrap();
        let revision = record.metadata.revision;
        record.state["payload_reference"] = serde_json::json!("/Users/alice/private/reference.png");
        repositories
            .media()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for TamperInitialPayload {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: scope(),
            id: "message-1".to_string(),
        };
        let mut record = repositories.agent_messages().get(&query).await?.unwrap();
        let revision = record.metadata.revision;
        record.message.structured_payload.as_mut().unwrap()[self.field] = self.value.clone();
        repositories
            .agent_messages()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn tampered_snapshot_and_expanded_capabilities_fail_closed() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let run = create_run(
        storage.as_ref(),
        ContextStrategy::ProviderNative,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    let provider = StoredMainChatContextProvider::new(
        storage.clone(),
        scope(),
        Arc::new(BytesResolver {
            bytes,
            ..BytesResolver::default()
        }),
    );
    let capability = FrozenSnapshot::new(
        "main-capabilities-1",
        "main-capabilities-revision-1",
        serde_json::to_value(MainChatCapabilitySnapshot {
            snapshot_ref: "main-capabilities-1".to_string(),
            allowed_tools: vec![
                "ask_user".to_string(),
                "create_local_task".to_string(),
                "write_file".to_string(),
            ],
        })
        .unwrap(),
    )
    .unwrap();
    storage
        .transaction(&mut TamperInitialPayload {
            field: "capability_snapshot",
            value: serde_json::to_value(capability).unwrap(),
        })
        .await
        .unwrap();
    let error = provider.load_step_context(&run).await.unwrap_err();
    assert!(error.contains("capabilities"));
}

#[tokio::test]
async fn project_snapshot_cannot_change_the_frozen_project_scope() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let run = create_run(
        storage.as_ref(),
        ContextStrategy::ProviderNative,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    let provider = StoredMainChatContextProvider::new(
        storage.clone(),
        scope(),
        Arc::new(BytesResolver {
            bytes,
            ..BytesResolver::default()
        }),
    );
    let project = FrozenSnapshot::new(
        "main-project-snapshot-2",
        "project-revision-2",
        serde_json::to_value(MainChatProjectSnapshot {
            project_id: "project-2".to_string(),
            snapshot_revision: "project-revision-2".to_string(),
            project_name: "Another project".to_string(),
            design_context: serde_json::json!({"surface": "website"}),
        })
        .unwrap(),
    )
    .unwrap();
    storage
        .transaction(&mut TamperInitialPayload {
            field: "project_snapshot",
            value: serde_json::to_value(project).unwrap(),
        })
        .await
        .unwrap();
    let error = provider.load_step_context(&run).await.unwrap_err();
    assert!(error.contains("project snapshot does not match"));
}

#[tokio::test]
async fn absolute_attachment_paths_are_rejected_before_resolution() {
    let (_directory, storage) = storage().await;
    let bytes = b"small-png-fixture".to_vec();
    let run = create_run(
        storage.as_ref(),
        ContextStrategy::ProviderNative,
        &bytes,
        "attachment-grant:visual-1",
    )
    .await;
    storage
        .transaction(&mut TamperAttachmentReference)
        .await
        .unwrap();
    let resolver = Arc::new(BytesResolver {
        bytes,
        ..BytesResolver::default()
    });
    let provider = StoredMainChatContextProvider::new(storage, scope(), resolver.clone());
    let error = provider.load_step_context(&run).await.unwrap_err();
    assert!(error.contains("opaque payload references"));
    assert!(resolver.received.lock().unwrap().is_empty());
}
