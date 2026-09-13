// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_agent_profiles::{
    ApprovalReviewAgentProfile, ApprovalReviewInput, APPROVAL_CAPABILITY_SNAPSHOT_REF,
    APPROVAL_DECISION_TOOL, APPROVAL_PROFILE_KEY, APPROVAL_PROMPT_REVISION,
};
use chatos_client_storage::{
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
};
use chatos_local_agent_host::StoredApprovalReviewContextProvider;
use chatos_local_agent_protocol::{ContextStrategy, ModelProtocol, ModelRuntimeDescriptor};
use chatos_local_agent_runtime::{
    create_local_agent_run, CreateLocalAgentRunRequest, InitialRunMessage, LocalAgentProfile,
};
use chrono::Utc;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn input() -> ApprovalReviewInput {
    ApprovalReviewInput {
        review_id: "approval-1".to_string(),
        source: "shell".to_string(),
        cwd: "workspace".to_string(),
        operation: "git status --short".to_string(),
        requested_permissions_description: Some("Read repository status".to_string()),
        risk_level: "low".to_string(),
        risk_reason: None,
        reasoning_effort: Some("low".to_string()),
    }
}

fn descriptor(strategy: ContextStrategy) -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 200_000,
        maximum_output_tokens: 1_200,
        context_strategy: strategy,
        supports_streaming: true,
        supports_native_compaction: strategy == ContextStrategy::ProviderNative,
        supports_input_token_count: true,
    }
}

async fn context(
    strategy: ContextStrategy,
) -> (
    tempfile::TempDir,
    ApprovalReviewAgentProfile,
    chatos_local_agent_protocol::LocalAgentRun,
) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:approval-context-key").unwrap(),
            },
            &StorageEncryptionKey::new([92; 32]),
        )
        .await
        .unwrap(),
    );
    let request = input();
    let prompt = request.user_prompt();
    let created = create_local_agent_run(
        storage.as_ref(),
        CreateLocalAgentRunRequest {
            scope: scope(),
            run_id: "approval-run-1".to_string(),
            profile_key: APPROVAL_PROFILE_KEY.to_string(),
            owner_entity_type: "approval".to_string(),
            owner_entity_id: request.review_id.clone(),
            project_id: None,
            model_runtime_snapshot: descriptor(strategy),
            prompt_revision: APPROVAL_PROMPT_REVISION.to_string(),
            capability_snapshot_ref: APPROVAL_CAPABILITY_SNAPSHOT_REF.to_string(),
            origin_device_id: "device-1".to_string(),
            causation_id: "request-1".to_string(),
            deadline_at: None,
            initial_message: Some(InitialRunMessage {
                record_id: "approval-message-1".to_string(),
                turn_id: request.review_id.clone(),
                content: Some(prompt),
                structured_payload: Some(serde_json::json!({
                    "type": "approval_review",
                    "request": request,
                })),
                message_source: "approval_review".to_string(),
            }),
            initial_attachments: Vec::new(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    let provider = Arc::new(StoredApprovalReviewContextProvider::new(storage, scope()));
    (
        directory,
        ApprovalReviewAgentProfile::new(provider),
        created.run_record.run,
    )
}

#[tokio::test]
async fn provider_native_review_rebuilds_one_frozen_prompt_and_one_terminal_tool() {
    let (_directory, profile, run) = context(ContextStrategy::ProviderNative).await;
    let step = profile.prepare_model_step(&run).await.unwrap();
    assert_eq!(step.model_input_items.len(), 1);
    assert!(step.model_input_items[0]
        .to_string()
        .contains("git status --short"));
    assert_eq!(step.tools.len(), 1);
    assert_eq!(step.tools[0]["name"], APPROVAL_DECISION_TOOL);
    assert_eq!(step.reasoning_effort.as_deref(), Some("low"));
    assert!(step.native_compaction_threshold.is_some());
    assert!(step.memory_engine_active_threshold.is_none());
}

#[tokio::test]
async fn memory_engine_review_does_not_duplicate_the_synced_request() {
    let (_directory, profile, run) = context(ContextStrategy::MemoryEngine).await;
    let step = profile.prepare_model_step(&run).await.unwrap();
    assert!(step.model_input_items.is_empty());
    assert!(step.native_compaction_threshold.is_none());
    assert!(step.memory_engine_active_threshold.is_some());
    assert!(step.maximum_summary_attempts > 0);
}
