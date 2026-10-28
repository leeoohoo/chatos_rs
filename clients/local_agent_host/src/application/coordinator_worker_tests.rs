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
    HostRequestEnvelope, HostResult, LocalAgentHostSchedulerSnapshot,
    LocalAgentResourcePressureLevel, LocalAgentRunClaim, LocalAgentStepOutcome, LocalTaskSpec,
    SetSchedulerAdmissionCommand, SetSchedulerPressureCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry, LocalAgentRuntime};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Notify};

struct UnusedProfile;

struct SuccessProfile;

struct BlockingProfile {
    started: mpsc::UnboundedSender<String>,
    release: Arc<Notify>,
}

#[async_trait]
impl LocalAgentProfile for BlockingProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        self.started
            .send(claim.run.run_id.clone())
            .map_err(|_| "start observer closed".to_string())?;
        self.release.notified().await;
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({"run_id": claim.run.run_id}),
        })
    }
}

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
        "model scheduling waited for a remote Memory request"
    );

    memory_release.notify_one();
    shutdown_sender.send(true).expect("shutdown");
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}

#[tokio::test]
async fn health_reports_active_idle_and_durable_queue_at_the_claim_boundary() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    for index in 0..3 {
        runtime
            .try_handle(envelope(
                &format!("create-run-{index}"),
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("parallel-run-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("conversation-{index}"),
                    profile_key: "blocking".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": index}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create Run");
    }
    let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
    let release = Arc::new(Notify::new());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "blocking",
            BlockingProfile {
                started: started_sender,
                release: Arc::clone(&release),
            },
        )
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
            .with_maximum_concurrent_work_items(2)
            .expect("concurrency"),
    );
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_receiver).await }
    });
    coordinator.wake();
    for _ in 0..2 {
        tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
            .await
            .expect("worker started before timeout")
            .expect("worker start");
    }

    let health = coordinator
        .handle_request(envelope("health-during-work", HostCommand::Health))
        .await;
    let Some(HostResult::Health {
        scheduler: Some(snapshot),
        ..
    }) = health.result
    else {
        panic!("expected scheduler health snapshot: {health:?}")
    };
    assert_eq!(snapshot.maximum_concurrent_work_items, 2);
    assert_eq!(snapshot.effective_maximum_concurrent_work_items, 2);
    assert_eq!(
        snapshot.pressure_level,
        LocalAgentResourcePressureLevel::Normal
    );
    assert_eq!(snapshot.active_work_items, 2);
    assert_eq!(snapshot.idle_workers, 0);
    assert_eq!(snapshot.queued_ready_work_items, 1);

    shutdown_sender.send(true).expect("shutdown");
    release.notify_waiters();
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}

#[tokio::test]
async fn pressure_gate_applies_before_the_next_claim_without_cancelling_active_work() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    for index in 0..4 {
        runtime
            .try_handle(envelope(
                &format!("create-pressure-run-{index}"),
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("pressure-run-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("pressure-conversation-{index}"),
                    profile_key: "blocking".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": index}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create pressure Run");
    }
    let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
    let release = Arc::new(Notify::new());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "blocking",
            BlockingProfile {
                started: started_sender,
                release: Arc::clone(&release),
            },
        )
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "local-pressure-worker",
    )
    .expect("scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_maximum_concurrent_work_items(2)
            .expect("concurrency"),
    );
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_receiver).await }
    });
    coordinator.wake();
    for _ in 0..2 {
        tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
            .await
            .expect("initial worker started before timeout")
            .expect("initial worker start");
    }

    let pressure = coordinator
        .handle_request(envelope(
            "set-critical-pressure",
            HostCommand::SetSchedulerPressure(SetSchedulerPressureCommand {
                pressure_level: LocalAgentResourcePressureLevel::Critical,
            }),
        ))
        .await;
    assert!(matches!(
        pressure.result,
        Some(HostResult::SchedulerPressureUpdated {
            pressure_level: LocalAgentResourcePressureLevel::Critical,
            effective_maximum_concurrent_work_items: 1,
        })
    ));
    let health = coordinator
        .handle_request(envelope("health-under-pressure", HostCommand::Health))
        .await;
    let Some(HostResult::Health {
        scheduler: Some(snapshot),
        ..
    }) = health.result
    else {
        panic!("expected scheduler health snapshot: {health:?}")
    };
    assert_eq!(snapshot.maximum_concurrent_work_items, 2);
    assert_eq!(snapshot.effective_maximum_concurrent_work_items, 1);
    assert_eq!(
        snapshot.pressure_level,
        LocalAgentResourcePressureLevel::Critical
    );
    assert_eq!(
        snapshot.active_work_items, 2,
        "active claims are not cancelled"
    );

    release.notify_waiters();
    tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
        .await
        .expect("one worker remains eligible")
        .expect("third Run start");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(150),
            started_receiver.recv()
        )
        .await
        .is_err(),
        "critical pressure must leave the fourth durable Run unclaimed"
    );

    let recovery = coordinator
        .handle_request(envelope(
            "restore-normal-pressure",
            HostCommand::SetSchedulerPressure(SetSchedulerPressureCommand {
                pressure_level: LocalAgentResourcePressureLevel::Normal,
            }),
        ))
        .await;
    assert!(matches!(
        recovery.result,
        Some(HostResult::SchedulerPressureUpdated {
            pressure_level: LocalAgentResourcePressureLevel::Normal,
            effective_maximum_concurrent_work_items: 2,
        })
    ));
    tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
        .await
        .expect("restored worker started before timeout")
        .expect("fourth Run start");

    shutdown_sender.send(true).expect("shutdown");
    release.notify_waiters();
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}

#[tokio::test]
async fn admission_close_drains_active_work_and_prevents_the_next_durable_claim() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    for index in 0..2 {
        runtime
            .try_handle(envelope(
                &format!("create-admission-run-{index}"),
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("admission-run-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("admission-conversation-{index}"),
                    profile_key: "blocking".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": index}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create admission Run");
    }
    let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
    let release = Arc::new(Notify::new());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "blocking",
            BlockingProfile {
                started: started_sender,
                release: Arc::clone(&release),
            },
        )
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "local-admission-worker",
    )
    .expect("scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_maximum_concurrent_work_items(1)
            .expect("concurrency"),
    );
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_receiver).await }
    });
    coordinator.wake();
    tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
        .await
        .expect("first worker started before timeout")
        .expect("first worker start");

    let closed = coordinator
        .handle_request(envelope(
            "close-scheduler-admission",
            HostCommand::SetSchedulerAdmission(SetSchedulerAdmissionCommand {
                accepting_new_work: false,
            }),
        ))
        .await;
    assert!(matches!(
        closed.result,
        Some(HostResult::SchedulerAdmissionUpdated {
            accepting_new_work: false,
        })
    ));
    let health = coordinator
        .handle_request(envelope("health-admission-closed", HostCommand::Health))
        .await;
    let Some(HostResult::Health {
        scheduler: Some(snapshot),
        ..
    }) = health.result
    else {
        panic!("expected scheduler health snapshot: {health:?}")
    };
    assert!(!snapshot.accepting_new_work);
    assert_eq!(snapshot.active_work_items, 1);

    release.notify_waiters();
    for _ in 0..100 {
        let health = coordinator
            .handle_request(envelope("health-admission-draining", HostCommand::Health))
            .await;
        if matches!(
            health.result,
            Some(HostResult::Health {
                scheduler: Some(LocalAgentHostSchedulerSnapshot {
                    active_work_items: 0,
                    ..
                }),
                ..
            })
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(150),
            started_receiver.recv()
        )
        .await
        .is_err(),
        "closed admission must leave the next durable Run unclaimed"
    );

    let reopened = coordinator
        .handle_request(envelope(
            "open-scheduler-admission",
            HostCommand::SetSchedulerAdmission(SetSchedulerAdmissionCommand {
                accepting_new_work: true,
            }),
        ))
        .await;
    assert!(matches!(
        reopened.result,
        Some(HostResult::SchedulerAdmissionUpdated {
            accepting_new_work: true,
        })
    ));
    tokio::time::timeout(std::time::Duration::from_secs(1), started_receiver.recv())
        .await
        .expect("second worker started after reopening")
        .expect("second worker start");

    shutdown_sender.send(true).expect("shutdown");
    release.notify_waiters();
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}
