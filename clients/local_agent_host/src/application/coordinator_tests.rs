// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{
    decode_response, read_frame, serve_stream, write_frame, LocalToolExecutor, LocalToolRegistry,
};
use async_trait::async_trait;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRequirementSurveyCommand, CreateRunCommand, GetEventCursorCommand, GetRunCommand,
    HostCommand, HostResult, ListEventsCommand, LocalAgentRunClaim, LocalAgentStepOutcome,
    LocalAgentToolCall, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalRequirementSurveyQuestion, LocalRequirementSurveyResponseKind, RenewToolClaimCommand,
    WaitEventsCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, Barrier, Notify};
use uuid::Uuid;

struct ModelProfile;

struct ConcurrentSuccessProfile {
    barrier: Arc<Barrier>,
    started: Arc<AtomicUsize>,
}

struct LateArrivalProfile {
    first_started: Arc<Notify>,
    release_first: Arc<Notify>,
    second_started: Arc<Notify>,
}

struct BlockingMixedProfile {
    started: mpsc::UnboundedSender<String>,
    release: Arc<Notify>,
}

struct SignalingToolExecutor {
    started: Arc<Notify>,
}

struct ConcurrentToolExecutor {
    barrier: Arc<Barrier>,
    started: Arc<AtomicUsize>,
}

#[async_trait]
impl LocalToolExecutor for ConcurrentToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.barrier.wait().await;
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({"invocation_id": invocation.invocation_id}),
        })
    }
}

#[async_trait]
impl LocalToolExecutor for SignalingToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        self.started.notify_one();
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({"invocation_id": invocation.invocation_id}),
        })
    }
}

#[async_trait]
impl LocalAgentProfile for ConcurrentSuccessProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.barrier.wait().await;
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({"run_id": claim.run.run_id}),
        })
    }
}

#[async_trait]
impl LocalAgentProfile for LateArrivalProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        if claim.run.run_id == "run-first" {
            self.first_started.notify_one();
            self.release_first.notified().await;
        } else if claim.run.run_id == "run-second" {
            self.second_started.notify_one();
        }
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({"run_id": claim.run.run_id}),
        })
    }
}

#[async_trait]
impl LocalAgentProfile for BlockingMixedProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        self.started
            .send(claim.run.run_id.clone())
            .map_err(|_| "model start observer closed".to_string())?;
        self.release.notified().await;
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({"run_id": claim.run.run_id}),
        })
    }
}

#[tokio::test]
async fn coordinator_executes_independent_model_runs_concurrently() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    for index in 0..4 {
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("create-concurrent-{index}"),
                command: HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("run-concurrent-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("conversation-{index}"),
                    profile_key: "concurrent".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": format!("work-{index}")}),
                    max_iterations: 4,
                }),
            })
            .await
            .expect("create concurrent Run");
    }
    let started = Arc::new(AtomicUsize::new(0));
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "concurrent",
            ConcurrentSuccessProfile {
                barrier: Arc::new(Barrier::new(4)),
                started: Arc::clone(&started),
            },
        )
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_maximum_concurrent_work_items(4)
            .expect("four model slots");
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);

    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.drain_ready_work(&shutdown_rx),
    )
    .await
    .expect("four model Runs should reach the barrier together")
    .expect("drain concurrent Runs");

    assert_eq!(started.load(Ordering::SeqCst), 4);
    for index in 0..4 {
        let run = runtime
            .get_run_for_host_worker(&format!("run-concurrent-{index}"))
            .await
            .expect("get Run")
            .expect("persisted Run");
        assert_eq!(run.status.as_str(), "succeeded");
    }
}

#[tokio::test]
async fn coordinator_refills_an_idle_slot_when_a_run_arrives_mid_execution() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-first".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-first".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-first".to_string(),
                profile_key: "late-arrival".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "first"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create first Run");

    let first_started = Arc::new(Notify::new());
    let release_first = Arc::new(Notify::new());
    let second_started = Arc::new(Notify::new());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "late-arrival",
            LateArrivalProfile {
                first_started: Arc::clone(&first_started),
                release_first: Arc::clone(&release_first),
                second_started: Arc::clone(&second_started),
            },
        )
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_maximum_concurrent_work_items(2)
            .expect("two work slots"),
    );
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_receiver).await }
    });

    tokio::time::timeout(Duration::from_secs(1), first_started.notified())
        .await
        .expect("first Run started");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-second".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-second".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-second".to_string(),
                profile_key: "late-arrival".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "second"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create second Run");
    coordinator.wake();

    tokio::time::timeout(Duration::from_secs(1), second_started.notified())
        .await
        .expect("an idle slot should claim the late-arriving Run");

    release_first.notify_one();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let first = runtime
                .get_run_for_host_worker("run-first")
                .await
                .expect("read first Run")
                .expect("first Run");
            let second = runtime
                .get_run_for_host_worker("run-second")
                .await
                .expect("read second Run")
                .expect("second Run");
            if first.status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded
                && second.status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both Runs completed");

    shutdown_sender.send(true).expect("shutdown");
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}

#[tokio::test]
async fn coordinator_executes_independent_tool_calls_concurrently() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    for index in 0..4 {
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("create-tool-run-{index}"),
                command: HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("tool-run-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("tool-conversation-{index}"),
                    profile_key: "tool-test".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": format!("tool-work-{index}")}),
                    max_iterations: 4,
                }),
            })
            .await
            .expect("create tool Run");
        let HostResult::Claim { claim: Some(claim) } = runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("claim-tool-run-{index}"),
                command: HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 30_000,
                }),
            })
            .await
            .expect("claim tool Run")
        else {
            panic!("expected tool Run claim")
        };
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("wait-for-tool-{index}"),
                command: HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: format!("tool-batch-{index}"),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: format!("tool-call-{index}"),
                            tool_name: "concurrent_tool".to_string(),
                            arguments: json!({"index": index}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({"index": index}),
                    },
                }),
            })
            .await
            .expect("commit tool batch");
    }
    let started = Arc::new(AtomicUsize::new(0));
    let mut tools = LocalToolRegistry::new();
    tools
        .register(
            "concurrent_tool",
            ConcurrentToolExecutor {
                barrier: Arc::new(Barrier::new(4)),
                started: Arc::clone(&started),
            },
        )
        .expect("register concurrent tool");
    let scheduler = LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "tool-worker")
        .expect("tool scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", None, Some(scheduler))
            .expect("coordinator")
            .with_maximum_concurrent_work_items(4)
            .expect("four tool slots");
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);

    tokio::time::timeout(
        Duration::from_secs(2),
        coordinator.drain_ready_work(&shutdown_rx),
    )
    .await
    .expect("four tool calls should reach the barrier together")
    .expect("drain concurrent tools");

    assert_eq!(started.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn ready_tool_is_not_stranded_behind_slow_model_backlog() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");

    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-tool-owner".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-tool-owner".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-tool".to_string(),
                profile_key: "mixed".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "tool owner"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create Tool owner Run");
    let HostResult::Claim {
        claim: Some(tool_owner_claim),
    } = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-tool-owner".to_string(),
            command: HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "fixture-model-worker".to_string(),
                lease_duration_ms: 30_000,
            }),
        })
        .await
        .expect("claim Tool owner Run")
    else {
        panic!("expected Tool owner claim")
    };
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "enqueue-ready-tool".to_string(),
            command: HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: tool_owner_claim.run.run_id,
                claim_token: tool_owner_claim.claim_token,
                expected_version: tool_owner_claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "mixed-batch".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "mixed-tool-call".to_string(),
                        tool_name: "signaling_tool".to_string(),
                        arguments: json!({}),
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"phase": "waiting_for_tool"}),
                },
            }),
        })
        .await
        .expect("enqueue ready Tool");

    for index in 0..2 {
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("create-slow-model-{index}"),
                command: HostCommand::CreateRun(CreateRunCommand {
                    run_id: format!("run-slow-model-{index}"),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: format!("conversation-slow-{index}"),
                    profile_key: "mixed".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": index}),
                    max_iterations: 4,
                }),
            })
            .await
            .expect("create slow model Run");
    }

    let (model_started_tx, mut model_started_rx) = mpsc::unbounded_channel();
    let model_release = Arc::new(Notify::new());
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register(
            "mixed",
            BlockingMixedProfile {
                started: model_started_tx,
                release: Arc::clone(&model_release),
            },
        )
        .expect("mixed profile");
    let model_scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "mixed-model-worker",
    )
    .expect("model scheduler");
    let tool_started = Arc::new(Notify::new());
    let mut tools = LocalToolRegistry::new();
    tools
        .register(
            "signaling_tool",
            SignalingToolExecutor {
                started: Arc::clone(&tool_started),
            },
        )
        .expect("signaling Tool");
    let tool_scheduler =
        LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "mixed-tool-worker")
            .expect("tool scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(
            Arc::clone(&runtime),
            "user-1",
            Some(model_scheduler),
            Some(tool_scheduler),
        )
        .expect("coordinator")
        .with_maximum_concurrent_work_items(2)
        .expect("two shared slots"),
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let coordinator_task = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.run_until_shutdown(shutdown_rx).await }
    });
    coordinator.wake();

    tokio::time::timeout(Duration::from_secs(1), model_started_rx.recv())
        .await
        .expect("a model Run started")
        .expect("model start event");
    tokio::time::timeout(Duration::from_secs(1), tool_started.notified())
        .await
        .expect("ready Tool must receive a shared slot while model work is blocked");
    tokio::time::timeout(Duration::from_secs(1), model_started_rx.recv())
        .await
        .expect("the released Tool slot refills with the second model Run")
        .expect("second model start event");

    model_release.notify_waiters();
    shutdown_tx.send(true).expect("shutdown");
    coordinator.wake();
    coordinator_task
        .await
        .expect("coordinator task")
        .expect("coordinator shutdown");
}

#[test]
fn read_only_ipc_does_not_wake_scheduler_or_signal_activity() {
    let commands = [
        HostCommand::Health,
        HostCommand::SetSchedulerPressure(
            chatos_local_agent_protocol::SetSchedulerPressureCommand {
                pressure_level:
                    chatos_local_agent_protocol::LocalAgentResourcePressureLevel::Elevated,
            },
        ),
        HostCommand::SetSchedulerAdmission(
            chatos_local_agent_protocol::SetSchedulerAdmissionCommand {
                accepting_new_work: false,
            },
        ),
        HostCommand::GetEventCursor(GetEventCursorCommand {
            owner_user_id: "user-1".to_string(),
        }),
        HostCommand::ListEvents(ListEventsCommand {
            owner_user_id: "user-1".to_string(),
            after_cursor: 0,
            limit: 100,
            run_id: None,
            event_type: None,
            newest_first: false,
            payload_mode: chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
        }),
    ];
    for command in commands {
        assert!(!command_wakes_scheduler(&command));
        assert_eq!(
            command_activity_signal_policy(&command),
            ActivitySignalPolicy::Never
        );
    }

    let tool_claim = HostCommand::ClaimNextTool(ClaimNextToolCommand {
        owner_user_id: "user-1".to_string(),
        worker_id: "native-worker".to_string(),
        lease_duration_ms: 10_000,
        include_tool_names: None,
        exclude_tool_names: Vec::new(),
        execution_route: None,
    });
    assert!(!command_wakes_scheduler(&tool_claim));
    assert_eq!(
        command_activity_signal_policy(&tool_claim),
        ActivitySignalPolicy::ToolClaimed
    );
    assert!(!response_signals_activity(
        ActivitySignalPolicy::ToolClaimed,
        &HostResponseEnvelope::success(
            "empty-tool-claim".to_string(),
            HostResult::ToolClaim { claim: None },
        )
    ));

    let tool_renewal = HostCommand::RenewToolClaim(RenewToolClaimCommand {
        owner_user_id: "user-1".to_string(),
        invocation_id: "invocation-1".to_string(),
        claim_token: "claim-1".to_string(),
        expected_version: 2,
        lease_duration_ms: 10_000,
    });
    assert!(!command_wakes_scheduler(&tool_renewal));
    assert_eq!(
        command_activity_signal_policy(&tool_renewal),
        ActivitySignalPolicy::Never
    );
}

#[test]
fn pressure_policy_scales_configured_workers_and_keeps_one_critical_worker() {
    use chatos_local_agent_protocol::LocalAgentResourcePressureLevel::{
        Critical, Elevated, Normal, Serious,
    };

    assert_eq!(effective_worker_limit(1, Normal), 1);
    assert_eq!(effective_worker_limit(1, Critical), 1);
    assert_eq!(effective_worker_limit(3, Elevated), 3);
    assert_eq!(effective_worker_limit(4, Elevated), 3);
    assert_eq!(effective_worker_limit(3, Serious), 2);
    assert_eq!(effective_worker_limit(4, Serious), 2);
    assert_eq!(effective_worker_limit(16, Critical), 1);
}

#[async_trait]
impl LocalAgentProfile for ModelProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<chatos_local_agent_protocol::LocalAgentStepOutcome, String> {
        if claim.run.continuation_input.is_some() {
            Ok(
                chatos_local_agent_protocol::LocalAgentStepOutcome::Succeed {
                    output: json!({"completed": true}),
                },
            )
        } else {
            Ok(
                chatos_local_agent_protocol::LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-coordinator".to_string(),
                    tool_calls: vec![chatos_local_agent_protocol::LocalAgentToolCall {
                        call_id: "call-coordinator".to_string(),
                        tool_name: "read_file".to_string(),
                        arguments: json!({"path": "README.md"}),
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"model_step": 1}),
                },
            )
        }
    }
}

struct ReadFile;

#[async_trait]
impl LocalToolExecutor for ReadFile {
    async fn execute_tool(
        &self,
        _invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({"content": "hello"}),
        })
    }
}

#[tokio::test]
async fn coordinator_rejects_requests_for_a_different_active_account() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", ModelProfile)
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator");
    let rejected = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "cross-account-create".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-other".to_string(),
                owner_user_id: "user-2".to_string(),
                owner_entity_type: "test".to_string(),
                owner_entity_id: "entity-other".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "default".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "must not persist"}),
                max_iterations: 4,
            }),
        })
        .await;
    assert_eq!(
        rejected.error.expect("account mismatch").code,
        "account_mismatch"
    );
    assert!(runtime
        .get_run_for_host_worker("run-other")
        .await
        .expect("lookup")
        .is_none());

    let health = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "health".to_string(),
            command: HostCommand::Health,
        })
        .await;
    assert!(health.ok);

    let reserved = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "external-survey-create".to_string(),
            command: HostCommand::CreateRequirementSurvey(CreateRequirementSurveyCommand {
                survey_id: "survey-external".to_string(),
                owner_user_id: "user-1".to_string(),
                project_resource_id: "project-1".to_string(),
                source_conversation_id: "conversation-1".to_string(),
                source_run_id: "run-1".to_string(),
                source_task_id: Some("task-1".to_string()),
                title: "Must be rejected".to_string(),
                description: None,
                questions: vec![LocalRequirementSurveyQuestion {
                    question_id: "confirm".to_string(),
                    prompt: "Continue?".to_string(),
                    response_kind: LocalRequirementSurveyResponseKind::Boolean,
                    required: true,
                    options: Vec::new(),
                }],
            }),
        })
        .await;
    assert_eq!(
        reserved.error.expect("reserved command").code,
        "reserved_command"
    );
}

#[tokio::test]
async fn coordinator_rejects_external_commits_for_host_owned_tools() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-reserved-tool-run".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "reserved-tool-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "default".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "create a Task"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create Run");
    let claim = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-reserved-tool-run".to_string(),
            command: HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 30_000,
            }),
        })
        .await
        .expect("claim Run");
    let HostResult::Claim { claim: Some(claim) } = claim else {
        panic!("expected Run claim")
    };
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "commit-reserved-tool-batch".to_string(),
            command: HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "reserved-tool-batch".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "create-task-call".to_string(),
                        tool_name: "create_task".to_string(),
                        arguments: json!({"title": "Task", "objective": "Do work"}),
                        side_effecting: true,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"model_step": 1}),
                },
            }),
        })
        .await
        .expect("commit tool batch");
    let tool_claim = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-reserved-tool".to_string(),
            command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "local-tool-worker".to_string(),
                lease_duration_ms: 30_000,
                include_tool_names: Some(vec!["create_task".to_string()]),
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        })
        .await
        .expect("claim tool");
    let HostResult::ToolClaim {
        claim: Some(tool_claim),
    } = tool_claim
    else {
        panic!("expected tool claim")
    };
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", ModelProfile)
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_reserved_ipc_tools(["create_task"])
            .expect("reserved tools");
    let commit = CommitToolCommand {
        owner_user_id: "user-1".to_string(),
        invocation_id: tool_claim.invocation.invocation_id,
        claim_token: tool_claim.claim_token,
        expected_version: tool_claim.invocation.version,
        outcome: LocalAgentToolOutcome::Succeeded {
            output: json!({"task_id": "forged"}),
        },
    };

    let rejected = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "external-reserved-tool-commit".to_string(),
            command: HostCommand::CommitTool(commit.clone()),
        })
        .await;

    assert_eq!(
        rejected.error.expect("reserved commit").code,
        "reserved_command"
    );
    assert!(matches!(
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: "internal-reserved-tool-commit".to_string(),
                command: HostCommand::CommitTool(commit),
            })
            .await
            .expect("built-in worker commit"),
        HostResult::ToolCommit { .. }
    ));
}

#[tokio::test]
async fn coordinator_drains_model_tool_model_without_polling() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-coordinator".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-coordinator".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "coordinator".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create Run inside Host");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("coordinator", ModelProfile)
        .expect("profile");
    let model_scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("model scheduler");
    let mut tools = LocalToolRegistry::new();
    tools.register("read_file", ReadFile).expect("tool");
    let tool_scheduler =
        LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "tool-worker")
            .expect("tool scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(
            Arc::clone(&runtime),
            "user-1",
            Some(model_scheduler),
            Some(tool_scheduler),
        )
        .expect("coordinator")
        .with_reserved_ipc_tools(["create_task"])
        .expect("reserved tools"),
    );
    let mut external_claim = HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: "claim-native-tool".to_string(),
        command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: "user-1".to_string(),
            worker_id: "native-worker".to_string(),
            lease_duration_ms: 10_000,
            include_tool_names: None,
            exclude_tool_names: Vec::new(),
            execution_route: None,
        }),
    };
    coordinator.route_external_tool_claim(&mut external_claim);
    let HostCommand::ClaimNextTool(routed) = external_claim.command else {
        panic!("expected tool claim")
    };
    assert_eq!(routed.exclude_tool_names, vec!["create_task"]);
    assert_eq!(
        routed.execution_route,
        Some(chatos_local_agent_protocol::LocalAgentToolExecutionRoute::Platform)
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let wait_task = {
        let coordinator = Arc::clone(&coordinator);
        tokio::spawn(async move {
            coordinator
                .handle_request(HostRequestEnvelope {
                    protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                    command_id: "wait-coordinator-events".to_string(),
                    command: HostCommand::WaitEvents(WaitEventsCommand {
                        owner_user_id: "user-1".to_string(),
                        after_cursor: 0,
                        limit: 50,
                        run_id: Some("run-coordinator".to_string()),
                        timeout_ms: 2_000,
                        payload_mode: chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
                    }),
                })
                .await
        })
    };
    tokio::task::yield_now().await;
    let task = {
        let coordinator = Arc::clone(&coordinator);
        tokio::spawn(async move { coordinator.run_until_shutdown(shutdown_rx).await })
    };
    let (mut client, server) = tokio::io::duplex(16 * 1024);
    let ipc_task = tokio::spawn(serve_stream(server, Arc::clone(&coordinator)));
    let request = HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: "health-coordinator-ipc".to_string(),
        command: HostCommand::Health,
    };
    write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
        .await
        .expect("write");
    let response = read_frame(&mut client)
        .await
        .expect("read")
        .expect("response");
    let response = decode_response(&response).expect("decode");
    assert!(response.ok);
    let waited = wait_task.await.expect("wait join");
    assert!(response_has_events(&waited), "wait response: {waited:?}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let response = runtime
            .handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("get-{}", Uuid::new_v4()),
                command: HostCommand::GetRun(GetRunCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: "run-coordinator".to_string(),
                }),
            })
            .await;
        let status = match response.result.expect("run") {
            HostResult::Run { run } => run.status,
            result => panic!("unexpected result: {result:?}"),
        };
        if status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "coordinator timed out"
        );
        tokio::task::yield_now().await;
    }
    shutdown_tx.send(true).expect("shutdown");
    task.await.expect("join").expect("coordinator");
    drop(client);
    ipc_task.await.expect("IPC join").expect("IPC server");
}
