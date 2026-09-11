// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentHost, LocalAgentHostControlExecutor, LocalAgentHostPolicy, LocalAgentHostRunRequest,
    LocalAgentIpcMutationExecutor, LocalAgentProfileRegistry,
};
use chatos_local_agent_protocol::{
    AnswerUserQuestionCommand, ContextStrategy, LocalAgentCommand, LocalAgentEvent,
    LocalAgentEventStatus, LocalAgentEventType, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalAgentRun, LocalAgentRunStatus, ModelGatewayRequest, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepResult, UserInteractionAnswer,
};
use chatos_local_agent_runtime::{
    LocalAgentProfile, LocalAgentProfileStep, LocalToolInvocation, LocalToolOutcome,
    LocalToolRuntime, ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError,
    ModelGatewayOutput, SchedulerTickResult, StepEvidence,
};
use chrono::Utc;
use tokio_util::sync::CancellationToken;

struct Profile;

#[async_trait]
impl LocalAgentProfile for Profile {
    fn profile_key(&self) -> &'static str {
        "main_chat"
    }

    async fn prepare_model_step(
        &self,
        _run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        unreachable!("not executed by lifecycle test")
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        _output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        unreachable!("not executed by lifecycle test")
    }
}

struct Gateway;

struct Tools;

struct UnusedMutationExecutor;

#[async_trait]
impl LocalAgentIpcMutationExecutor for UnusedMutationExecutor {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        panic!("control command must not be delegated");
    }
}

#[async_trait]
impl LocalToolRuntime for Tools {
    async fn execute(
        &self,
        _invocation: LocalToolInvocation,
        _cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String> {
        unreachable!("not executed by lifecycle test")
    }
}

struct RecordingTools {
    invocations: Mutex<Vec<LocalToolInvocation>>,
    delay: std::time::Duration,
    fail: bool,
}

#[async_trait]
impl LocalToolRuntime for RecordingTools {
    async fn execute(
        &self,
        invocation: LocalToolInvocation,
        _cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String> {
        self.invocations.lock().unwrap().push(invocation);
        tokio::time::sleep(self.delay).await;
        if self.fail {
            Err("local transport became indeterminate".to_string())
        } else {
            Ok(LocalToolOutcome::succeeded(
                serde_json::json!({"verified": true}),
            ))
        }
    }
}

#[async_trait]
impl ModelGatewayClient for Gateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }
}

struct Seed {
    now: chrono::DateTime<Utc>,
}

struct SeedRunOnly {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedRunOnly {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: RecordMetadata {
                        id: "run-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: self.now,
                        updated_at: self.now,
                    },
                    run: run(self.now),
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct SeedPausedRunOnly {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedPausedRunOnly {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut paused = run(self.now);
        paused.status = LocalAgentRunStatus::Paused;
        paused.pending_interaction = Some(serde_json::json!({
            "type": "ask_user",
            "interaction_id": "interaction-1",
            "question": {
                "prompt": "Choose a direction",
                "options": [],
                "image_references": [],
                "details": null
            }
        }));
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: RecordMetadata {
                        id: "run-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: self.now,
                        updated_at: self.now,
                    },
                    run: paused,
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let scope = scope();
        let metadata = |id: &str| RecordMetadata {
            id: id.to_string(),
            scope: scope.clone(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: self.now,
            updated_at: self.now,
        };
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: metadata("run-1"),
                    run: run(self.now),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: AgentEventStateRecord {
                    metadata: metadata("event-1"),
                    event: LocalAgentEvent {
                        event_id: "event-1".to_string(),
                        run_id: "run-1".to_string(),
                        event_type: LocalAgentEventType::RunStarted,
                        expected_version: 1,
                        available_at: self.now,
                        status: LocalAgentEventStatus::Pending,
                        attempt_count: 0,
                        claimed_by_device_id: None,
                        claim_token: None,
                        claim_until: None,
                        causation_id: "turn-1".to_string(),
                        correlation_id: "thread-1-1".to_string(),
                        bounded_payload: serde_json::Value::Null,
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct SeedToolBatch {
    now: chrono::DateTime<Utc>,
    effect: &'static str,
}

#[async_trait]
impl StorageTransaction for SeedToolBatch {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut run = run(self.now);
        run.profile_key = "task_runner".to_string();
        run.owner_entity_type = "task".to_string();
        run.owner_entity_id = "task-1".to_string();
        run.project_id = Some("project-1".to_string());
        run.status = LocalAgentRunStatus::WaitingToolResult;
        run.pending_batch_id = Some("batch-1".to_string());
        let metadata = |id: &str| RecordMetadata {
            id: id.to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: self.now,
            updated_at: self.now,
        };
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: metadata("run-1"),
                    run,
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: AgentEventStateRecord {
                    metadata: metadata("event-tool-1"),
                    event: LocalAgentEvent {
                        event_id: "event-tool-1".to_string(),
                        run_id: "run-1".to_string(),
                        event_type: LocalAgentEventType::ToolBatchRequested,
                        expected_version: 1,
                        available_at: self.now,
                        status: LocalAgentEventStatus::Pending,
                        attempt_count: 0,
                        claimed_by_device_id: None,
                        claim_token: None,
                        claim_until: None,
                        causation_id: "model-step-1".to_string(),
                        correlation_id: "task-1".to_string(),
                        bounded_payload: serde_json::json!({
                            "project_id": "project-1",
                            "capability_snapshot_ref": "capabilities-1",
                            "calls": [{
                                "call_id": "call-1",
                                "name": "project_tool",
                                "effect": self.effect,
                                "arguments": {"path": "src/lib.rs"}
                            }]
                        }),
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn host_recovers_claims_commits_and_schedules_the_next_event() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:key").unwrap(),
            },
            &StorageEncryptionKey::new([7; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut Seed { now }).await.unwrap();
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, report) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();

    assert_eq!(report.active_run_count, 1);
    assert_eq!(report.ready_event_count, 1);
    assert!(report.recovery_issues.is_empty());
    assert_eq!(
        host.profiles().require("main_chat").unwrap().profile_key(),
        "main_chat"
    );

    let SchedulerTickResult::Claimed(started) = host.claim_next("claim-1", now).await.unwrap()
    else {
        panic!("run_started was not claimed");
    };
    host.commit_claimed(&started, StepEvidence::None, now)
        .await
        .unwrap();

    let SchedulerTickResult::Claimed(next) = host.claim_next("claim-2", now).await.unwrap() else {
        panic!("model step request was not scheduled");
    };
    assert_eq!(
        next.event.event_type,
        LocalAgentEventType::ModelStepRequested
    );
    assert_eq!(next.event.expected_version, 2);
    let begun = host.begin_claimed_model_step(&next, now).await.unwrap();
    assert_eq!(
        begun.run_record.run.status,
        LocalAgentRunStatus::ModelRunning
    );
    assert_eq!(begun.run_record.run.version, 3);
    assert_eq!(begun.request_event.event.expected_version, 3);
    assert_eq!(
        begun.request_event.event.status,
        LocalAgentEventStatus::Claimed
    );
}

#[tokio::test]
async fn host_schedules_a_durable_control_request_immediately() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:control-key").unwrap(),
            },
            &StorageEncryptionKey::new([12; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut SeedRunOnly { now }).await.unwrap();
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, report) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    assert_eq!(report.ready_event_count, 0);

    let host = Arc::new(host);
    let executor =
        LocalAgentHostControlExecutor::new(host.clone(), Arc::new(UnusedMutationExecutor));
    let response = executor
        .execute_mutation(
            "ipc-request-1",
            LocalAgentCommand::PauseRun {
                run_id: "run-1".to_string(),
            },
        )
        .await
        .unwrap();
    let LocalAgentIpcResponse::Accepted { operation_id } = response else {
        panic!("control executor must return an accepted operation");
    };
    let SchedulerTickResult::Claimed(claimed) = host
        .claim_next("control-claim", Utc::now() + chrono::Duration::seconds(1))
        .await
        .unwrap()
    else {
        panic!("new control event was not scheduled");
    };
    assert_eq!(claimed.event.event_id, operation_id);
    assert_eq!(
        claimed.event.event_type,
        LocalAgentEventType::PauseRequested
    );
}

#[tokio::test]
async fn host_creates_and_schedules_a_durable_run_start() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:create-run-key").unwrap(),
            },
            &StorageEncryptionKey::new([17; 32]),
        )
        .await
        .unwrap(),
    );
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let request = LocalAgentHostRunRequest {
        run_id: "created-run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "created-thread-1".to_string(),
        project_id: Some("project-1".to_string()),
        model_runtime_snapshot: run(now).model_runtime_snapshot,
        prompt_revision: "prompt-1".to_string(),
        capability_snapshot_ref: "capabilities-1".to_string(),
        causation_id: "created-turn-1".to_string(),
        deadline_at: None,
        initial_message: None,
    };
    let created = host.create_run(request.clone(), now).await.unwrap();
    let repeated = host
        .create_run(request, now + chrono::Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(created, repeated);
    let SchedulerTickResult::Claimed(claimed) = host
        .claim_next("created-run-claim", now + chrono::Duration::seconds(2))
        .await
        .unwrap()
    else {
        panic!("created Run start event was not scheduled");
    };
    assert_eq!(claimed.event.run_id, "created-run-1");
    assert_eq!(claimed.event.event_type, LocalAgentEventType::RunStarted);
}

#[tokio::test]
async fn host_persists_an_answer_and_schedules_resume_through_ipc() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:answer-key").unwrap(),
            },
            &StorageEncryptionKey::new([13; 32]),
        )
        .await
        .unwrap(),
    );
    storage
        .transaction(&mut SeedPausedRunOnly { now })
        .await
        .unwrap();
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let host = Arc::new(host);
    let executor =
        LocalAgentHostControlExecutor::new(host.clone(), Arc::new(UnusedMutationExecutor));
    let response = executor
        .execute_mutation(
            "ipc-answer-1",
            LocalAgentCommand::AnswerUserQuestion(AnswerUserQuestionCommand {
                run_id: "run-1".to_string(),
                interaction_id: "interaction-1".to_string(),
                answer: UserInteractionAnswer {
                    text: Some("Use editorial".to_string()),
                    selected_option_ids: Vec::new(),
                    attachments: Vec::new(),
                },
            }),
        )
        .await
        .unwrap();
    let LocalAgentIpcResponse::Accepted { operation_id } = response else {
        panic!("answer must return an accepted resume event");
    };
    let SchedulerTickResult::Claimed(claimed) = host
        .claim_next("answer-claim", Utc::now() + chrono::Duration::seconds(1))
        .await
        .unwrap()
    else {
        panic!("answer resume event was not scheduled");
    };
    assert_eq!(claimed.event.event_id, operation_id);
    assert_eq!(
        claimed.event.event_type,
        LocalAgentEventType::ResumeRequested
    );
}

#[tokio::test]
async fn host_renews_the_claim_and_commits_a_successful_local_tool_batch() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:tool-key").unwrap(),
            },
            &StorageEncryptionKey::new([41; 32]),
        )
        .await
        .unwrap(),
    );
    storage
        .transaction(&mut SeedToolBatch {
            now,
            effect: "read",
        })
        .await
        .unwrap();
    let tools = Arc::new(RecordingTools {
        invocations: Mutex::new(Vec::new()),
        delay: std::time::Duration::from_millis(140),
        fail: false,
    });
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let policy = LocalAgentHostPolicy {
        claim_ttl: chrono::Duration::milliseconds(90),
        ..LocalAgentHostPolicy::default()
    };
    let (host, _) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        tools.clone(),
        profiles,
        scope(),
        "device-1",
        policy,
        now,
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(claimed) = host.claim_next("tool-claim-1", now).await.unwrap()
    else {
        panic!("tool batch was not claimed");
    };
    let committed = host
        .execute_claimed_tool_batch(&claimed, CancellationToken::new(), now)
        .await
        .unwrap();
    assert_eq!(tools.invocations.lock().unwrap().len(), 1);
    assert_eq!(committed.emitted_events.len(), 1);
    assert_eq!(
        committed.emitted_events[0].event.event_type,
        LocalAgentEventType::ToolBatchCompleted
    );

    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("tool-complete-1", Utc::now())
        .await
        .unwrap()
    else {
        panic!("tool completion was not claimed");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::ContinuationReady
    );
}

#[tokio::test]
async fn indeterminate_irreversible_tool_result_moves_the_run_to_review() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:unknown-key").unwrap(),
            },
            &StorageEncryptionKey::new([40; 32]),
        )
        .await
        .unwrap(),
    );
    storage
        .transaction(&mut SeedToolBatch {
            now,
            effect: "write",
        })
        .await
        .unwrap();
    let tools = Arc::new(RecordingTools {
        invocations: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        fail: true,
    });
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        tools,
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(claimed) = host.claim_next("tool-claim-1", now).await.unwrap()
    else {
        panic!("tool batch was not claimed");
    };
    host.execute_claimed_tool_batch(&claimed, CancellationToken::new(), now)
        .await
        .unwrap();
    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("tool-complete-1", Utc::now())
        .await
        .unwrap()
    else {
        panic!("tool completion was not claimed");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::NeedsReview
    );
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn run(now: chrono::DateTime<Utc>) -> LocalAgentRun {
    let descriptor = ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    };
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "thread-1".to_string(),
        project_id: None,
        status: LocalAgentRunStatus::Queued,
        version: 1,
        step_seq: 0,
        iteration: 0,
        retry_count: 0,
        model_config_id: "model-1".to_string(),
        model_config_revision: 1,
        model_runtime_snapshot: descriptor,
        context_strategy: ContextStrategy::ProviderNative,
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
