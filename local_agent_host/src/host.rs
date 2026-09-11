// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{AgentEventStateRecord, ClientStorage, RecordScope, StorageError};
use chatos_local_agent_protocol::{
    LocalAgentCommand, LocalAgentEventType, LocalAgentIpcError, LocalAgentIpcResponse,
    ModelRuntimeDescriptor, ModelStepCompletion, ModelStepResult,
};
use chatos_local_agent_runtime::{
    answer_run_interaction, begin_model_step_execution, begin_tool_execution,
    build_local_tool_invocation, complete_tool_execution, create_local_agent_run,
    inspect_tool_batch, mark_tool_outcome_unknown, prepare_model_step_persistence,
    prepare_tool_batch, record_model_step_completion, reduce_and_commit, renew_event_claim,
    request_run_control, scan_recoverable_work, validate_local_tool_outcome, AnswerRunInteraction,
    BeganModelStepExecution, BeginModelStepExecutionRequest, BeginToolExecutionRequest,
    BeginToolExecutionResult, CommittedReduction, CompleteToolExecutionRequest,
    CompletedAssistantMessage, CreateLocalAgentRunRequest, CreatedLocalAgentRun,
    DurableModelStepCompletionPayload, DurableProviderContextCommit, DurableScheduler,
    ExecutedModelStep, InitialRunMessage, LocalToolRuntime, MarkToolOutcomeUnknownRequest,
    ModelGatewayCallbacks, ModelGatewayClient, ModelStepExecutorError, ModelStepPersistenceError,
    PrepareToolBatchRequest, RecordModelStepCompletionRequest, RecoveryIssue,
    ReduceAndCommitRequest, ReducerPolicy, RenewEventClaimRequest, RequestRunControl,
    RunControlAction, SchedulerTickRequest, SchedulerTickResult, SingleModelStepExecutor,
    StepEvidence,
};
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    LocalAgentContextRuntime, LocalAgentContextRuntimeError, LocalAgentIpcMutationExecutor,
    LocalAgentProfileRegistry, ProfileRegistryError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgentHostPolicy {
    pub claim_ttl: Duration,
    pub maximum_event_attempts: u32,
    pub model_retry_delay: Duration,
    pub reducer: ReducerPolicy,
}

impl Default for LocalAgentHostPolicy {
    fn default() -> Self {
        Self {
            claim_ttl: Duration::seconds(90),
            maximum_event_attempts: 3,
            model_retry_delay: Duration::seconds(5),
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
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostError {
    #[error("invalid local Agent Host configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Profile(#[from] ProfileRegistryError),
    #[error("event {actual} does not match the claimed event {expected}")]
    ClaimedEventMismatch { expected: String, actual: String },
    #[error("event {0} is not a claimed local tool batch")]
    NotToolBatch(String),
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
    #[error(transparent)]
    ContextRuntime(#[from] LocalAgentContextRuntimeError),
    #[error(transparent)]
    ModelStepExecutor(#[from] ModelStepExecutorError),
    #[error(transparent)]
    ModelStepPersistence(#[from] ModelStepPersistenceError),
    #[error("model retry deadline overflowed")]
    ModelRetryDeadlineOverflow,
}

pub struct LocalAgentHost {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    policy: LocalAgentHostPolicy,
    scheduler: Mutex<DurableScheduler>,
    profiles: LocalAgentProfileRegistry,
    model_steps: SingleModelStepExecutor,
    context_runtime: Arc<dyn LocalAgentContextRuntime>,
    tool_runtime: Arc<dyn LocalToolRuntime>,
}

impl LocalAgentHost {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        storage: Arc<dyn ClientStorage>,
        gateway: Arc<dyn ModelGatewayClient>,
        context_runtime: Arc<dyn LocalAgentContextRuntime>,
        tool_runtime: Arc<dyn LocalToolRuntime>,
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
        {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "claim TTL, event attempt limit, and model retry delay must be positive",
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
                profiles,
                model_steps: SingleModelStepExecutor::new(gateway),
                context_runtime,
                tool_runtime,
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
                now,
            },
        )
        .await?;
        self.scheduler.lock().await.schedule(&created.start_event);
        Ok(created)
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
        Ok(answered.resume_event)
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
            .await?;
        let completion_now = Utc::now();
        let retry_at = if matches!(&executed.result, ModelStepResult::Retry(_)) {
            Some(
                completion_now
                    .checked_add_signed(self.policy.model_retry_delay)
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
                    .await?;
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

    /// Executes one already-claimed tool batch. Invocation identities and
    /// arguments are frozen before dispatch; irreversible calls are marked
    /// started before I/O and are never replayed after an indeterminate exit.
    pub async fn execute_claimed_tool_batch(
        &self,
        claimed: &AgentEventStateRecord,
        cancellation: CancellationToken,
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
                    match self
                        .execute_tool_with_claim_renewal(
                            claimed.event.event_id.as_str(),
                            claim_token.as_str(),
                            invocation,
                            cancellation.clone(),
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
                        Err(_error) if call.effect.requires_durable_start() => {
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

    async fn execute_tool_with_claim_renewal(
        &self,
        event_id: &str,
        claim_token: &str,
        invocation: chatos_local_agent_runtime::LocalToolInvocation,
        cancellation: CancellationToken,
    ) -> Result<chatos_local_agent_runtime::LocalToolOutcome, String> {
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
        let execution = self.tool_runtime.execute(invocation, cancellation);
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

    pub async fn refresh_recovery(
        &self,
        now: DateTime<Utc>,
    ) -> Result<LocalAgentHostStartupReport, LocalAgentHostError> {
        let plan = scan_recoverable_work(self.storage.as_ref(), self.scope.clone(), now).await?;
        self.scheduler.lock().await.merge_recovery(&plan);
        Ok(LocalAgentHostStartupReport {
            active_run_count: plan.active_runs.len(),
            ready_event_count: plan.ready_events.len(),
            next_wake_at: plan.next_wake_at,
            recovery_issues: plan.issues,
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
