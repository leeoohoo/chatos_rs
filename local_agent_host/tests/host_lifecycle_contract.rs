// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, ListQuery, PutRecord,
    RecordMetadata, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TaskRecord, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentContextRuntime, LocalAgentContextRuntimeError, LocalAgentExecutionSession,
    LocalAgentHost, LocalAgentHostControlExecutor, LocalAgentHostCreationExecutor,
    LocalAgentHostPolicy, LocalAgentHostRunRequest, LocalAgentIpcMutationExecutor,
    LocalAgentIpcServer, LocalAgentProfileRegistry, ProcessedClaimedEvent,
};
use chatos_local_agent_protocol::{
    AnswerUserQuestionCommand, ContextStrategy, CreateMainChatTurnCommand, CreateTaskCommand,
    FrozenSnapshotReference, LocalAgentCommand, LocalAgentEvent, LocalAgentEventStatus,
    LocalAgentEventType, LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse,
    LocalAgentRun, LocalAgentRunStatus, ModelGatewayRequest, ModelGatewayTerminal,
    ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepCompletion, ModelStepResult, UserInteractionAnswer,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{
    CompletedAssistantMessage, DurableProviderContextCommit, LocalAgentProfile,
    LocalAgentProfileStep, LocalToolInvocation, LocalToolOutcome, LocalToolRuntime,
    ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError, ModelGatewayOutput,
    ModelStepContext, ProviderNativeContextCommit, ProviderNativeContextWindow,
    SchedulerTickResult, StepEvidence,
};
use chrono::Utc;
use tokio_util::sync::CancellationToken;

struct Profile;

struct TaskProfile;

#[async_trait]
impl LocalAgentProfile for Profile {
    fn profile_key(&self) -> &'static str {
        "main_chat"
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        Ok(LocalAgentProfileStep {
            model_input_items: vec![serde_json::json!({
                "type": "message",
                "role": "user",
                "content": "complete this step"
            })],
            tools: Vec::new(),
            instructions: Some("Complete exactly one local Agent step.".to_string()),
            maximum_output_tokens: 32_000,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: (run.context_strategy == ContextStrategy::ProviderNative)
                .then_some(300_000),
            memory_engine_active_threshold: (run.context_strategy == ContextStrategy::MemoryEngine)
                .then_some(300_000),
            maximum_summary_attempts: if run.context_strategy == ContextStrategy::MemoryEngine {
                2
            } else {
                0
            },
        })
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        Ok(ModelStepResult::Final(
            serde_json::json!({"text": output.content}),
        ))
    }
}

#[async_trait]
impl LocalAgentProfile for TaskProfile {
    fn profile_key(&self) -> &'static str {
        "task_runner"
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        Profile.prepare_model_step(run).await
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        Ok(ModelStepResult::Final(
            serde_json::json!({"text": output.content}),
        ))
    }
}

struct Gateway;

struct DescriptorGateway {
    calls: Mutex<Vec<(String, String)>>,
}

struct Tools;

struct TestContextRuntime;

struct FailingMemoryContextRuntime;

#[async_trait]
impl LocalAgentContextRuntime for TestContextRuntime {
    async fn prepare_model_step_context(
        &self,
        _storage: &dyn ClientStorage,
        _scope: &RecordScope,
        _run: &LocalAgentRun,
        cancellation: &CancellationToken,
    ) -> Result<ModelStepContext, LocalAgentContextRuntimeError> {
        if cancellation.is_cancelled() {
            return Err(LocalAgentContextRuntimeError::Cancelled);
        }
        Ok(ModelStepContext::ProviderNative(
            ProviderNativeContextWindow::empty(1).unwrap(),
        ))
    }

    async fn seal_provider_context_commit(
        &self,
        _run: &LocalAgentRun,
        commit: ProviderNativeContextCommit,
        now: chrono::DateTime<Utc>,
    ) -> Result<DurableProviderContextCommit, LocalAgentContextRuntimeError> {
        Ok(DurableProviderContextCommit {
            generation: commit.generation,
            retained_items: commit
                .retained_items
                .into_iter()
                .enumerate()
                .map(
                    |(index, item)| chatos_local_agent_runtime::DurableProviderContextItem {
                        sequence: u64::try_from(index).unwrap() + 1,
                        item_type: item
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                        encrypted_payload: format!("sealed:{item}"),
                        payload_digest: format!("sha256:{}", "a".repeat(64)),
                        created_at: now,
                    },
                )
                .collect(),
        })
    }
}

#[async_trait]
impl LocalAgentContextRuntime for FailingMemoryContextRuntime {
    async fn prepare_model_step_context(
        &self,
        _storage: &dyn ClientStorage,
        _scope: &RecordScope,
        _run: &LocalAgentRun,
        _cancellation: &CancellationToken,
    ) -> Result<ModelStepContext, LocalAgentContextRuntimeError> {
        Err(LocalAgentContextRuntimeError::Runtime(
            "Memory Engine active summary is unavailable".to_string(),
        ))
    }

    async fn seal_provider_context_commit(
        &self,
        _run: &LocalAgentRun,
        _commit: ProviderNativeContextCommit,
        _now: chrono::DateTime<Utc>,
    ) -> Result<DurableProviderContextCommit, LocalAgentContextRuntimeError> {
        unreachable!("Memory Engine strategy has no provider context commit")
    }
}

struct ExecutingGateway {
    requests: Mutex<Vec<ModelGatewayRequest>>,
    delay: std::time::Duration,
    terminal_status: ModelGatewayTerminalStatus,
}

struct RetryableGateway {
    requests: Mutex<Vec<ModelGatewayRequest>>,
}

#[async_trait]
impl ModelGatewayClient for RetryableGateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("descriptor is frozen into the Run")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        self.requests.lock().unwrap().push(request);
        Err(ModelGatewayClientError::Transport {
            kind: "connection_reset",
        })
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        Ok(ModelGatewayTokenCount {
            request_id: request.request_id.clone(),
            model_config_id: request.model_config_id.clone(),
            model_config_revision: request.model_config_revision,
            input_tokens: 100,
        })
    }
}

#[async_trait]
impl ModelGatewayClient for ExecutingGateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("descriptor is frozen into the Run")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        self.requests.lock().unwrap().push(request);
        tokio::time::sleep(self.delay).await;
        let output_items = vec![serde_json::json!({
            "type": "message",
            "id": "provider-message-1",
            "role": "assistant",
            "content": []
        })];
        Ok(ModelGatewayOutput {
            content: if self.terminal_status == ModelGatewayTerminalStatus::Completed {
                "Finished".to_string()
            } else {
                "partial output".to_string()
            },
            reasoning: String::new(),
            output_items: output_items.clone(),
            terminal: ModelGatewayTerminal {
                status: self.terminal_status,
                source: ModelGatewayTerminalSource::Provider,
                response_id: Some("response-1".to_string()),
                provider_request_id: Some("provider-request-1".to_string()),
                terminal_event: match self.terminal_status {
                    ModelGatewayTerminalStatus::Completed => "response.completed",
                    ModelGatewayTerminalStatus::Incomplete => "response.incomplete",
                    ModelGatewayTerminalStatus::Failed => "response.failed",
                }
                .to_string(),
                provider_http_status: Some(200),
                usage: Some(serde_json::json!({
                    "input_tokens": 100,
                    "output_tokens": 10
                })),
                output_items,
                incomplete_details: (self.terminal_status
                    == ModelGatewayTerminalStatus::Incomplete)
                    .then(|| serde_json::json!({"reason": "max_output_tokens"})),
                provider_error: (self.terminal_status == ModelGatewayTerminalStatus::Failed)
                    .then(|| serde_json::json!({"code": "provider_failed"})),
            },
        })
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        Ok(ModelGatewayTokenCount {
            request_id: request.request_id.clone(),
            model_config_id: request.model_config_id.clone(),
            model_config_revision: request.model_config_revision,
            input_tokens: 100,
        })
    }
}

struct RetryProfile;

#[async_trait]
impl LocalAgentProfile for RetryProfile {
    fn profile_key(&self) -> &'static str {
        "main_chat"
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        Profile.prepare_model_step(run).await
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        _output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        Ok(ModelStepResult::Retry(
            serde_json::json!({"reason": "provider_busy"}),
        ))
    }
}

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

#[async_trait]
impl ModelGatewayClient for DescriptorGateway {
    async fn descriptor(
        &self,
        access_token: &str,
        model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        self.calls
            .lock()
            .unwrap()
            .push((access_token.to_string(), model_config_id.to_string()));
        let mut descriptor = run(Utc::now()).model_runtime_snapshot;
        descriptor.model_config_id = model_config_id.to_string();
        Ok(descriptor)
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        unreachable!("creation contract does not execute a model step")
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        unreachable!("creation contract does not count model input")
    }
}

struct Seed {
    now: chrono::DateTime<Utc>,
}

#[derive(Default)]
struct ReadCreationState {
    runs: Vec<LocalAgentRun>,
    tasks: Vec<TaskRecord>,
    message_count: usize,
    outbox_count: usize,
}

#[async_trait]
impl StorageTransaction for ReadCreationState {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = ListQuery {
            scope: scope(),
            cursor: None,
            limit: 100,
        };
        self.runs = repositories
            .agent_runs()
            .list(&query)
            .await?
            .records
            .into_iter()
            .map(|record| record.run)
            .collect();
        self.tasks = repositories.tasks().list(&query).await?.records;
        self.message_count = repositories
            .agent_messages()
            .list(&query)
            .await?
            .records
            .len();
        self.outbox_count = repositories.sync_outbox().list(&query).await?.records.len();
        Ok(())
    }
}

fn snapshot(id: &str, revision: &str, fill: char) -> FrozenSnapshotReference {
    FrozenSnapshotReference {
        snapshot_id: id.to_string(),
        revision: revision.to_string(),
        digest: format!("sha256:{}", fill.to_string().repeat(64)),
    }
}

struct SeedMemory {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedMemory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        Seed { now: self.now }.execute(repositories).await?;
        let query = chatos_client_storage::RecordQuery {
            scope: scope(),
            id: "run-1".to_string(),
        };
        let mut record = repositories
            .agent_runs()
            .get(&query)
            .await?
            .expect("seeded Run");
        let revision = record.metadata.revision;
        record.run.version = revision + 1;
        record.run.context_strategy = ContextStrategy::MemoryEngine;
        record.run.model_runtime_snapshot.context_strategy = ContextStrategy::MemoryEngine;
        record.run.model_runtime_snapshot.supports_native_compaction = false;
        record.run.model_runtime_snapshot.provider = "deepseek".to_string();
        repositories
            .agent_runs()
            .put(PutRecord {
                record,
                expected_revision: Some(revision),
            })
            .await?;
        let event_query = chatos_client_storage::RecordQuery {
            scope: scope(),
            id: "event-1".to_string(),
        };
        let mut event = repositories
            .agent_events()
            .get(&event_query)
            .await?
            .expect("seeded event");
        let event_revision = event.metadata.revision;
        event.event.expected_version = revision + 1;
        repositories
            .agent_events()
            .put(PutRecord {
                record: event,
                expected_revision: Some(event_revision),
            })
            .await?;
        Ok(())
    }
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
        Arc::new(TestContextRuntime),
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
    let completion = host
        .record_claimed_model_step_completion(
            &next,
            ModelStepCompletion {
                result: ModelStepResult::Final(serde_json::json!({"text": "done"})),
                pending_batch_id: None,
                retry_at: None,
            },
            Some(CompletedAssistantMessage {
                record_id: "assistant-1".to_string(),
                turn_id: "turn-1".to_string(),
                content: Some("Done".to_string()),
                reasoning: None,
                structured_payload: None,
                response_id: Some("response-1".to_string()),
                message_source: "main_chat".to_string(),
            }),
            None,
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        completion.event.event_type,
        LocalAgentEventType::ModelStepCompleted
    );
    let SchedulerTickResult::Claimed(scheduled_completion) =
        host.claim_next("claim-3", now).await.unwrap()
    else {
        panic!("durable model completion was not scheduled");
    };
    assert_eq!(
        scheduled_completion.event.event_id,
        completion.event.event_id
    );
}

#[tokio::test]
async fn host_executes_and_persists_one_claimed_model_step_end_to_end() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:model-execution-key").unwrap(),
            },
            &StorageEncryptionKey::new([27; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut Seed { now }).await.unwrap();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        terminal_status: ModelGatewayTerminalStatus::Completed,
    });
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        gateway.clone(),
        Arc::new(TestContextRuntime),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();

    let SchedulerTickResult::Claimed(started) = host.claim_next("claim-start", now).await.unwrap()
    else {
        panic!("run start was not claimed");
    };
    let session = LocalAgentExecutionSession::new(
        "access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
    )
    .unwrap();
    let ProcessedClaimedEvent::ReductionCommitted(_) = host
        .process_claimed_event(&started, &session, now)
        .await
        .unwrap()
    else {
        panic!("run start did not use the reducer path");
    };
    let SchedulerTickResult::Claimed(requested) =
        host.claim_next("claim-model", now).await.unwrap()
    else {
        panic!("model step was not claimed");
    };

    let ProcessedClaimedEvent::ModelCompletionScheduled(completion) = host
        .process_claimed_event(&requested, &session, now)
        .await
        .unwrap()
    else {
        panic!("model request did not use the model execution path");
    };
    assert_eq!(
        completion.event.event_type,
        LocalAgentEventType::ModelStepCompleted
    );
    {
        let requests = gateway.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].request_id, requested.event.event_id);
    }

    let SchedulerTickResult::Claimed(scheduled) = host
        .claim_next("claim-completion", Utc::now())
        .await
        .unwrap()
    else {
        panic!("model completion was not scheduled");
    };
    assert_eq!(scheduled.event.event_id, completion.event.event_id);
    let committed = host
        .commit_claimed_protocol_event(&scheduled, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::Succeeded
    );
}

#[tokio::test]
async fn cancelled_context_becomes_a_durable_cancelled_model_completion() {
    let now = Utc::now();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        terminal_status: ModelGatewayTerminalStatus::Completed,
    });
    let (_directory, host) = model_test_host(
        now,
        gateway.clone(),
        Arc::new(Profile),
        LocalAgentHostPolicy::default(),
    )
    .await;
    let requested = claim_model_request(&host, now).await;
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    host.execute_claimed_model_step(
        &requested,
        "access-token",
        ModelGatewayCallbacks::default(),
        cancellation,
        now,
    )
    .await
    .unwrap();
    assert!(gateway.requests.lock().unwrap().is_empty());
    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("claim-cancelled", Utc::now())
        .await
        .unwrap()
    else {
        panic!("cancelled completion was not scheduled");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::Cancelled
    );
}

#[tokio::test]
async fn incomplete_provider_terminal_fails_the_run_instead_of_becoming_an_empty_result() {
    let now = Utc::now();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        terminal_status: ModelGatewayTerminalStatus::Incomplete,
    });
    let (_directory, host) = model_test_host(
        now,
        gateway,
        Arc::new(Profile),
        LocalAgentHostPolicy::default(),
    )
    .await;
    let requested = claim_model_request(&host, now).await;

    host.execute_claimed_model_step(
        &requested,
        "access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
        now,
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("claim-incomplete", Utc::now())
        .await
        .unwrap()
    else {
        panic!("failed completion was not scheduled");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(committed.run_record.run.status, LocalAgentRunStatus::Failed);
    assert_eq!(
        committed.run_record.run.terminal_outcome,
        Some(serde_json::json!({
            "reason": "model_terminal_not_completed",
            "status": "incomplete",
            "terminal_event": "response.incomplete",
            "response_id": "response-1",
            "provider_request_id": "provider-request-1",
            "provider_http_status": 200
        }))
    );
}

#[tokio::test]
async fn unavailable_memory_context_pauses_the_run_for_an_explicit_resume() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:memory-block-key").unwrap(),
            },
            &StorageEncryptionKey::new([47; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut SeedMemory { now }).await.unwrap();
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        Arc::new(FailingMemoryContextRuntime),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let requested = claim_model_request(&host, now).await;

    host.execute_claimed_model_step(
        &requested,
        "access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
        now,
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("claim-memory-blocked", Utc::now())
        .await
        .unwrap()
    else {
        panic!("blocked completion was not scheduled");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(committed.run_record.run.status, LocalAgentRunStatus::Paused);
    assert_eq!(committed.run_record.run.retry_count, 0);
    assert_eq!(
        committed.run_record.run.pending_interaction,
        Some(serde_json::json!({
            "type": "runtime_blocked",
            "details": {
                "reason": "memory_sync_unavailable",
                "detail": "Memory Engine active summary is unavailable"
            }
        }))
    );
}

#[tokio::test]
async fn host_alone_assigns_the_retry_deadline() {
    let now = Utc::now();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        terminal_status: ModelGatewayTerminalStatus::Completed,
    });
    let policy = LocalAgentHostPolicy {
        model_retry_delay: chrono::Duration::seconds(30),
        ..LocalAgentHostPolicy::default()
    };
    let (_directory, host) = model_test_host(now, gateway, Arc::new(RetryProfile), policy).await;
    let execution_now = Utc::now();
    let requested = claim_model_request(&host, execution_now).await;

    host.execute_claimed_model_step(
        &requested,
        "access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
        execution_now,
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(completed) =
        host.claim_next("claim-retry", Utc::now()).await.unwrap()
    else {
        panic!("retry completion was not scheduled");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::RetryScheduled
    );
    assert_eq!(committed.emitted_events.len(), 1);
    assert_eq!(
        committed.emitted_events[0].event.event_type,
        LocalAgentEventType::RetryDue
    );
    assert!(committed.emitted_events[0].event.available_at > now);
}

#[tokio::test]
async fn transient_gateway_failure_becomes_a_durable_retry_instead_of_an_expired_claim() {
    let now = Utc::now();
    let gateway = Arc::new(RetryableGateway {
        requests: Mutex::new(Vec::new()),
    });
    let (_directory, host) = model_test_host(
        now,
        gateway.clone(),
        Arc::new(Profile),
        LocalAgentHostPolicy::default(),
    )
    .await;
    let requested = claim_model_request(&host, now).await;

    host.execute_claimed_model_step(
        &requested,
        "access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
        now,
    )
    .await
    .unwrap();
    assert_eq!(gateway.requests.lock().unwrap().len(), 1);
    let SchedulerTickResult::Claimed(completed) = host
        .claim_next("claim-gateway-retry", Utc::now())
        .await
        .unwrap()
    else {
        panic!("gateway retry completion was not scheduled");
    };
    let committed = host
        .commit_claimed_protocol_event(&completed, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::RetryScheduled
    );
    assert_eq!(committed.run_record.run.retry_count, 1);
    assert_eq!(
        committed.emitted_events[0].event.bounded_payload,
        serde_json::json!({
            "reason": "model_gateway_unavailable",
            "detail": "model gateway transport failed (connection_reset)"
        })
    );
}

#[tokio::test]
async fn long_model_execution_renews_its_claim_until_completion_is_durable() {
    let now = Utc::now();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::from_millis(140),
        terminal_status: ModelGatewayTerminalStatus::Completed,
    });
    let policy = LocalAgentHostPolicy {
        claim_ttl: chrono::Duration::milliseconds(90),
        ..LocalAgentHostPolicy::default()
    };
    let (_directory, host) = model_test_host(now, gateway, Arc::new(Profile), policy).await;
    let execution_now = Utc::now();
    let requested = claim_model_request(&host, execution_now).await;

    let completion = host
        .execute_claimed_model_step(
            &requested,
            "access-token",
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
            execution_now,
        )
        .await
        .unwrap();
    assert_eq!(
        completion.event.event_type,
        LocalAgentEventType::ModelStepCompleted
    );
}

async fn model_test_host(
    now: chrono::DateTime<Utc>,
    gateway: Arc<dyn ModelGatewayClient>,
    profile: Arc<dyn LocalAgentProfile>,
    policy: LocalAgentHostPolicy,
) -> (tempfile::TempDir, LocalAgentHost) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:model-test-host-key").unwrap(),
            },
            &StorageEncryptionKey::new([37; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut Seed { now }).await.unwrap();
    let profiles = LocalAgentProfileRegistry::new([profile]).unwrap();
    let (host, _) = LocalAgentHost::start(
        storage,
        gateway,
        Arc::new(TestContextRuntime),
        Arc::new(Tools),
        profiles,
        scope(),
        "device-1",
        policy,
        now,
    )
    .await
    .unwrap();
    (directory, host)
}

async fn claim_model_request(
    host: &LocalAgentHost,
    now: chrono::DateTime<Utc>,
) -> AgentEventStateRecord {
    let SchedulerTickResult::Claimed(started) =
        host.claim_next("claim-start-helper", now).await.unwrap()
    else {
        panic!("run start was not claimed");
    };
    host.commit_claimed(&started, StepEvidence::None, now)
        .await
        .unwrap();
    let SchedulerTickResult::Claimed(requested) =
        host.claim_next("claim-model-helper", now).await.unwrap()
    else {
        panic!("model request was not claimed");
    };
    *requested
}

#[tokio::test]
async fn typed_ipc_creates_main_chat_and_task_work_atomically_and_idempotently() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:create-ipc-key").unwrap(),
            },
            &StorageEncryptionKey::new([19; 32]),
        )
        .await
        .unwrap(),
    );
    let gateway = Arc::new(DescriptorGateway {
        calls: Mutex::new(Vec::new()),
    });
    let profiles = LocalAgentProfileRegistry::new([
        Arc::new(Profile) as Arc<dyn LocalAgentProfile>,
        Arc::new(TaskProfile) as Arc<dyn LocalAgentProfile>,
    ])
    .unwrap();
    let (host, _) = LocalAgentHost::start(
        storage.clone(),
        gateway.clone(),
        Arc::new(TestContextRuntime),
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
    let session = LocalAgentExecutionSession::new(
        "private-access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
    )
    .unwrap();
    let executor = Arc::new(LocalAgentHostCreationExecutor::new(
        host,
        session,
        Arc::new(UnusedMutationExecutor),
    ));
    let server = LocalAgentIpcServer::new(storage.clone(), scope(), executor).unwrap();

    let main_request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "ipc-main-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::CreateMainChatTurn(CreateMainChatTurnCommand {
            thread_id: "thread-created-1".to_string(),
            turn_id: "turn-created-1".to_string(),
            message_id: "message-created-1".to_string(),
            project_id: Some("project-1".to_string()),
            model_config_id: "model-main".to_string(),
            prompt_revision: "main-prompt-1".to_string(),
            capability_snapshot_ref: "main-capabilities-1".to_string(),
            content: Some("Review this visual and improve the page hierarchy".to_string()),
            attachments: Vec::new(),
        }),
    };
    let first_main = server.handle_request(main_request.clone()).await.response;
    let repeated_main = server.handle_request(main_request).await.response;
    assert_eq!(first_main, repeated_main);

    let task_request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "ipc-task-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::CreateTask(CreateTaskCommand {
            task_id: "task-created-1".to_string(),
            source_thread_id: "thread-created-1".to_string(),
            source_turn_id: "turn-created-1".to_string(),
            project_id: "project-1".to_string(),
            objective: "Implement the approved visual design".to_string(),
            acceptance_criteria: vec![
                "The rendered UI matches the approved reference".to_string(),
                "Relevant verification succeeds".to_string(),
            ],
            model_config_id: "model-task".to_string(),
            prompt_snapshot: snapshot("task-prompt-1", "task-prompt-revision-1", 'a'),
            project_snapshot: snapshot("project-snapshot-1", "project-revision-1", 'b'),
            capability_snapshot: snapshot("task-capabilities-1", "capability-revision-1", 'c'),
        }),
    };
    let first_task = server.handle_request(task_request.clone()).await.response;
    let repeated_task = server.handle_request(task_request).await.response;
    assert_eq!(first_task, repeated_task);
    assert!(matches!(first_main, LocalAgentIpcResponse::Accepted { .. }));
    assert!(matches!(first_task, LocalAgentIpcResponse::Accepted { .. }));

    {
        let descriptor_calls = gateway.calls.lock().unwrap();
        assert_eq!(descriptor_calls.len(), 2);
        assert!(descriptor_calls
            .iter()
            .all(|(token, _)| token == "private-access-token"));
    }

    let mut state = ReadCreationState::default();
    storage.transaction(&mut state).await.unwrap();
    assert_eq!(state.runs.len(), 2);
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.message_count, 2);
    assert_eq!(state.outbox_count, 2);
    let main = state
        .runs
        .iter()
        .find(|run| run.profile_key == "main_chat")
        .unwrap();
    assert_eq!(main.project_id.as_deref(), Some("project-1"));
    assert_eq!(main.model_config_id, "model-main");
    let task = state
        .runs
        .iter()
        .find(|run| run.profile_key == "task_runner")
        .unwrap();
    assert_eq!(task.owner_entity_id, "task-created-1");
    assert_eq!(task.project_id.as_deref(), Some("project-1"));
    assert_eq!(state.tasks[0].state["run_id"], task.run_id);
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
        Arc::new(TestContextRuntime),
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
        Arc::new(TestContextRuntime),
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
        Arc::new(TestContextRuntime),
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
        Arc::new(TestContextRuntime),
        tools.clone(),
        profiles,
        scope(),
        "device-1",
        policy,
        now,
    )
    .await
    .unwrap();
    let execution_now = Utc::now();
    let SchedulerTickResult::Claimed(claimed) = host
        .claim_next("tool-claim-1", execution_now)
        .await
        .unwrap()
    else {
        panic!("tool batch was not claimed");
    };
    let committed = host
        .execute_claimed_tool_batch(&claimed, CancellationToken::new(), execution_now)
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
        Arc::new(TestContextRuntime),
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
