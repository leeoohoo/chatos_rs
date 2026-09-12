// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerAgentProfile, TaskRunnerCapabilitySnapshot, TaskRunnerContextProvider,
    TaskRunnerExecutionTool, TaskRunnerProjectSnapshot, TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    ClientStorage, PutRecord, RecordMetadata, RecordQuery, RecordScope, SecretReference,
    SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey, StorageResult,
    StorageTransaction, ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAttachmentLocator, LocalAttachmentResolver, StoredTaskRunnerContextProvider,
};
use chatos_local_agent_protocol::{
    ContextStrategy, FrozenSnapshot, LocalAgentRunStatus, LocalAttachmentReference, ModelProtocol,
    ModelRuntimeDescriptor, ToolEffect, ToolExecution, ToolExecutionStatus, UserInteractionAnswer,
};
use chatos_local_agent_runtime::{
    answer_run_interaction, create_local_agent_task, AnswerRunInteraction,
    CreateLocalAgentRunRequest, CreateLocalAgentTaskRequest, InitialRunMessage, LocalAgentProfile,
};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};

struct NoAttachments;

#[async_trait]
impl LocalAttachmentResolver for NoAttachments {
    async fn resolve(&self, _attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String> {
        Err("test has no attachments".to_string())
    }
}

struct BytesResolver(Vec<u8>);

#[async_trait]
impl LocalAttachmentResolver for BytesResolver {
    async fn resolve(&self, _attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String> {
        Ok(self.0.clone())
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-task-1".to_string(),
        revision: 7,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn snapshots() -> (FrozenSnapshot, FrozenSnapshot, FrozenSnapshot) {
    let prompt = FrozenSnapshot::new(
        "prompt-snapshot-1",
        "prompt-revision-1",
        serde_json::to_value(TaskRunnerPromptSnapshot {
            prompt_revision: "prompt-revision-1".to_string(),
            base_system_prompt: "Work as a careful local implementation agent.".to_string(),
            task_prompt: "Preserve the approved visual design and verify the result.".to_string(),
            skill_snapshot: serde_json::json!({"skills": ["visual-verification"]}),
        })
        .unwrap(),
    )
    .unwrap();
    let project = FrozenSnapshot::new(
        "project-snapshot-1",
        "project-revision-1",
        serde_json::to_value(TaskRunnerProjectSnapshot {
            project_id: "project-1".to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            working_directory_ref: "workspace-grant-1".to_string(),
            authority_snapshot: serde_json::json!({"device_id": "device-1"}),
        })
        .unwrap(),
    )
    .unwrap();
    let capability = FrozenSnapshot::new(
        "capabilities-1",
        "capability-revision-1",
        serde_json::to_value(TaskRunnerCapabilitySnapshot {
            snapshot_ref: "capabilities-1".to_string(),
            plugin_release_snapshot: serde_json::json!({"plugins": []}),
            execution_tools: vec![TaskRunnerExecutionTool {
                name: "read_file".to_string(),
                effect: ToolEffect::Read,
                schema: serde_json::json!({
                    "type": "function",
                    "name": "read_file",
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"],
                        "additionalProperties": false
                    }
                }),
            }],
        })
        .unwrap(),
    )
    .unwrap();
    (prompt, project, capability)
}

struct SeedReceipt {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedReceipt {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .tool_executions()
            .put(PutRecord {
                record: ToolExecutionStateRecord {
                    metadata: RecordMetadata {
                        id: "invocation-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: self.now,
                        updated_at: self.now,
                    },
                    execution: ToolExecution {
                        invocation_id: "invocation-1".to_string(),
                        run_id: "task-run-1".to_string(),
                        batch_id: "batch-1".to_string(),
                        tool_call_id: "call-1".to_string(),
                        tool_name: "read_file".to_string(),
                        effect: ToolEffect::Read,
                        arguments_digest: format!("sha256:{}", "a".repeat(64)),
                        status: ToolExecutionStatus::Succeeded,
                        bounded_result: Some(serde_json::json!({
                            "verification": true,
                            "summary": "The requested file was read from the frozen workspace."
                        })),
                        approval_decided_at: None,
                        approval_reason: None,
                        started_at: Some(self.now - Duration::seconds(1)),
                        completed_at: Some(self.now),
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct TamperProjectSnapshot;

#[async_trait]
impl StorageTransaction for TamperProjectSnapshot {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: scope(),
            id: "task-1".to_string(),
        };
        let mut task = repositories.tasks().get(&query).await?.unwrap();
        let revision = task.metadata.revision;
        task.state["project_snapshot"]["payload"]["working_directory_ref"] =
            serde_json::json!("different-workspace");
        repositories
            .tasks()
            .put(PutRecord {
                record: task,
                expected_revision: Some(revision),
            })
            .await?;
        Ok(())
    }
}

struct PauseTaskForAnswer;

#[async_trait]
impl StorageTransaction for PauseTaskForAnswer {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: scope(),
            id: "task-run-visual".to_string(),
        };
        let mut record = repositories.agent_runs().get(&query).await?.unwrap();
        let revision = record.metadata.revision;
        record.run.status = LocalAgentRunStatus::Paused;
        record.run.version += 1;
        record.run.pending_interaction = Some(serde_json::json!({
            "type": "ask_user",
            "interaction_id": "task-interaction-1",
            "question": {
                "prompt": "Annotate the implementation reference",
                "options": [],
                "image_references": [],
                "details": null
            }
        }));
        repositories
            .agent_runs()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn provider_rebuilds_task_runner_context_only_from_frozen_task_state_and_receipts() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:task-context-key").unwrap(),
            },
            &StorageEncryptionKey::new([63; 32]),
        )
        .await
        .unwrap(),
    );
    let (prompt_snapshot, project_snapshot, capability_snapshot) = snapshots();
    let objective = "Implement the approved design".to_string();
    let acceptance_criteria = vec!["The rendered result matches the visual reference".to_string()];
    let created = create_local_agent_task(
        storage.as_ref(),
        CreateLocalAgentTaskRequest {
            run: CreateLocalAgentRunRequest {
                scope: scope(),
                run_id: "task-run-1".to_string(),
                profile_key: "task_runner".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-1".to_string(),
                project_id: Some("project-1".to_string()),
                model_runtime_snapshot: descriptor(),
                prompt_revision: prompt_snapshot.revision.clone(),
                capability_snapshot_ref: capability_snapshot.snapshot_id.clone(),
                origin_device_id: "device-1".to_string(),
                causation_id: "create-task-1".to_string(),
                deadline_at: None,
                initial_message: Some(InitialRunMessage {
                    record_id: "task-message-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    content: Some(objective.clone()),
                    structured_payload: Some(serde_json::json!({"type": "task_objective"})),
                    message_source: "task_creation".to_string(),
                }),
                initial_attachments: Vec::new(),
                now,
            },
            task_id: "task-1".to_string(),
            source_thread_id: "thread-1".to_string(),
            source_turn_id: "turn-1".to_string(),
            project_id: "project-1".to_string(),
            objective,
            acceptance_criteria,
            prompt_snapshot,
            project_snapshot,
            capability_snapshot,
        },
    )
    .await
    .unwrap();
    storage.transaction(&mut SeedReceipt { now }).await.unwrap();

    let provider = Arc::new(StoredTaskRunnerContextProvider::new(
        storage.clone(),
        scope(),
        Arc::new(NoAttachments),
    ));
    let context = provider
        .load_step_context(&created.run.run_record.run)
        .await
        .unwrap();
    assert_eq!(context.project_snapshot.project_id, "project-1");
    assert_eq!(
        context.prompt_snapshot.base_system_prompt,
        "Work as a careful local implementation agent."
    );
    assert_eq!(context.tool_receipts.len(), 1);
    assert!(context.tool_receipts[0].verification);
    assert_eq!(context.native_compaction_threshold, Some(294_400));
    assert_eq!(context.memory_engine_active_threshold, None);
    assert_eq!(context.maximum_summary_attempts, 0);

    let profile = TaskRunnerAgentProfile::new(provider.clone());
    let step = profile
        .prepare_model_step(&created.run.run_record.run)
        .await
        .unwrap();
    assert!(step
        .tools
        .iter()
        .any(|tool| tool.get("name").and_then(serde_json::Value::as_str) == Some("read_file")));

    storage
        .transaction(&mut TamperProjectSnapshot)
        .await
        .unwrap();
    let error = provider
        .load_step_context(&created.run.run_record.run)
        .await
        .unwrap_err();
    assert!(error.contains("integrity validation"));
}

#[tokio::test]
async fn task_runner_resume_rebuilds_a_visual_user_answer_from_durable_storage() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:task-visual-key").unwrap(),
            },
            &StorageEncryptionKey::new([64; 32]),
        )
        .await
        .unwrap(),
    );
    let (prompt_snapshot, project_snapshot, capability_snapshot) = snapshots();
    let created = create_local_agent_task(
        storage.as_ref(),
        CreateLocalAgentTaskRequest {
            run: CreateLocalAgentRunRequest {
                scope: scope(),
                run_id: "task-run-visual".to_string(),
                profile_key: "task_runner".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-visual".to_string(),
                project_id: Some("project-1".to_string()),
                model_runtime_snapshot: descriptor(),
                prompt_revision: prompt_snapshot.revision.clone(),
                capability_snapshot_ref: capability_snapshot.snapshot_id.clone(),
                origin_device_id: "device-1".to_string(),
                causation_id: "create-task-visual".to_string(),
                deadline_at: None,
                initial_message: Some(InitialRunMessage {
                    record_id: "task-message-visual".to_string(),
                    turn_id: "turn-visual".to_string(),
                    content: Some("Implement the approved visual".to_string()),
                    structured_payload: Some(serde_json::json!({"type": "task_objective"})),
                    message_source: "task_creation".to_string(),
                }),
                initial_attachments: Vec::new(),
                now,
            },
            task_id: "task-visual".to_string(),
            source_thread_id: "thread-visual".to_string(),
            source_turn_id: "turn-visual".to_string(),
            project_id: "project-1".to_string(),
            objective: "Implement the approved visual".to_string(),
            acceptance_criteria: vec!["The visual matches the reference".to_string()],
            prompt_snapshot,
            project_snapshot,
            capability_snapshot,
        },
    )
    .await
    .unwrap();
    storage.transaction(&mut PauseTaskForAnswer).await.unwrap();
    let bytes = b"task-answer-image".to_vec();
    answer_run_interaction(
        storage.as_ref(),
        AnswerRunInteraction {
            scope: scope(),
            run_id: "task-run-visual".to_string(),
            interaction_id: "task-interaction-1".to_string(),
            answer: UserInteractionAnswer {
                text: Some("Use this spacing annotation.".to_string()),
                selected_option_ids: Vec::new(),
                attachments: vec![LocalAttachmentReference {
                    attachment_id: "task-answer-visual".to_string(),
                    media_type: "image/png".to_string(),
                    payload_reference: "attachment-grant:task-answer-visual".to_string(),
                    payload_digest: format!("sha256:{:x}", Sha256::digest(&bytes)),
                    byte_size: u64::try_from(bytes.len()).unwrap(),
                }],
            },
            origin_device_id: "device-1".to_string(),
            causation_id: "answer-task-visual".to_string(),
            now,
        },
    )
    .await
    .unwrap();
    let mut run = created.run.run_record.run;
    run.iteration = 1;
    let provider =
        StoredTaskRunnerContextProvider::new(storage, scope(), Arc::new(BytesResolver(bytes)));
    let context = provider.load_step_context(&run).await.unwrap();
    assert_eq!(context.model_input_items.len(), 1);
    let input = context.model_input_items[0].to_string();
    assert!(input.contains("spacing annotation"));
    assert!(input.contains("input_image"));
    assert!(!input.contains("attachment-grant"));
}
