// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{future::Future, sync::Arc};

use async_trait::async_trait;
use chatos_agent_profiles::{
    MainChatCapabilitySnapshot, MainChatProjectSnapshot, MainChatPromptSnapshot,
};
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TaskRecord, ToolExecutionStateRecord,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    CreateMainChatTurnCommand, CreateTaskCommand, FrozenSnapshot, LocalAgentCommand,
    LocalAgentEventType, LocalAgentIpcError, LocalAgentIpcResponse, ModelRuntimeDescriptor,
    ModelStepCompletion, ModelStepResult, ProtocolError, ToolApprovalCommand, ToolEffect,
};
use chatos_local_agent_runtime::{
    answer_run_interaction, begin_model_step_execution, begin_tool_execution,
    build_local_tool_invocation, complete_tool_execution, create_local_agent_run,
    create_local_agent_task, decide_tool_approval, defer_tool_batch_for_approval,
    inspect_tool_batch, mark_tool_outcome_unknown, prepare_model_step_persistence,
    prepare_tool_batch, record_model_step_completion, reduce_and_commit, renew_event_claim,
    request_run_control, scan_recoverable_work, validate_local_tool_outcome, AnswerRunInteraction,
    BeganModelStepExecution, BeginModelStepExecutionRequest, BeginToolExecutionRequest,
    BeginToolExecutionResult, CommittedReduction, CompleteToolExecutionRequest,
    CompletedAssistantMessage, CreateLocalAgentRunRequest, CreateLocalAgentTaskRequest,
    CreatedLocalAgentRun, CreatedLocalAgentTask, DecideToolApprovalRequest,
    DeferToolBatchForApprovalRequest, DurableModelStepCompletionPayload,
    DurableProviderContextCommit, DurableScheduler, ExecutedModelStep, InitialRunMessage,
    LocalToolInvocation, LocalToolOutcome, LocalToolRuntime, MarkToolOutcomeUnknownRequest,
    ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError, ModelGatewayStreamError,
    ModelInputTokenGuardError, ModelStepExecutorError, ModelStepPersistenceError,
    PrepareToolBatchRequest, RecordModelStepCompletionRequest, RecoveryIssue,
    ReduceAndCommitRequest, ReducerPolicy, RenewEventClaimRequest, RequestRunControl,
    RunControlAction, SchedulerTickRequest, SchedulerTickResult, SingleModelStepExecutor,
    StepEvidence, ToolApprovalDeferralResult,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    LocalAgentContextRuntime, LocalAgentContextRuntimeError, LocalAgentIpcMutationExecutor,
    LocalAgentProfileRegistry, ProfileRegistryError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgentHostPolicy {
    pub claim_ttl: Duration,
    pub maximum_event_attempts: u32,
    pub model_retry_delay: Duration,
    pub maximum_model_retry_delay: Duration,
    pub reducer: ReducerPolicy,
}

impl Default for LocalAgentHostPolicy {
    fn default() -> Self {
        Self {
            claim_ttl: Duration::seconds(90),
            maximum_event_attempts: 3,
            model_retry_delay: Duration::seconds(5),
            maximum_model_retry_delay: Duration::minutes(5),
            reducer: ReducerPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentHostStartupReport {
    pub active_run_count: usize,
    pub ready_event_count: usize,
    pub next_wake_at: Option<DateTime<Utc>>,
    pub recovery_issues: Vec<RecoveryIssue>,
}

#[derive(Debug, Clone)]
pub struct LocalAgentHostRunRequest {
    pub run_id: String,
    pub profile_key: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub project_id: Option<String>,
    pub model_runtime_snapshot: ModelRuntimeDescriptor,
    pub prompt_revision: String,
    pub capability_snapshot_ref: String,
    pub causation_id: String,
    pub deadline_at: Option<DateTime<Utc>>,
    pub initial_message: Option<InitialRunMessage>,
    pub initial_attachments: Vec<chatos_local_agent_protocol::LocalAttachmentReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTaskPlanningRequest {
    pub task_id: String,
    pub parent_run_id: String,
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub project_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub parent_capability_snapshot_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTaskCreationPlan {
    pub project_id: String,
    pub model_config_id: String,
    pub prompt_snapshot: FrozenSnapshot,
    pub project_snapshot: FrozenSnapshot,
    pub capability_snapshot: FrozenSnapshot,
}

#[async_trait]
pub trait LocalTaskCreationPlanner: Send + Sync {
    async fn plan_task(
        &self,
        request: &LocalTaskPlanningRequest,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskCreationPlan, String>;
}

#[derive(Clone)]
pub struct LocalAgentExecutionSession {
    access_token: Arc<Zeroizing<String>>,
    callbacks: ModelGatewayCallbacks,
    cancellation: CancellationToken,
}

impl LocalAgentExecutionSession {
    pub fn new(
        access_token: impl Into<String>,
        callbacks: ModelGatewayCallbacks,
        cancellation: CancellationToken,
    ) -> Result<Self, LocalAgentHostError> {
        let access_token = access_token.into();
        if access_token.trim().is_empty() || access_token.trim() != access_token {
            return Err(LocalAgentHostError::InvalidModelAccessToken);
        }
        Ok(Self {
            access_token: Arc::new(Zeroizing::new(access_token)),
            callbacks,
            cancellation,
        })
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessedClaimedEvent {
    ModelCompletionScheduled(Box<AgentEventStateRecord>),
    ReductionCommitted(Box<CommittedReduction>),
    ToolApprovalPending { event_id: String, run_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostError {
    #[error("invalid local Agent Host configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("invalid Main Chat frozen context: {0}")]
    InvalidMainChatContext(String),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Profile(#[from] ProfileRegistryError),
    #[error("event {actual} does not match the claimed event {expected}")]
    ClaimedEventMismatch { expected: String, actual: String },
    #[error("event {0} is not a claimed local tool batch")]
    NotToolBatch(String),
    #[error("tool batch event {0} is waiting for durable approval")]
    ToolApprovalPending(String),
    #[error("event {0} requires the dedicated model step executor")]
    ModelStepExecutorRequired(String),
    #[error("event {0} is not a claimed model step request")]
    NotModelStep(String),
    #[error("local tool runtime failed after invocation start: {0}")]
    ToolRuntime(String),
    #[error("local tool runtime returned an invalid outcome: {0}")]
    InvalidToolOutcome(String),
    #[error("local tool batch still has uncompleted invocations")]
    IncompleteToolBatch,
    #[error("claimed event payload is invalid: {0}")]
    InvalidEventPayload(String),
    #[error("model access token must not be empty")]
    InvalidModelAccessToken,
    #[error("local Agent creation identity conflicts with existing run {0}")]
    CreationConflict(String),
    #[error(transparent)]
    ModelGateway(#[from] ModelGatewayClientError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    ContextRuntime(#[from] LocalAgentContextRuntimeError),
    #[error(transparent)]
    ModelStepExecutor(#[from] ModelStepExecutorError),
    #[error(transparent)]
    ModelStepPersistence(#[from] ModelStepPersistenceError),
    #[error("model retry deadline overflowed")]
    ModelRetryDeadlineOverflow,
    #[error("model event claim renewal failed: {0}")]
    ModelClaimRenewal(StorageError),
}

pub struct LocalAgentHost {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    policy: LocalAgentHostPolicy,
    scheduler: Mutex<DurableScheduler>,
    scheduler_wake: Notify,
    profiles: LocalAgentProfileRegistry,
    gateway: Arc<dyn ModelGatewayClient>,
    model_steps: SingleModelStepExecutor,
    context_runtime: Arc<dyn LocalAgentContextRuntime>,
    tool_runtime: Arc<dyn LocalToolRuntime>,
    task_planner: Arc<dyn LocalTaskCreationPlanner>,
}

impl LocalAgentHost {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        storage: Arc<dyn ClientStorage>,
        gateway: Arc<dyn ModelGatewayClient>,
        context_runtime: Arc<dyn LocalAgentContextRuntime>,
        tool_runtime: Arc<dyn LocalToolRuntime>,
        task_planner: Arc<dyn LocalTaskCreationPlanner>,
        profiles: LocalAgentProfileRegistry,
        scope: RecordScope,
        device_id: impl Into<String>,
        policy: LocalAgentHostPolicy,
        now: DateTime<Utc>,
    ) -> Result<(Self, LocalAgentHostStartupReport), LocalAgentHostError> {
        let device_id = device_id.into();
        if scope.owner_user_id.trim().is_empty() || device_id.trim().is_empty() {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "owner user and device identifiers are required",
            ));
        }
        if profiles.is_empty() {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "at least one Agent profile must be registered",
            ));
        }
        if policy.claim_ttl <= Duration::zero()
            || policy.maximum_event_attempts == 0
            || policy.model_retry_delay <= Duration::zero()
            || policy.maximum_model_retry_delay < policy.model_retry_delay
        {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "claim TTL, event attempt limit, and model retry delay bounds are invalid",
            ));
        }
        let plan = scan_recoverable_work(storage.as_ref(), scope.clone(), now).await?;
        let report = LocalAgentHostStartupReport {
            active_run_count: plan.active_runs.len(),
            ready_event_count: plan.ready_events.len(),
            next_wake_at: plan.next_wake_at,
            recovery_issues: plan.issues.clone(),
        };
        Ok((
            Self {
                storage,
                scope: scope.clone(),
                device_id,
                policy,
                scheduler: Mutex::new(DurableScheduler::from_recovery(scope, &plan)),
                scheduler_wake: Notify::new(),
                profiles,
                gateway: gateway.clone(),
                model_steps: SingleModelStepExecutor::new(gateway),
                context_runtime,
                tool_runtime,
                task_planner,
            },
            report,
        ))
    }

    pub fn profiles(&self) -> &LocalAgentProfileRegistry {
        &self.profiles
    }

    pub fn model_steps(&self) -> &SingleModelStepExecutor {
        &self.model_steps
    }

    pub async fn claim_next(
        &self,
        claim_token: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<SchedulerTickResult, LocalAgentHostError> {
        Ok(self
            .scheduler
            .lock()
            .await
            .tick(
                self.storage.as_ref(),
                SchedulerTickRequest {
                    device_id: self.device_id.clone(),
                    claim_token: claim_token.into(),
                    now,
                    claim_ttl: self.policy.claim_ttl,
                    max_attempts: self.policy.maximum_event_attempts,
                },
            )
            .await?)
    }

    pub async fn create_run(
        &self,
        request: LocalAgentHostRunRequest,
        now: DateTime<Utc>,
    ) -> Result<CreatedLocalAgentRun, LocalAgentHostError> {
        self.profiles.require(&request.profile_key)?;
        let created = create_local_agent_run(
            self.storage.as_ref(),
            CreateLocalAgentRunRequest {
                scope: self.scope.clone(),
                run_id: request.run_id,
                profile_key: request.profile_key,
                owner_entity_type: request.owner_entity_type,
                owner_entity_id: request.owner_entity_id,
                project_id: request.project_id,
                model_runtime_snapshot: request.model_runtime_snapshot,
                prompt_revision: request.prompt_revision,
                capability_snapshot_ref: request.capability_snapshot_ref,
                origin_device_id: self.device_id.clone(),
                causation_id: request.causation_id,
                deadline_at: request.deadline_at,
                initial_message: request.initial_message,
                initial_attachments: request.initial_attachments,
                now,
            },
        )
        .await?;
        self.scheduler.lock().await.schedule(&created.start_event);
        self.scheduler_wake.notify_one();
        Ok(created)
    }

    pub async fn create_main_chat_turn(
        &self,
        request_id: &str,
        command: CreateMainChatTurnCommand,
        session: &LocalAgentExecutionSession,
        now: DateTime<Utc>,
    ) -> Result<CreatedLocalAgentRun, LocalAgentHostError> {
        command.validate()?;
        const PROFILE_KEY: &str = "main_chat";
        self.profiles.require(PROFILE_KEY)?;
        let run_id = stable_host_id(
            "main-chat-run",
            &[
                self.scope.owner_user_id.as_str(),
                command.thread_id.as_str(),
                command.turn_id.as_str(),
            ],
        );
        let descriptor = self
            .descriptor_for_creation(&run_id, &command.model_config_id, session)
            .await?;
        MainChatPromptSnapshot::from_frozen(&command.prompt_snapshot)
            .map_err(LocalAgentHostError::InvalidMainChatContext)?;
        MainChatCapabilitySnapshot::from_frozen(&command.capability_snapshot)
            .map_err(LocalAgentHostError::InvalidMainChatContext)?;
        command
            .project_snapshot
            .as_ref()
            .map(|snapshot| {
                let project = MainChatProjectSnapshot::from_frozen(snapshot)
                    .map_err(LocalAgentHostError::InvalidMainChatContext)?;
                if Some(project.project_id.as_str()) != command.project_id.as_deref() {
                    return Err(LocalAgentHostError::InvalidMainChatContext(
                        "project snapshot does not match the frozen project_id".to_string(),
                    ));
                }
                Ok(project)
            })
            .transpose()?;
        let attachment_manifest = command
            .attachments
            .iter()
            .map(|attachment| {
                json!({
                    "attachment_id": attachment.attachment_id,
                    "media_type": attachment.media_type,
                    "payload_digest": attachment.payload_digest,
                    "byte_size": attachment.byte_size,
                })
            })
            .collect::<Vec<_>>();
        let structured_payload = Some(json!({
            "type": "main_chat_turn",
            "prompt_snapshot": &command.prompt_snapshot,
            "capability_snapshot": &command.capability_snapshot,
            "project_snapshot": &command.project_snapshot,
            "attachments": attachment_manifest,
        }));
        let prompt_revision = command.prompt_snapshot.revision.clone();
        let capability_snapshot_ref = command.capability_snapshot.snapshot_id.clone();
        let initial_attachments = command.attachments.clone();
        self.create_run(
            LocalAgentHostRunRequest {
                run_id,
                profile_key: PROFILE_KEY.to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: command.thread_id,
                project_id: command.project_id,
                model_runtime_snapshot: descriptor,
                prompt_revision,
                capability_snapshot_ref,
                causation_id: request_id.to_string(),
                deadline_at: None,
                initial_message: Some(InitialRunMessage {
                    record_id: command.message_id,
                    turn_id: command.turn_id,
                    content: command.content,
                    structured_payload,
                    message_source: "main_chat".to_string(),
                }),
                initial_attachments,
            },
            now,
        )
        .await
    }

    pub async fn create_task(
        &self,
        request_id: &str,
        command: CreateTaskCommand,
        session: &LocalAgentExecutionSession,
        now: DateTime<Utc>,
    ) -> Result<CreatedLocalAgentTask, LocalAgentHostError> {
        command.validate()?;
        const PROFILE_KEY: &str = "task_runner";
        self.profiles.require(PROFILE_KEY)?;
        let run_id = stable_host_id(
            "task-run",
            &[self.scope.owner_user_id.as_str(), command.task_id.as_str()],
        );
        let descriptor = self
            .descriptor_for_creation(&run_id, &command.model_config_id, session)
            .await?;
        let initial_message_id = stable_host_id(
            "task-message",
            &[self.scope.owner_user_id.as_str(), command.task_id.as_str()],
        );
        let initial_payload = json!({
            "type": "task_objective",
            "task_id": &command.task_id,
            "source_thread_id": &command.source_thread_id,
            "source_turn_id": &command.source_turn_id,
            "project_id": &command.project_id,
            "objective": &command.objective,
            "acceptance_criteria": &command.acceptance_criteria,
            "prompt_snapshot": &command.prompt_snapshot,
            "project_snapshot": &command.project_snapshot,
            "capability_snapshot": &command.capability_snapshot,
        });
        let created = create_local_agent_task(
            self.storage.as_ref(),
            CreateLocalAgentTaskRequest {
                run: CreateLocalAgentRunRequest {
                    scope: self.scope.clone(),
                    run_id,
                    profile_key: PROFILE_KEY.to_string(),
                    owner_entity_type: "task".to_string(),
                    owner_entity_id: command.task_id.clone(),
                    project_id: Some(command.project_id.clone()),
                    model_runtime_snapshot: descriptor,
                    prompt_revision: command.prompt_snapshot.revision.clone(),
                    capability_snapshot_ref: command.capability_snapshot.snapshot_id.clone(),
                    origin_device_id: self.device_id.clone(),
                    causation_id: request_id.to_string(),
                    deadline_at: None,
                    initial_message: Some(InitialRunMessage {
                        record_id: initial_message_id,
                        turn_id: command.source_turn_id.clone(),
                        content: Some(command.objective.clone()),
                        structured_payload: Some(initial_payload),
                        message_source: "task_creation".to_string(),
                    }),
                    initial_attachments: Vec::new(),
                    now,
                },
                task_id: command.task_id,
                source_thread_id: command.source_thread_id,
                source_turn_id: command.source_turn_id,
                project_id: command.project_id,
                objective: command.objective,
                acceptance_criteria: command.acceptance_criteria,
                prompt_snapshot: command.prompt_snapshot,
                project_snapshot: command.project_snapshot,
                capability_snapshot: command.capability_snapshot,
            },
        )
        .await?;
        self.scheduler
            .lock()
            .await
            .schedule(&created.run.start_event);
        Ok(created)
    }

    async fn descriptor_for_creation(
        &self,
        run_id: &str,
        model_config_id: &str,
        session: &LocalAgentExecutionSession,
    ) -> Result<ModelRuntimeDescriptor, LocalAgentHostError> {
        if let Some(existing) = self.load_run_record(run_id).await? {
            if existing.run.model_config_id != model_config_id {
                return Err(LocalAgentHostError::CreationConflict(run_id.to_string()));
            }
            return Ok(existing.run.model_runtime_snapshot);
        }
        Ok(self
            .gateway
            .descriptor(
                session.access_token.as_str(),
                model_config_id,
                session.cancellation.clone(),
            )
            .await?)
    }

    async fn load_run_record(&self, run_id: &str) -> StorageResult<Option<AgentRunStateRecord>> {
        let mut operation = LoadRunRecord {
            query: RecordQuery {
                scope: self.scope.clone(),
                id: run_id.to_string(),
            },
            result: None,
        };
        self.storage.transaction(&mut operation).await?;
        Ok(operation.result)
    }

    async fn load_task_record(&self, task_id: &str) -> StorageResult<Option<TaskRecord>> {
        let mut operation = LoadTaskRecord {
            query: RecordQuery {
                scope: self.scope.clone(),
                id: task_id.to_string(),
            },
            result: None,
        };
        self.storage.transaction(&mut operation).await?;
        Ok(operation.result)
    }

    pub async fn request_control(
        &self,
        run_id: impl Into<String>,
        action: RunControlAction,
        causation_id: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<AgentEventStateRecord, LocalAgentHostError> {
        let event = request_run_control(
            self.storage.as_ref(),
            RequestRunControl {
                scope: self.scope.clone(),
                run_id: run_id.into(),
                action,
                origin_device_id: self.device_id.clone(),
                causation_id: causation_id.into(),
                now,
            },
        )
        .await?;
        self.scheduler.lock().await.schedule(&event);
        self.scheduler_wake.notify_one();
        Ok(event)
    }

    pub async fn answer_user_question(
        &self,
        command: chatos_local_agent_protocol::AnswerUserQuestionCommand,
        causation_id: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<AgentEventStateRecord, LocalAgentHostError> {
        let answered = answer_run_interaction(
            self.storage.as_ref(),
            AnswerRunInteraction {
                scope: self.scope.clone(),
                run_id: command.run_id,
                interaction_id: command.interaction_id,
                answer: command.answer,
                origin_device_id: self.device_id.clone(),
                causation_id: causation_id.into(),
                now,
            },
        )
        .await?;
        self.scheduler.lock().await.schedule(&answered.resume_event);
        self.scheduler_wake.notify_one();
        Ok(answered.resume_event)
    }

    pub async fn decide_tool_approval(
        &self,
        command: ToolApprovalCommand,
        now: DateTime<Utc>,
    ) -> Result<ToolExecutionStateRecord, LocalAgentHostError> {
        command.validate()?;
        let execution = decide_tool_approval(
            self.storage.as_ref(),
            DecideToolApprovalRequest {
                scope: self.scope.clone(),
                invocation_id: command.invocation_id,
                decision: command.decision,
                reason: command.reason,
                now,
            },
        )
        .await?;
        self.refresh_recovery(now).await?;
        Ok(execution)
    }

    pub async fn commit_claimed(
        &self,
        claimed: &AgentEventStateRecord,
        evidence: StepEvidence,
        now: DateTime<Utc>,
    ) -> Result<CommittedReduction, LocalAgentHostError> {
        if claimed.metadata.id != claimed.event.event_id {
            return Err(LocalAgentHostError::ClaimedEventMismatch {
                expected: claimed.metadata.id.clone(),
                actual: claimed.event.event_id.clone(),
            });
        }
        let committed = reduce_and_commit(
            self.storage.as_ref(),
            ReduceAndCommitRequest {
                scope: self.scope.clone(),
                event_id: claimed.event.event_id.clone(),
                claim_token: claimed.event.claim_token.clone().unwrap_or_default(),
                origin_device_id: self.device_id.clone(),
                evidence,
                now,
                policy: self.policy.reducer,
            },
        )
        .await?;
        self.scheduler
            .lock()
            .await
            .schedule_all(committed.emitted_events.iter());
        if !committed.emitted_events.is_empty() {
            self.scheduler_wake.notify_one();
        }
        Ok(committed)
    }

    pub async fn begin_claimed_model_step(
        &self,
        claimed: &AgentEventStateRecord,
        now: DateTime<Utc>,
    ) -> Result<BeganModelStepExecution, LocalAgentHostError> {
        if claimed.metadata.id != claimed.event.event_id {
            return Err(LocalAgentHostError::ClaimedEventMismatch {
                expected: claimed.metadata.id.clone(),
                actual: claimed.event.event_id.clone(),
            });
        }
        if claimed.event.event_type != LocalAgentEventType::ModelStepRequested {
            return Err(LocalAgentHostError::NotModelStep(
                claimed.event.event_id.clone(),
            ));
        }
        Ok(begin_model_step_execution(
            self.storage.as_ref(),
            BeginModelStepExecutionRequest {
                scope: self.scope.clone(),
                event_id: claimed.event.event_id.clone(),
                claim_token: claimed.event.claim_token.clone().unwrap_or_default(),
                now,
            },
        )
        .await?)
    }

    pub async fn record_claimed_model_step_completion(
        &self,
        claimed: &AgentEventStateRecord,
        completion: ModelStepCompletion,
        assistant_message: Option<CompletedAssistantMessage>,
        provider_context_commit: Option<DurableProviderContextCommit>,
        now: DateTime<Utc>,
    ) -> Result<AgentEventStateRecord, LocalAgentHostError> {
        if claimed.metadata.id != claimed.event.event_id {
            return Err(LocalAgentHostError::ClaimedEventMismatch {
                expected: claimed.metadata.id.clone(),
                actual: claimed.event.event_id.clone(),
            });
        }
        if claimed.event.event_type != LocalAgentEventType::ModelStepRequested {
            return Err(LocalAgentHostError::NotModelStep(
                claimed.event.event_id.clone(),
            ));
        }
        let event = record_model_step_completion(
            self.storage.as_ref(),
            RecordModelStepCompletionRequest {
                scope: self.scope.clone(),
                request_event_id: claimed.event.event_id.clone(),
                claim_token: claimed.event.claim_token.clone().unwrap_or_default(),
                completion,
                assistant_message,
                provider_context_commit,
                origin_device_id: self.device_id.clone(),
                now,
            },
        )
        .await?;
        self.scheduler.lock().await.schedule(&event);
        self.scheduler_wake.notify_one();
        Ok(event)
    }

    /// Executes one claimed model request through the only supported Host
    /// pipeline. Context construction, the gateway request identity, retry
    /// policy, provider-context sealing, durable completion and scheduling are
    /// deliberately not exposed as choices to Profiles or native UI callers.
    pub async fn execute_claimed_model_step(
        &self,
        claimed: &AgentEventStateRecord,
        access_token: &str,
        callbacks: ModelGatewayCallbacks,
        cancellation: CancellationToken,
        now: DateTime<Utc>,
    ) -> Result<AgentEventStateRecord, LocalAgentHostError> {
        if access_token.trim().is_empty() {
            return Err(LocalAgentHostError::InvalidModelAccessToken);
        }
        let begun = self.begin_claimed_model_step(claimed, now).await?;
        let request_event = begun.request_event;
        let run = begun.run_record.run;
        let profile = self.profiles.require(run.profile_key.as_str())?;
        let request_id = request_event.event.event_id.clone();
        let turn_id = request_event.event.correlation_id.clone();
        let claim_token = request_event.event.claim_token.clone().unwrap_or_default();

        let execution = async {
            let context = self
                .context_runtime
                .prepare_model_step_context(self.storage.as_ref(), &self.scope, &run, &cancellation)
                .await;
            let context = match context {
                Ok(context) => context,
                Err(LocalAgentContextRuntimeError::Cancelled) => {
                    return Ok(ExecutedModelStep {
                        result: ModelStepResult::Cancelled,
                        output: None,
                        token_assessments: Vec::new(),
                        provider_context_commit: None,
                    });
                }
                Err(error) => return Err(LocalAgentHostError::ContextRuntime(error)),
            };
            self.model_steps
                .execute(
                    access_token,
                    &run,
                    request_id,
                    profile.as_ref(),
                    context,
                    callbacks,
                    cancellation,
                )
                .await
                .map_err(LocalAgentHostError::ModelStepExecutor)
        };
        let executed = self
            .await_model_execution_with_claim_renewal(
                request_event.event.event_id.as_str(),
                claim_token.as_str(),
                execution,
            )
            .await;
        let executed = match executed {
            Ok(executed) => executed,
            Err(error) => match memory_context_block(&run, &error) {
                Some(details) => ExecutedModelStep {
                    result: ModelStepResult::Blocked(details),
                    output: None,
                    token_assessments: Vec::new(),
                    provider_context_commit: None,
                },
                None => match retryable_model_execution(&error) {
                    Some(details) => ExecutedModelStep {
                        result: ModelStepResult::Retry(details),
                        output: None,
                        token_assessments: Vec::new(),
                        provider_context_commit: None,
                    },
                    None => return Err(error),
                },
            },
        };
        let completion_now = Utc::now();
        let retry_at = if matches!(&executed.result, ModelStepResult::Retry(_)) {
            let retry_delay = model_retry_delay(&run, self.policy)?;
            Some(
                completion_now
                    .checked_add_signed(retry_delay)
                    .ok_or(LocalAgentHostError::ModelRetryDeadlineOverflow)?,
            )
        } else {
            None
        };
        let prepared = prepare_model_step_persistence(
            &run,
            request_event.event.event_id.as_str(),
            turn_id.as_str(),
            executed,
            retry_at,
            completion_now,
        )?;
        let provider_context_commit = match prepared.provider_context_commit {
            Some(commit) => Some(
                self.context_runtime
                    .seal_provider_context_commit(&run, commit, completion_now)
                    .await?,
            ),
            None => None,
        };
        self.record_claimed_model_step_completion(
            &request_event,
            prepared.completion,
            prepared.assistant_message,
            provider_context_commit,
            completion_now,
        )
        .await
    }

    async fn await_model_execution_with_claim_renewal<F>(
        &self,
        event_id: &str,
        claim_token: &str,
        execution: F,
    ) -> Result<ExecutedModelStep, LocalAgentHostError>
    where
        F: std::future::Future<Output = Result<ExecutedModelStep, LocalAgentHostError>>,
    {
        let renewal_period = self.policy.claim_ttl.to_std().map_err(|_| {
            LocalAgentHostError::InvalidConfiguration("model event claim TTL cannot be represented")
        })? / 3;
        if renewal_period.is_zero() {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "model event claim renewal period is zero",
            ));
        }
        let first_tick = tokio::time::Instant::now() + renewal_period;
        let mut interval = tokio::time::interval_at(first_tick, renewal_period);
        tokio::pin!(execution);
        loop {
            tokio::select! {
                result = &mut execution => return result,
                _ = interval.tick() => {
                    let now = Utc::now();
                    let claim_until = now
                        .checked_add_signed(self.policy.claim_ttl)
                        .ok_or(LocalAgentHostError::InvalidConfiguration(
                            "model event claim renewal overflow",
                        ))?;
                    renew_event_claim(
                        self.storage.as_ref(),
                        RenewEventClaimRequest {
                            scope: self.scope.clone(),
                            event_id: event_id.to_string(),
                            device_id: self.device_id.clone(),
                            claim_token: claim_token.to_string(),
                            now,
                            claim_until,
                        },
                    )
                    .await
                    .map_err(LocalAgentHostError::ModelClaimRenewal)?;
                }
            }
        }
    }

    /// Applies events whose evidence is already encoded in their durable
    /// payload. Model and tool completion payloads are decoded here so native
    /// callers cannot accidentally supply different evidence.
    pub async fn commit_claimed_protocol_event(
        &self,
        claimed: &AgentEventStateRecord,
        now: DateTime<Utc>,
    ) -> Result<CommittedReduction, LocalAgentHostError> {
        let evidence = match claimed.event.event_type {
            LocalAgentEventType::ModelStepRequested => {
                return Err(LocalAgentHostError::ModelStepExecutorRequired(
                    claimed.event.event_id.clone(),
                ));
            }
            LocalAgentEventType::ModelStepCompleted => {
                let payload: DurableModelStepCompletionPayload = serde_json::from_value(
                    claimed.event.bounded_payload.clone(),
                )
                .map_err(|error| LocalAgentHostError::InvalidEventPayload(error.to_string()))?;
                StepEvidence::from(payload.completion)
            }
            LocalAgentEventType::ToolBatchCompleted => {
                let outcome_unknown = claimed
                    .event
                    .bounded_payload
                    .get("outcome_unknown")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| {
                        LocalAgentHostError::InvalidEventPayload(
                            "tool batch completion has no outcome_unknown flag".to_string(),
                        )
                    })?;
                StepEvidence::ToolBatch { outcome_unknown }
            }
            LocalAgentEventType::ToolBatchRequested => {
                return Err(LocalAgentHostError::NotToolBatch(
                    claimed.event.event_id.clone(),
                ));
            }
            _ => StepEvidence::None,
        };
        self.commit_claimed(claimed, evidence, now).await
    }

    /// Routes exactly one claimed event through the shared Rust execution
    /// runtime. Platform clients own wake/sleep lifecycle, but never choose a
    /// second model loop, tool loop, or reducer path based on Profile type.
    pub async fn process_claimed_event(
        &self,
        claimed: &AgentEventStateRecord,
        session: &LocalAgentExecutionSession,
        now: DateTime<Utc>,
    ) -> Result<ProcessedClaimedEvent, LocalAgentHostError> {
        match claimed.event.event_type {
            LocalAgentEventType::ModelStepRequested => self
                .execute_claimed_model_step(
                    claimed,
                    session.access_token.as_str(),
                    session.callbacks.clone(),
                    session.cancellation.clone(),
                    now,
                )
                .await
                .map(Box::new)
                .map(ProcessedClaimedEvent::ModelCompletionScheduled),
            LocalAgentEventType::ToolBatchRequested => {
                match self.execute_claimed_tool_batch(claimed, session, now).await {
                    Ok(committed) => Ok(ProcessedClaimedEvent::ReductionCommitted(Box::new(
                        committed,
                    ))),
                    Err(LocalAgentHostError::ToolApprovalPending(event_id)) => {
                        Ok(ProcessedClaimedEvent::ToolApprovalPending {
                            event_id,
                            run_id: claimed.event.run_id.clone(),
                        })
                    }
                    Err(error) => Err(error),
                }
            }
            _ => self
                .commit_claimed_protocol_event(claimed, now)
                .await
                .map(Box::new)
                .map(ProcessedClaimedEvent::ReductionCommitted),
        }
    }

    /// Executes one already-claimed tool batch. Invocation identities and
    /// arguments are frozen before dispatch; irreversible calls are marked
    /// started before I/O and are never replayed after an indeterminate exit.
    pub async fn execute_claimed_tool_batch(
        &self,
        claimed: &AgentEventStateRecord,
        session: &LocalAgentExecutionSession,
        now: DateTime<Utc>,
    ) -> Result<CommittedReduction, LocalAgentHostError> {
        if claimed.metadata.id != claimed.event.event_id {
            return Err(LocalAgentHostError::ClaimedEventMismatch {
                expected: claimed.metadata.id.clone(),
                actual: claimed.event.event_id.clone(),
            });
        }
        if claimed.event.event_type != LocalAgentEventType::ToolBatchRequested {
            return Err(LocalAgentHostError::NotToolBatch(
                claimed.event.event_id.clone(),
            ));
        }
        let claim_token = claimed.event.claim_token.clone().unwrap_or_default();
        let batch = prepare_tool_batch(
            self.storage.as_ref(),
            PrepareToolBatchRequest {
                scope: self.scope.clone(),
                event_id: claimed.event.event_id.clone(),
                claim_token: claim_token.clone(),
                now,
            },
        )
        .await?;

        let prepared =
            inspect_tool_batch(self.storage.as_ref(), self.scope.clone(), &batch).await?;
        if prepared.awaiting_approval {
            let deferral = defer_tool_batch_for_approval(
                self.storage.as_ref(),
                DeferToolBatchForApprovalRequest {
                    scope: self.scope.clone(),
                    event_id: claimed.event.event_id.clone(),
                    claim_token: claim_token.clone(),
                    batch: batch.clone(),
                    now: Utc::now(),
                },
            )
            .await?;
            if matches!(deferral, ToolApprovalDeferralResult::Deferred(_)) {
                return Err(LocalAgentHostError::ToolApprovalPending(
                    claimed.event.event_id.clone(),
                ));
            }
        }

        'calls: for call in &batch.calls {
            match begin_tool_execution(
                self.storage.as_ref(),
                BeginToolExecutionRequest {
                    scope: self.scope.clone(),
                    invocation_id: call.invocation_id.clone(),
                    now: Utc::now(),
                },
            )
            .await?
            {
                BeginToolExecutionResult::Execute(_) => {
                    let invocation = build_local_tool_invocation(&batch, call);
                    let execution = async {
                        if invocation.tool_name == "create_local_task" {
                            self.execute_local_task_creation_tool(&invocation, session)
                                .await
                        } else {
                            self.tool_runtime
                                .execute(invocation, session.cancellation.clone())
                                .await
                        }
                    };
                    match self
                        .execute_with_claim_renewal(
                            claimed.event.event_id.as_str(),
                            claim_token.as_str(),
                            execution,
                        )
                        .await
                    {
                        Ok(outcome) => {
                            validate_local_tool_outcome(&outcome)
                                .map_err(LocalAgentHostError::InvalidToolOutcome)?;
                            complete_tool_execution(
                                self.storage.as_ref(),
                                CompleteToolExecutionRequest {
                                    scope: self.scope.clone(),
                                    invocation_id: call.invocation_id.clone(),
                                    status: outcome.status,
                                    bounded_result: outcome.bounded_result,
                                    now: Utc::now(),
                                },
                            )
                            .await?;
                        }
                        Err(_error)
                            if call.effect.requires_durable_start()
                                && !call.effect.can_replay_after_started() =>
                        {
                            mark_tool_outcome_unknown(
                                self.storage.as_ref(),
                                MarkToolOutcomeUnknownRequest {
                                    scope: self.scope.clone(),
                                    invocation_id: call.invocation_id.clone(),
                                    now: Utc::now(),
                                },
                            )
                            .await?;
                            // The batch is still reduced below so the Run
                            // enters needs_review instead of retrying the call.
                            break 'calls;
                        }
                        Err(error) => return Err(LocalAgentHostError::ToolRuntime(error)),
                    }
                }
                BeginToolExecutionResult::AlreadyCompleted(_) => {}
                BeginToolExecutionResult::AwaitingApproval(_) => {
                    return Err(LocalAgentHostError::ToolApprovalPending(
                        claimed.event.event_id.clone(),
                    ));
                }
                BeginToolExecutionResult::NeedsReview(_) => break 'calls,
            }
        }

        let state = inspect_tool_batch(self.storage.as_ref(), self.scope.clone(), &batch).await?;
        if !state.all_completed && !state.outcome_unknown {
            return Err(LocalAgentHostError::IncompleteToolBatch);
        }
        self.commit_claimed(
            claimed,
            StepEvidence::ToolBatch {
                outcome_unknown: state.outcome_unknown,
            },
            Utc::now(),
        )
        .await
    }

    async fn execute_with_claim_renewal<T>(
        &self,
        event_id: &str,
        claim_token: &str,
        execution: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let renewal_period = self
            .policy
            .claim_ttl
            .to_std()
            .map_err(|_| "tool event claim TTL cannot be represented".to_string())?
            / 3;
        if renewal_period.is_zero() {
            return Err("tool event claim renewal period is zero".to_string());
        }
        let first_tick = tokio::time::Instant::now() + renewal_period;
        let mut interval = tokio::time::interval_at(first_tick, renewal_period);
        tokio::pin!(execution);
        loop {
            tokio::select! {
                outcome = &mut execution => return outcome,
                _ = interval.tick() => {
                    let now = Utc::now();
                    let claim_until = now
                        .checked_add_signed(self.policy.claim_ttl)
                        .ok_or_else(|| "tool event claim renewal overflow".to_string())?;
                    renew_event_claim(
                        self.storage.as_ref(),
                        RenewEventClaimRequest {
                            scope: self.scope.clone(),
                            event_id: event_id.to_string(),
                            device_id: self.device_id.clone(),
                            claim_token: claim_token.to_string(),
                            now,
                            claim_until,
                        },
                    )
                    .await
                    .map_err(|error| format!("tool event claim renewal failed: {error}"))?;
                }
            }
        }
    }

    async fn execute_local_task_creation_tool(
        &self,
        invocation: &LocalToolInvocation,
        session: &LocalAgentExecutionSession,
    ) -> Result<LocalToolOutcome, String> {
        if invocation.effect != ToolEffect::IdempotentWrite {
            return Err("create_local_task must be an idempotent_write tool".to_string());
        }
        let parent = self
            .load_run_record(&invocation.run_id)
            .await
            .map_err(|error| format!("failed to load Main Chat run: {error}"))?
            .ok_or_else(|| "Main Chat run was not found".to_string())?;
        if parent.run.profile_key != "main_chat"
            || parent.run.owner_entity_type != "conversation"
            || parent.run.project_id != invocation.project_id
            || parent.run.capability_snapshot_ref != invocation.capability_snapshot_ref
        {
            return Err(
                "create_local_task invocation does not match its frozen Main Chat run".to_string(),
            );
        }
        let project_id = invocation
            .project_id
            .clone()
            .ok_or_else(|| "create_local_task requires a frozen project_id".to_string())?;
        let arguments = invocation
            .arguments
            .as_object()
            .ok_or_else(|| "create_local_task arguments must be a JSON object".to_string())?;
        if arguments
            .keys()
            .any(|key| !matches!(key.as_str(), "objective" | "acceptance_criteria"))
        {
            return Err("create_local_task contains unsupported arguments".to_string());
        }
        let objective = arguments
            .get("objective")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "create_local_task objective must not be empty".to_string())?
            .to_string();
        let acceptance_criteria = arguments
            .get("acceptance_criteria")
            .and_then(serde_json::Value::as_array)
            .filter(|criteria| !criteria.is_empty())
            .ok_or_else(|| "create_local_task acceptance_criteria must not be empty".to_string())?
            .iter()
            .map(|criterion| {
                criterion
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        "create_local_task acceptance criteria must be non-empty strings"
                            .to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let task_id = stable_host_id(
            "local-task",
            &[
                self.scope.owner_user_id.as_str(),
                invocation.invocation_id.as_str(),
            ],
        );
        let planning_request = LocalTaskPlanningRequest {
            task_id: task_id.clone(),
            parent_run_id: parent.run.run_id,
            source_thread_id: parent.run.owner_entity_id,
            source_turn_id: invocation.source_turn_id.clone(),
            project_id: project_id.clone(),
            objective: objective.clone(),
            acceptance_criteria: acceptance_criteria.clone(),
            parent_capability_snapshot_ref: invocation.capability_snapshot_ref.clone(),
        };
        if let Some(outcome) = self
            .existing_local_task_creation_outcome(&planning_request)
            .await?
        {
            return Ok(outcome);
        }
        let plan = self
            .task_planner
            .plan_task(&planning_request, session.cancellation.clone())
            .await?;
        if plan.project_id != project_id {
            return Err(
                "local Task plan does not match the project frozen by the Main Chat run"
                    .to_string(),
            );
        }
        let created = self
            .create_task(
                invocation.invocation_id.as_str(),
                CreateTaskCommand {
                    task_id: task_id.clone(),
                    source_thread_id: planning_request.source_thread_id,
                    source_turn_id: planning_request.source_turn_id,
                    project_id,
                    objective,
                    acceptance_criteria,
                    model_config_id: plan.model_config_id,
                    prompt_snapshot: plan.prompt_snapshot,
                    project_snapshot: plan.project_snapshot,
                    capability_snapshot: plan.capability_snapshot,
                },
                session,
                Utc::now(),
            )
            .await
            .map_err(|error| format!("failed to create local Task: {error}"))?;
        Ok(LocalToolOutcome::succeeded(json!({
            "task_id": task_id,
            "run_id": created.run.run_record.run.run_id,
            "project_id": created.run.run_record.run.project_id,
        })))
    }

    async fn existing_local_task_creation_outcome(
        &self,
        request: &LocalTaskPlanningRequest,
    ) -> Result<Option<LocalToolOutcome>, String> {
        let Some(task) = self
            .load_task_record(&request.task_id)
            .await
            .map_err(|error| format!("failed to load an existing local Task: {error}"))?
        else {
            return Ok(None);
        };
        let expected_criteria = json!(request.acceptance_criteria);
        if task.conversation_id.as_deref() != Some(request.source_thread_id.as_str())
            || task.state.get("source_thread_id") != Some(&json!(request.source_thread_id))
            || task.state.get("source_turn_id") != Some(&json!(request.source_turn_id))
            || task.state.get("project_id") != Some(&json!(request.project_id))
            || task.state.get("objective") != Some(&json!(request.objective))
            || task.state.get("acceptance_criteria") != Some(&expected_criteria)
        {
            return Err(
                "existing local Task does not match the replayed create_local_task invocation"
                    .to_string(),
            );
        }
        let run_id = task
            .state
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "existing local Task has no frozen run_id".to_string())?;
        let run = self
            .load_run_record(run_id)
            .await
            .map_err(|error| format!("failed to load the existing Task Runner run: {error}"))?
            .ok_or_else(|| "existing local Task has no durable Task Runner run".to_string())?;
        if run.run.profile_key != "task_runner"
            || run.run.owner_entity_type != "task"
            || run.run.owner_entity_id != request.task_id
            || run.run.project_id.as_deref() != Some(request.project_id.as_str())
        {
            return Err("existing Task Runner run does not match its local Task".to_string());
        }
        Ok(Some(LocalToolOutcome::succeeded(json!({
            "task_id": task.metadata.id,
            "run_id": run.run.run_id,
            "project_id": run.run.project_id,
        }))))
    }

    pub async fn refresh_recovery(
        &self,
        now: DateTime<Utc>,
    ) -> Result<LocalAgentHostStartupReport, LocalAgentHostError> {
        let plan = scan_recoverable_work(self.storage.as_ref(), self.scope.clone(), now).await?;
        self.scheduler.lock().await.merge_recovery(&plan);
        if !plan.ready_events.is_empty() || plan.next_wake_at.is_some() {
            self.scheduler_wake.notify_one();
        }
        Ok(LocalAgentHostStartupReport {
            active_run_count: plan.active_runs.len(),
            ready_event_count: plan.ready_events.len(),
            next_wake_at: plan.next_wake_at,
            recovery_issues: plan.issues,
        })
    }

    pub(crate) async fn wait_for_scheduler_wake(&self) {
        self.scheduler_wake.notified().await;
    }
}

struct LoadRunRecord {
    query: RecordQuery,
    result: Option<AgentRunStateRecord>,
}

struct LoadTaskRecord {
    query: RecordQuery,
    result: Option<TaskRecord>,
}

#[async_trait]
impl StorageTransaction for LoadRunRecord {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.result = repositories.agent_runs().get(&self.query).await?;
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for LoadTaskRecord {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.result = repositories.tasks().get(&self.query).await?;
        Ok(())
    }
}

pub(crate) fn stable_host_id(prefix: &str, values: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{prefix}:{:x}", hasher.finalize())
}

/// Handles the only two commands allowed to create durable Agent work. The
/// authenticated execution session is supplied by the native Host process and
/// is never serialized into the command or persisted with the resulting Run.
pub struct LocalAgentHostCreationExecutor {
    host: Arc<LocalAgentHost>,
    session: LocalAgentExecutionSession,
    next: Arc<dyn LocalAgentIpcMutationExecutor>,
}

impl LocalAgentHostCreationExecutor {
    pub fn new(
        host: Arc<LocalAgentHost>,
        session: LocalAgentExecutionSession,
        next: Arc<dyn LocalAgentIpcMutationExecutor>,
    ) -> Self {
        Self {
            host,
            session,
            next,
        }
    }
}

#[async_trait]
impl LocalAgentIpcMutationExecutor for LocalAgentHostCreationExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let event = match command {
            LocalAgentCommand::CreateMainChatTurn(command) => self
                .host
                .create_main_chat_turn(request_id, *command, &self.session, Utc::now())
                .await
                .map(|created| created.start_event),
            LocalAgentCommand::CreateTask(command) => self
                .host
                .create_task(request_id, *command, &self.session, Utc::now())
                .await
                .map(|created| created.run.start_event),
            other => return self.next.execute_mutation(request_id, other).await,
        }
        .map_err(run_creation_ipc_error)?;
        Ok(LocalAgentIpcResponse::Accepted {
            operation_id: event.event.event_id,
        })
    }
}

/// Adds durable Run lifecycle commands to an IPC executor chain. Commands
/// owned by model/run creation, approval, or platform storage are delegated to
/// the next typed executor instead of being reimplemented here.
pub struct LocalAgentHostControlExecutor {
    host: Arc<LocalAgentHost>,
    next: Arc<dyn LocalAgentIpcMutationExecutor>,
}

impl LocalAgentHostControlExecutor {
    pub fn new(host: Arc<LocalAgentHost>, next: Arc<dyn LocalAgentIpcMutationExecutor>) -> Self {
        Self { host, next }
    }
}

#[async_trait]
impl LocalAgentIpcMutationExecutor for LocalAgentHostControlExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let (run_id, action) = match command {
            LocalAgentCommand::PauseRun { run_id } => (run_id, RunControlAction::Pause),
            LocalAgentCommand::ResumeRun { run_id } => (run_id, RunControlAction::Resume),
            LocalAgentCommand::CancelRun { run_id } => (run_id, RunControlAction::Cancel),
            LocalAgentCommand::AnswerUserQuestion(command) => {
                return self
                    .host
                    .answer_user_question(command, request_id, Utc::now())
                    .await
                    .map(|event| LocalAgentIpcResponse::Accepted {
                        operation_id: event.event.event_id,
                    })
                    .map_err(run_control_ipc_error);
            }
            LocalAgentCommand::DecideToolApproval(command) => {
                return self
                    .host
                    .decide_tool_approval(command, Utc::now())
                    .await
                    .map(|execution| LocalAgentIpcResponse::Accepted {
                        operation_id: execution.execution.invocation_id,
                    })
                    .map_err(run_control_ipc_error);
            }
            other => return self.next.execute_mutation(request_id, other).await,
        };
        self.host
            .request_control(run_id, action, request_id, Utc::now())
            .await
            .map(|event| LocalAgentIpcResponse::Accepted {
                operation_id: event.event.event_id,
            })
            .map_err(run_control_ipc_error)
    }
}

fn run_control_ipc_error(error: LocalAgentHostError) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "run_control_rejected".to_string(),
        message: error.to_string(),
        retryable: matches!(
            error,
            LocalAgentHostError::Storage(StorageError::Unavailable { .. })
        ),
    }
}

fn run_creation_ipc_error(error: LocalAgentHostError) -> LocalAgentIpcError {
    let retryable = match &error {
        LocalAgentHostError::Storage(StorageError::Unavailable { .. }) => true,
        LocalAgentHostError::ModelGateway(ModelGatewayClientError::Transport { .. }) => true,
        LocalAgentHostError::ModelGateway(ModelGatewayClientError::HttpStatus {
            status, ..
        }) => matches!(*status, 408 | 429 | 500..=599),
        _ => false,
    };
    LocalAgentIpcError {
        code: "run_creation_rejected".to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn memory_context_block(
    run: &chatos_local_agent_protocol::LocalAgentRun,
    error: &LocalAgentHostError,
) -> Option<serde_json::Value> {
    if run.context_strategy != chatos_local_agent_protocol::ContextStrategy::MemoryEngine {
        return None;
    }
    let (reason, detail) = match error {
        LocalAgentHostError::ContextRuntime(LocalAgentContextRuntimeError::Runtime(detail)) => {
            ("memory_sync_unavailable", detail.clone())
        }
        LocalAgentHostError::ModelStepExecutor(ModelStepExecutorError::MemoryContext(error)) => {
            ("memory_context_unavailable", error.to_string())
        }
        LocalAgentHostError::ModelStepExecutor(
            ModelStepExecutorError::ContextReductionNoImprovement { .. },
        ) => ("memory_summary_no_improvement", error.to_string()),
        LocalAgentHostError::ModelStepExecutor(
            ModelStepExecutorError::ContextReductionAttemptsExhausted { .. },
        ) => ("memory_summary_attempts_exhausted", error.to_string()),
        _ => return None,
    };
    Some(serde_json::json!({
        "reason": reason,
        "detail": bounded_runtime_error(detail.as_str()),
    }))
}

fn bounded_runtime_error(error: &str) -> &str {
    const MAX_ERROR_BYTES: usize = 2_048;
    if error.len() <= MAX_ERROR_BYTES {
        return error;
    }
    let mut end = MAX_ERROR_BYTES;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    &error[..end]
}

fn retryable_model_execution(error: &LocalAgentHostError) -> Option<serde_json::Value> {
    let gateway = match error {
        LocalAgentHostError::ModelStepExecutor(ModelStepExecutorError::Gateway(error)) => error,
        LocalAgentHostError::ModelStepExecutor(ModelStepExecutorError::TokenGuard(
            ModelInputTokenGuardError::ExactCount(error),
        )) => error,
        _ => return None,
    };
    if !retryable_gateway_error(gateway) {
        return None;
    }
    let detail = gateway.to_string();
    Some(serde_json::json!({
        "reason": "model_gateway_unavailable",
        "detail": bounded_runtime_error(detail.as_str()),
    }))
}

fn retryable_gateway_error(error: &ModelGatewayClientError) -> bool {
    match error {
        ModelGatewayClientError::Transport { .. } | ModelGatewayClientError::StreamTransport(_) => {
            true
        }
        ModelGatewayClientError::HttpStatus { status, .. } => {
            *status == 408 || *status == 429 || *status >= 500
        }
        ModelGatewayClientError::StreamContract(ModelGatewayStreamError::MissingTerminal) => true,
        _ => false,
    }
}

fn model_retry_delay(
    run: &chatos_local_agent_protocol::LocalAgentRun,
    policy: LocalAgentHostPolicy,
) -> Result<Duration, LocalAgentHostError> {
    let exponent = run.retry_count.min(30);
    let factor = 1_i32 << exponent;
    Ok(policy
        .model_retry_delay
        .checked_mul(factor)
        .ok_or(LocalAgentHostError::ModelRetryDeadlineOverflow)?
        .min(policy.maximum_model_retry_delay))
}
