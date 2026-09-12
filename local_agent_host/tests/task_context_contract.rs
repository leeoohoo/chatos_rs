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
use chatos_local_agent_host::StoredTaskRunnerContextProvider;
use chatos_local_agent_protocol::{
    ContextStrategy, FrozenSnapshot, ModelProtocol, ModelRuntimeDescriptor, ToolEffect,
    ToolExecution, ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    create_local_agent_task, CreateLocalAgentRunRequest, CreateLocalAgentTaskRequest,
    InitialRunMessage, LocalAgentProfile,
};
use chrono::{Duration, Utc};

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
