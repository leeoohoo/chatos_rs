// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_agent_profiles::{
    MainChatAgentProfile, MainChatCapabilitySnapshot, MainChatContextProvider,
    MainChatProjectSnapshot, MainChatPromptSnapshot, MainChatStepContext,
    TaskRunnerCapabilitySnapshot, TaskRunnerContextProvider, TaskRunnerExecutionTool,
    TaskRunnerProjectSnapshot, TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, AgentUiEventCursorQuery, ClientStorage, ListQuery,
    PutRecord, RecordMetadata, RecordQuery, RecordScope, SecretReference, SqliteBootstrapProfile,
    SqliteClientStorage, StorageEncryptionKey, StorageResult, StorageTransaction, TaskRecord,
    ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentContextRuntime, LocalAgentContextRuntimeError, LocalAgentExecutionSession,
    LocalAgentHost, LocalAgentHostControlExecutor, LocalAgentHostCreationExecutor,
    LocalAgentHostPolicy, LocalAgentHostRunRequest, LocalAgentHostWorker,
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalAgentProfileRegistry,
    LocalAttachmentLocator, LocalAttachmentResolver, LocalTaskCreationPlan,
    LocalTaskCreationPlanner, LocalTaskPlanningRequest, ProcessedClaimedEvent,
    StoredTaskRunnerContextProvider,
};
use chatos_local_agent_protocol::{
    AnswerUserQuestionCommand, ContextStrategy, CreateMainChatTurnCommand, CreateTaskCommand,
    FrozenSnapshot, LocalAgentCommand, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, LocalAgentRun,
    LocalAgentRunStatus, LocalAgentUiEvent, LocalAgentUiEventPayload, ModelGatewayRequest,
    ModelGatewayTerminal, ModelGatewayTerminalSource, ModelGatewayTerminalStatus,
    ModelGatewayTokenCount, ModelProtocol, ModelRuntimeDescriptor, ModelStepCompletion,
    ModelStepResult, ModelStreamDeltaKind, RetryTaskCommand, ToolApprovalCommand,
    ToolApprovalDecision, ToolEffect, ToolExecutionStatus, UserInteractionAnswer,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{
    CompletedAssistantMessage, DurableProviderContextCommit, DurableTaskState, LocalAgentProfile,
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

struct TaskCreatingGateway {
    descriptor_calls: Mutex<Vec<(String, String)>>,
    model_requests: Mutex<Vec<ModelGatewayRequest>>,
}

struct Tools;

struct UnusedTaskPlanner;

struct NoAttachments;

#[async_trait]
impl LocalAttachmentResolver for NoAttachments {
    async fn resolve(&self, _attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String> {
        Err("test has no attachments".to_string())
    }
}

struct RecordingTaskPlanner {
    requests: Mutex<Vec<LocalTaskPlanningRequest>>,
    plan: LocalTaskCreationPlan,
}

struct MainChatTestContext;

fn execution_session() -> LocalAgentExecutionSession {
    LocalAgentExecutionSession::new(
        "test-access-token",
        ModelGatewayCallbacks::default(),
        CancellationToken::new(),
    )
    .unwrap()
}

#[async_trait]
impl LocalTaskCreationPlanner for UnusedTaskPlanner {
    async fn plan_task(
        &self,
        _request: &LocalTaskPlanningRequest,
        _cancellation: CancellationToken,
    ) -> Result<LocalTaskCreationPlan, String> {
        unreachable!("test does not create a Task from a Main Chat tool")
    }
}

#[async_trait]
impl LocalTaskCreationPlanner for RecordingTaskPlanner {
    async fn plan_task(
        &self,
        request: &LocalTaskPlanningRequest,
        _cancellation: CancellationToken,
    ) -> Result<LocalTaskCreationPlan, String> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(self.plan.clone())
    }
}

#[async_trait]
impl MainChatContextProvider for MainChatTestContext {
    async fn load_step_context(&self, run: &LocalAgentRun) -> Result<MainChatStepContext, String> {
        Ok(MainChatStepContext {
            prompt_snapshot: MainChatPromptSnapshot {
                prompt_revision: run.prompt_revision.clone(),
                base_system_prompt: "Design collaboratively.".to_string(),
                contact_system_prompt: None,
                skill_catalog_prompt: None,
            },
            capability_snapshot: MainChatCapabilitySnapshot {
                snapshot_ref: run.capability_snapshot_ref.clone(),
                allowed_tools: vec!["ask_user".to_string(), "create_local_task".to_string()],
            },
            project_snapshot: run
                .project_id
                .as_ref()
                .map(|project_id| MainChatProjectSnapshot {
                    project_id: project_id.clone(),
                    snapshot_revision: "project-revision-1".to_string(),
                    project_name: "Visual Project".to_string(),
                    design_context: serde_json::json!({"surface": "website"}),
                }),
            model_input_items: Vec::new(),
            maximum_output_tokens: 32_000,
            native_compaction_threshold: Some(300_000),
            memory_engine_active_threshold: None,
            maximum_summary_attempts: 0,
        })
    }
}

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
        callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        self.requests.lock().unwrap().push(request);
        tokio::time::sleep(self.delay).await;
        if let Some(callback) = callbacks.on_reasoning {
            callback("Checked the frozen design context.".to_string());
        }
        if let Some(callback) = callbacks.on_content {
            callback("Finished".to_string());
        }
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

struct ReadUiEvents(Vec<LocalAgentUiEvent>);

#[async_trait]
impl StorageTransaction for ReadUiEvents {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.0 = repositories
            .agent_ui_events()
            .list_after(&AgentUiEventCursorQuery {
                scope: scope(),
                after_seq: 0,
                limit: 100,
            })
            .await?
            .records
            .into_iter()
            .map(|record| record.event)
            .collect();
        Ok(())
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

#[async_trait]
impl ModelGatewayClient for TaskCreatingGateway {
    async fn descriptor(
        &self,
        access_token: &str,
        model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        self.descriptor_calls
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
        request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        self.model_requests.lock().unwrap().push(request);
        let output_items = vec![serde_json::json!({
            "type": "function_call",
            "call_id": "create-task-call-1",
            "name": "create_local_task",
            "arguments": serde_json::json!({
                "objective": "Implement the approved visual hierarchy",
                "acceptance_criteria": [
                    "The rendered page matches the approved visual reference",
                    "The focused verification suite passes"
                ]
            }).to_string()
        })];
        Ok(ModelGatewayOutput {
            content: String::new(),
            reasoning: String::new(),
            output_items: output_items.clone(),
            terminal: ModelGatewayTerminal {
                status: ModelGatewayTerminalStatus::Completed,
                source: ModelGatewayTerminalSource::Provider,
                response_id: Some("task-creation-response-1".to_string()),
                provider_request_id: Some("task-creation-provider-request-1".to_string()),
                terminal_event: "response.completed".to_string(),
                provider_http_status: Some(200),
                usage: None,
                output_items,
                incomplete_details: None,
                provider_error: None,
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

struct Seed {
    now: chrono::DateTime<Utc>,
}

#[derive(Default)]
struct ReadCreationState {
    runs: Vec<LocalAgentRun>,
    tasks: Vec<TaskRecord>,
    tool_executions: Vec<ToolExecutionStateRecord>,
    message_count: usize,
    outbox_count: usize,
}

struct MarkAgentRunFailed {
    run_id: String,
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for MarkAgentRunFailed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = RecordQuery {
            scope: scope(),
            id: self.run_id.clone(),
        };
        let mut record = repositories.agent_runs().get(&query).await?.unwrap();
        let revision = record.metadata.revision;
        record.run.status = LocalAgentRunStatus::Failed;
        record.run.version += 1;
        record.run.terminal_outcome = Some(serde_json::json!({"reason": "fixture failure"}));
        record.run.updated_at = self.now;
        record.metadata.updated_at = self.now;
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
        self.tool_executions = repositories.tool_executions().list(&query).await?.records;
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

fn snapshot(id: &str, revision: &str, fill: char) -> FrozenSnapshot {
    FrozenSnapshot::new(
        id,
        revision,
        serde_json::json!({"fixture": fill.to_string()}),
    )
    .unwrap()
}

fn main_chat_snapshots(project_id: &str) -> (FrozenSnapshot, FrozenSnapshot, FrozenSnapshot) {
    let prompt = FrozenSnapshot::new(
        "main-prompt-snapshot-1",
        "main-prompt-1",
        serde_json::to_value(MainChatPromptSnapshot {
            prompt_revision: "main-prompt-1".to_string(),
            base_system_prompt: "Design collaboratively.".to_string(),
            contact_system_prompt: None,
            skill_catalog_prompt: Some("Use visual design skills step by step.".to_string()),
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
            project_id: project_id.to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            project_name: "Visual Project".to_string(),
            design_context: serde_json::json!({"surface": "website"}),
        })
        .unwrap(),
    )
    .unwrap();
    (prompt, capability, project)
}

fn task_runner_snapshots() -> (FrozenSnapshot, FrozenSnapshot, FrozenSnapshot) {
    let prompt = FrozenSnapshot::new(
        "task-prompt-1",
        "task-prompt-revision-1",
        serde_json::to_value(TaskRunnerPromptSnapshot {
            prompt_revision: "task-prompt-revision-1".to_string(),
            base_system_prompt: "Execute the local task safely.".to_string(),
            task_prompt: "Preserve the approved visual design.".to_string(),
            skill_snapshot: serde_json::json!({"skills": []}),
        })
        .unwrap(),
    )
    .unwrap();
    let project = FrozenSnapshot::new(
        "project-snapshot-1",
        "project-revision-1",
        serde_json::to_value(TaskRunnerProjectSnapshot {
            project_id: "project-visual-1".to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            working_directory_ref: "workspace-grant-1".to_string(),
            authority_snapshot: serde_json::json!({"device_id": "device-1"}),
        })
        .unwrap(),
    )
    .unwrap();
    let capability = FrozenSnapshot::new(
        "task-capabilities-1",
        "task-capabilities-revision-1",
        serde_json::to_value(TaskRunnerCapabilitySnapshot {
            snapshot_ref: "task-capabilities-1".to_string(),
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
            .tasks()
            .put(PutRecord {
                record: TaskRecord {
                    metadata: metadata("task-1"),
                    conversation_id: Some("thread-1".to_string()),
                    status: "running".to_string(),
                    state: DurableTaskState::new(
                        "run-1".to_string(),
                        "thread-1".to_string(),
                        "turn-1".to_string(),
                        "project-1".to_string(),
                        "Exercise the tool batch".to_string(),
                        vec!["The tool receipt is durable".to_string()],
                        "model-1".to_string(),
                        1,
                        snapshot("prompt-snapshot-1", "prompt-1", 'p'),
                        snapshot("project-snapshot-1", "project-1", 'j'),
                        snapshot("capabilities-1", "capabilities-1", 'c'),
                    )
                    .unwrap()
                    .to_value()
                    .unwrap(),
                },
                expected_revision: None,
            })
            .await?;
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
        Arc::new(UnusedTaskPlanner),
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
        storage.clone(),
        gateway.clone(),
        Arc::new(TestContextRuntime),
        Arc::new(Tools),
        Arc::new(UnusedTaskPlanner),
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
    let mut ui_events = ReadUiEvents(Vec::new());
    storage.transaction(&mut ui_events).await.unwrap();
    let model_events = ui_events
        .0
        .iter()
        .filter_map(|event| match &event.event {
            LocalAgentUiEventPayload::ModelStream(event) => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(model_events.len(), 2);
    assert_eq!(model_events[0].run_id, "run-1");
    assert_eq!(model_events[0].step_seq, 1);
    assert_eq!(model_events[0].delta_kind, ModelStreamDeltaKind::Reasoning);
    assert_eq!(model_events[0].delta, "Checked the frozen design context.");
    assert_eq!(model_events[1].run_id, "run-1");
    assert_eq!(model_events[1].step_seq, 1);
    assert_eq!(model_events[1].delta_kind, ModelStreamDeltaKind::Content);
    assert_eq!(model_events[1].delta, "Finished");
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
        Arc::new(UnusedTaskPlanner),
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
        Arc::new(UnusedTaskPlanner),
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
        Arc::new(UnusedTaskPlanner),
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

    let (main_prompt_snapshot, main_capability_snapshot, main_project_snapshot) =
        main_chat_snapshots("project-1");
    let main_request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "ipc-main-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::CreateMainChatTurn(Box::new(CreateMainChatTurnCommand {
            thread_id: "thread-created-1".to_string(),
            turn_id: "turn-created-1".to_string(),
            message_id: "message-created-1".to_string(),
            project_id: Some("project-1".to_string()),
            model_config_id: "model-main".to_string(),
            prompt_snapshot: main_prompt_snapshot,
            capability_snapshot: main_capability_snapshot,
            project_snapshot: Some(main_project_snapshot),
            content: Some("Review this visual and improve the page hierarchy".to_string()),
            attachments: Vec::new(),
        })),
    };
    let first_main = server.handle_request(main_request.clone()).await.response;
    let repeated_main = server.handle_request(main_request).await.response;
    assert_eq!(first_main, repeated_main);

    let task_request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "ipc-task-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::CreateTask(Box::new(CreateTaskCommand {
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
        })),
    };
    let first_task = server.handle_request(task_request.clone()).await.response;
    let repeated_task = server.handle_request(task_request).await.response;
    assert_eq!(first_task, repeated_task);
    let LocalAgentIpcResponse::RunCreated { run, .. } = first_main else {
        panic!("Main Chat creation must return its durable Run");
    };
    assert_eq!(run.profile_key, "main_chat");
    assert_eq!(run.owner_entity_id, "thread-created-1");
    let LocalAgentIpcResponse::RunCreated { run, .. } = first_task else {
        panic!("Task creation must return its durable Run");
    };
    assert_eq!(run.profile_key, "task_runner");
    assert_eq!(run.owner_entity_id, "task-created-1");

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
    let original_task_run_id = task.run_id.clone();
    assert_eq!(task.owner_entity_id, "task-created-1");
    assert_eq!(task.project_id.as_deref(), Some("project-1"));
    assert_eq!(state.tasks[0].state["current_run_id"], task.run_id);
    assert_eq!(
        state.tasks[0].state["run_ids"],
        serde_json::json!([task.run_id])
    );

    storage
        .transaction(&mut MarkAgentRunFailed {
            run_id: original_task_run_id.clone(),
            now: now + chrono::Duration::seconds(1),
        })
        .await
        .unwrap();
    let retry_request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "ipc-task-retry-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::RetryTask(RetryTaskCommand {
            task_id: "task-created-1".to_string(),
            expected_run_id: original_task_run_id.clone(),
            instruction: Some("Preserve the approved visual spacing.".to_string()),
        }),
    };
    let first_retry = server.handle_request(retry_request.clone()).await.response;
    let repeated_retry = server.handle_request(retry_request).await.response;
    assert_eq!(first_retry, repeated_retry);
    let LocalAgentIpcResponse::RunCreated { run: retried, .. } = first_retry else {
        panic!("Task retry must return the newly created durable Run");
    };
    assert_ne!(retried.run_id, original_task_run_id);
    assert_eq!(retried.project_id.as_deref(), Some("project-1"));
    assert_eq!(retried.model_runtime_snapshot, task.model_runtime_snapshot);

    let mut retried_state = ReadCreationState::default();
    storage.transaction(&mut retried_state).await.unwrap();
    assert_eq!(retried_state.runs.len(), 3);
    assert_eq!(retried_state.message_count, 3);
    assert_eq!(retried_state.outbox_count, 3);
    assert_eq!(
        retried_state.tasks[0].state["current_run_id"],
        retried.run_id
    );
    assert_eq!(
        retried_state.tasks[0].state["run_ids"],
        serde_json::json!([original_task_run_id, retried.run_id])
    );
}

#[tokio::test]
async fn main_chat_model_tool_creates_one_frozen_local_task_end_to_end() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:main-chat-task-tool-key").unwrap(),
            },
            &StorageEncryptionKey::new([52; 32]),
        )
        .await
        .unwrap(),
    );
    let gateway = Arc::new(TaskCreatingGateway {
        descriptor_calls: Mutex::new(Vec::new()),
        model_requests: Mutex::new(Vec::new()),
    });
    let (prompt_snapshot, project_snapshot, capability_snapshot) = task_runner_snapshots();
    let planner = Arc::new(RecordingTaskPlanner {
        requests: Mutex::new(Vec::new()),
        plan: LocalTaskCreationPlan {
            project_id: "project-visual-1".to_string(),
            model_config_id: "model-task-1".to_string(),
            prompt_snapshot,
            project_snapshot,
            capability_snapshot,
        },
    });
    let profiles = LocalAgentProfileRegistry::new([
        Arc::new(MainChatAgentProfile::new(Arc::new(MainChatTestContext)))
            as Arc<dyn LocalAgentProfile>,
        Arc::new(TaskProfile) as Arc<dyn LocalAgentProfile>,
    ])
    .unwrap();
    let (host, _) = LocalAgentHost::start(
        storage.clone(),
        gateway.clone(),
        Arc::new(TestContextRuntime),
        Arc::new(Tools),
        planner.clone(),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let session = execution_session();
    let (main_prompt_snapshot, main_capability_snapshot, main_project_snapshot) =
        main_chat_snapshots("project-visual-1");
    let created_main = host
        .create_main_chat_turn(
            "create-main-request-1",
            CreateMainChatTurnCommand {
                thread_id: "thread-ai-task-1".to_string(),
                turn_id: "turn-ai-task-1".to_string(),
                message_id: "message-ai-task-1".to_string(),
                project_id: Some("project-visual-1".to_string()),
                model_config_id: "model-main-1".to_string(),
                prompt_snapshot: main_prompt_snapshot,
                capability_snapshot: main_capability_snapshot,
                project_snapshot: Some(main_project_snapshot),
                content: Some("Implement the approved visual direction".to_string()),
                attachments: Vec::new(),
            },
            &session,
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        created_main.start_event.event.correlation_id,
        "turn-ai-task-1"
    );

    let SchedulerTickResult::Claimed(started) =
        host.claim_next("claim-main-start", now).await.unwrap()
    else {
        panic!("Main Chat start event was not scheduled");
    };
    host.process_claimed_event(&started, &session, now)
        .await
        .unwrap();
    let SchedulerTickResult::Claimed(model_requested) =
        host.claim_next("claim-main-model", now).await.unwrap()
    else {
        panic!("Main Chat model step was not scheduled");
    };
    host.process_claimed_event(&model_requested, &session, now)
        .await
        .unwrap();
    let SchedulerTickResult::Claimed(model_completed) = host
        .claim_next("claim-main-completion", Utc::now())
        .await
        .unwrap()
    else {
        panic!("Main Chat model completion was not scheduled");
    };
    let committed_model = host
        .process_claimed_event(&model_completed, &session, Utc::now())
        .await
        .unwrap();
    let ProcessedClaimedEvent::ReductionCommitted(committed_model) = committed_model else {
        panic!("model completion did not use the reducer path");
    };
    assert_eq!(
        committed_model.run_record.run.status,
        LocalAgentRunStatus::WaitingToolResult
    );

    let SchedulerTickResult::Claimed(tool_requested) = host
        .claim_next("claim-create-task-tool", Utc::now())
        .await
        .unwrap()
    else {
        panic!("create_local_task tool batch was not scheduled");
    };
    assert_eq!(
        tool_requested.event.event_type,
        LocalAgentEventType::ToolBatchRequested
    );
    let pending = host
        .process_claimed_event(&tool_requested, &session, Utc::now())
        .await
        .unwrap_or_else(|error| panic!("create_local_task failed: {error}"));
    assert!(matches!(
        pending,
        ProcessedClaimedEvent::ToolApprovalPending { .. }
    ));
    assert!(planner.requests.lock().unwrap().is_empty());
    let mut approval_state = ReadCreationState::default();
    storage.transaction(&mut approval_state).await.unwrap();
    let invocation_id = approval_state.tool_executions[0]
        .execution
        .invocation_id
        .clone();
    host.decide_tool_approval(
        ToolApprovalCommand {
            run_id: created_main.run_record.run.run_id.clone(),
            invocation_id,
            decision: ToolApprovalDecision::Approve,
            reason: Some("create the reviewed local task".to_string()),
        },
        Utc::now(),
    )
    .await
    .unwrap();
    let SchedulerTickResult::Claimed(approved_tool) = host
        .claim_next("claim-approved-create-task-tool", Utc::now())
        .await
        .unwrap()
    else {
        panic!("approved create_local_task batch was not resumed");
    };
    host.process_claimed_event(&approved_tool, &session, Utc::now())
        .await
        .unwrap_or_else(|error| panic!("approved create_local_task failed: {error}"));

    {
        let requests = planner.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].source_thread_id, "thread-ai-task-1");
        assert_eq!(requests[0].source_turn_id, "turn-ai-task-1");
        assert_eq!(requests[0].project_id, "project-visual-1");
        assert_eq!(
            requests[0].parent_capability_snapshot_ref,
            "main-capabilities-1"
        );
    }

    let mut state = ReadCreationState::default();
    storage.transaction(&mut state).await.unwrap();
    assert_eq!(state.runs.len(), 2);
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.tool_executions.len(), 1);
    assert_eq!(state.message_count, 4);
    assert_eq!(state.outbox_count, 4);
    let task = &state.tasks[0];
    assert_eq!(task.conversation_id.as_deref(), Some("thread-ai-task-1"));
    assert_eq!(task.state["source_turn_id"], "turn-ai-task-1");
    assert_eq!(task.state["project_id"], "project-visual-1");
    let execution = &state.tool_executions[0].execution;
    assert_eq!(execution.effect, ToolEffect::IdempotentWrite);
    assert_eq!(execution.status, ToolExecutionStatus::Succeeded);
    assert_eq!(
        execution.bounded_result.as_ref().unwrap()["task_id"],
        task.metadata.id
    );
    assert_eq!(gateway.model_requests.lock().unwrap().len(), 1);
    assert_eq!(gateway.descriptor_calls.lock().unwrap().len(), 2);

    let task_run = state
        .runs
        .iter()
        .find(|run| run.profile_key == "task_runner")
        .unwrap()
        .clone();
    let task_context =
        StoredTaskRunnerContextProvider::new(storage, scope(), Arc::new(NoAttachments))
            .load_step_context(&task_run)
            .await
            .unwrap();
    assert_eq!(task_context.project_snapshot.project_id, "project-visual-1");
    assert_eq!(
        task_context.prompt_snapshot.base_system_prompt,
        "Execute the local task safely."
    );
    assert_eq!(task_context.capability_snapshot.execution_tools.len(), 1);
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
        Arc::new(UnusedTaskPlanner),
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
        Arc::new(UnusedTaskPlanner),
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
        initial_attachments: Vec::new(),
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
async fn one_background_worker_wakes_for_new_work_and_completes_the_run() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:worker-key").unwrap(),
            },
            &StorageEncryptionKey::new([19; 32]),
        )
        .await
        .unwrap(),
    );
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let gateway = Arc::new(ExecutingGateway {
        requests: Mutex::new(Vec::new()),
        delay: std::time::Duration::ZERO,
        terminal_status: ModelGatewayTerminalStatus::Completed,
    });
    let (host, _) = LocalAgentHost::start(
        storage.clone(),
        gateway.clone(),
        Arc::new(TestContextRuntime),
        Arc::new(Tools),
        Arc::new(UnusedTaskPlanner),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let host = Arc::new(host);
    let worker =
        Arc::new(LocalAgentHostWorker::new(host.clone(), execution_session(), "worker-1").unwrap());
    let shutdown = CancellationToken::new();
    let worker_task = {
        let worker = worker.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move { worker.run(shutdown).await })
    };
    // The worker is already idle here. Creating a Run must wake it without a
    // polling timer or a second Profile-specific loop.
    host.create_run(
        LocalAgentHostRunRequest {
            run_id: "worker-run-1".to_string(),
            profile_key: "main_chat".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: "worker-thread-1".to_string(),
            project_id: None,
            model_runtime_snapshot: run(now).model_runtime_snapshot,
            prompt_revision: "prompt-1".to_string(),
            capability_snapshot_ref: "capabilities-1".to_string(),
            causation_id: "worker-turn-1".to_string(),
            deadline_at: None,
            initial_message: None,
            initial_attachments: Vec::new(),
        },
        now,
    )
    .await
    .unwrap();
    let ipc = LocalAgentIpcServer::new(storage, scope(), Arc::new(UnusedMutationExecutor)).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let reply = ipc
                .handle_request(LocalAgentIpcRequest {
                    protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                    request_id: "observe-worker-run".to_string(),
                    owner_user_id: "user-1".to_string(),
                    command: LocalAgentCommand::GetRun {
                        run_id: "worker-run-1".to_string(),
                    },
                })
                .await;
            if matches!(
                reply.response,
                LocalAgentIpcResponse::Run(run) if run.status == LocalAgentRunStatus::Succeeded
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background worker did not complete the Run");
    shutdown.cancel();
    let report = worker_task.await.unwrap().unwrap();
    assert!(report.processed_event_count >= 4);
    assert_eq!(gateway.requests.lock().unwrap().len(), 1);
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
        Arc::new(UnusedTaskPlanner),
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
        Arc::new(UnusedTaskPlanner),
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
        .execute_claimed_tool_batch(&claimed, &execution_session(), execution_now)
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
        storage.clone(),
        Arc::new(Gateway),
        Arc::new(TestContextRuntime),
        tools,
        Arc::new(UnusedTaskPlanner),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();
    let host = Arc::new(host);
    let SchedulerTickResult::Claimed(claimed) = host.claim_next("tool-claim-1", now).await.unwrap()
    else {
        panic!("tool batch was not claimed");
    };
    let pending = host
        .process_claimed_event(&claimed, &execution_session(), now)
        .await
        .unwrap();
    assert!(matches!(
        pending,
        ProcessedClaimedEvent::ToolApprovalPending { .. }
    ));
    let mut state = ReadCreationState::default();
    storage.transaction(&mut state).await.unwrap();
    let control =
        LocalAgentHostControlExecutor::new(host.clone(), Arc::new(UnusedMutationExecutor));
    let approval = control
        .execute_mutation(
            "approval-request-1",
            LocalAgentCommand::DecideToolApproval(ToolApprovalCommand {
                run_id: "run-1".to_string(),
                invocation_id: state.tool_executions[0].execution.invocation_id.clone(),
                decision: ToolApprovalDecision::Approve,
                reason: Some("exercise unknown outcome recovery".to_string()),
            }),
        )
        .await
        .unwrap();
    assert!(matches!(approval, LocalAgentIpcResponse::Accepted { .. }));
    let SchedulerTickResult::Claimed(approved) = host
        .claim_next("tool-approved-claim-1", Utc::now())
        .await
        .unwrap()
    else {
        panic!("approved tool batch was not resumed");
    };
    host.execute_claimed_tool_batch(&approved, &execution_session(), Utc::now())
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
