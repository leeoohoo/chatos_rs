// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    LocalAgentHostCoordinator, LocalAgentScheduler, LocalMemoryOutboxWriter, LocalMemorySyncWorker,
};
use crate::HostRequestHandler;
use async_trait::async_trait;
use chatos_ai_runtime::{MemoryRecordWriter, SaveRecordInput};
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, CreateTaskGraphCommand, HostCommand,
    HostRequestEnvelope, HostResult, LocalAgentRunClaim, LocalAgentStepOutcome, LocalTaskSpec,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry, LocalAgentRuntime};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{watch, Notify};

struct UnusedProfile;

struct SuccessProfile;

#[async_trait]
impl LocalAgentProfile for SuccessProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({"run_id": claim.run.run_id}),
        })
    }
}

struct BlockingMemoryWriter {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl MemoryRecordWriter for BlockingMemoryWriter {
    async fn save_record(&self, _input: SaveRecordInput) -> Result<(), String> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(())
    }
}

#[async_trait]
impl LocalAgentProfile for UnusedProfile {
    async fn execute_step(
        &self,
        _claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        unreachable!("the coordinator loop is not started in this test")
    }
}

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[tokio::test]
async fn coordinator_keeps_model_claims_and_commits_inside_the_host() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(envelope(
            "create-model-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "model-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("create Run");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", UnusedProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "local-model-worker",
    )
    .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator");
    let rejected_run_creation = coordinator
        .handle_request(envelope(
            "external-run-create",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "forged-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "forged"}),
                max_iterations: 4,
            }),
        ))
        .await;
    let rejected_graph_creation = coordinator
        .handle_request(envelope(
            "external-graph-create",
            HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
                graph_id: "forged-graph".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-1".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "forged-task".to_string(),
                    title: "Forged".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "forged"}),
                    max_iterations: 4,
                }],
                dependencies: Vec::new(),
            }),
        ))
        .await;
    let claim_command = ClaimNextRunCommand {
        owner_user_id: "user-1".to_string(),
        worker_id: "external-model-worker".to_string(),
        lease_duration_ms: 30_000,
    };

    let rejected_claim = coordinator
        .handle_request(envelope(
            "external-model-claim",
            HostCommand::ClaimNextRun(claim_command.clone()),
        ))
        .await;

    assert_eq!(
        rejected_run_creation
            .error
            .expect("reserved Run creation")
            .code,
        "reserved_command"
    );
    assert_eq!(
        rejected_graph_creation
            .error
            .expect("reserved Task Graph creation")
            .code,
        "reserved_command"
    );
    assert_eq!(
        rejected_claim.error.expect("reserved claim").code,
        "reserved_command"
    );
    let internal_claim = runtime
        .try_handle(envelope(
            "internal-model-claim",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "local-model-worker".to_string(),
                ..claim_command
            }),
        ))
        .await
        .expect("internal claim");
    let HostResult::Claim {
        claim: Some(internal_claim),
    } = internal_claim
    else {
        panic!("expected internal Run claim")
    };
    let commit = CommitStepCommand {
        owner_user_id: "user-1".to_string(),
        run_id: internal_claim.run.run_id,
        claim_token: internal_claim.claim_token,
        expected_version: internal_claim.run.version,
        outcome: LocalAgentStepOutcome::Succeed {
            output: json!({"answer": "done"}),
        },
    };

    let rejected_commit = coordinator
        .handle_request(envelope(
            "external-model-commit",
            HostCommand::CommitStep(commit.clone()),
        ))
        .await;

    assert_eq!(
        rejected_commit.error.expect("reserved commit").code,
        "reserved_command"
    );
    assert!(matches!(
        runtime
            .try_handle(envelope(
                "internal-model-commit",
                HostCommand::CommitStep(commit),
            ))
            .await
            .expect("internal commit"),
        HostResult::Run { .. }
    ));
}

#[tokio::test]
async fn slow_remote_memory_sync_does_not_block_local_model_scheduling() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage.clone()));
    runtime.initialize("user-1").await.expect("initialize");
    let memory_writer =
        LocalMemoryOutboxWriter::with_clock(storage.clone(), "local_agent", Arc::new(|| Ok(1_000)))
            .expect("Memory writer");
    memory_writer
        .save_record(
            SaveRecordInput::user_message("conversation-1", "remember")
                .with_message_id("message-memory")
                .with_metadata(json!({"tenant_id": "user-1"})),
        )
        .await
        .expect("enqueue Memory record");

    let memory_started = Arc::new(Notify::new());
    let memory_release = Arc::new(Notify::new());
    let memory_worker = LocalMemorySyncWorker::with_clock(
        storage,
        Arc::new(BlockingMemoryWriter {
            started: Arc::clone(&memory_started),
            release: Arc::clone(&memory_release),
        }),
        "user-1",
        Arc::new(|| Ok(2_000)),
    )
    .expect("Memory worker");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", SuccessProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "local-model-worker",
    )
    .expect("scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_memory_sync_worker(memory_worker)
            .expect("Memory coordinator"),
    );
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_receiver).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), memory_started.notified())
        .await
        .expect("Memory upload started");

    runtime
        .try_handle(envelope(
            "create-run-during-memory-upload",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "model-run-during-memory".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("create Run");
    coordinator.wake();

    let completed = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let run = runtime
                .get_run_for_host_worker("model-run-during-memory")
                .await
                .expect("read Run")
                .expect("Run");
            if run.status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        completed.is_ok(),
        "local model scheduling waited for a remote Memory request"
    );

    memory_release.notify_one();
    shutdown_sender.send(true).expect("shutdown");
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}
