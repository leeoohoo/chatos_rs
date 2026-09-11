// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{AgentEventStateRecord, ClientStorage, RecordScope, StorageError};
use chatos_local_agent_protocol::{
    LocalAgentCommand, LocalAgentEventType, LocalAgentIpcError, LocalAgentIpcResponse,
    ModelStepCompletion,
};
use chatos_local_agent_runtime::{
    answer_run_interaction, begin_tool_execution, build_local_tool_invocation,
    complete_tool_execution, inspect_tool_batch, mark_tool_outcome_unknown, prepare_tool_batch,
    reduce_and_commit, renew_event_claim, request_run_control, scan_recoverable_work,
    validate_local_tool_outcome, AnswerRunInteraction, BeginToolExecutionRequest,
    BeginToolExecutionResult, CommittedReduction, CompleteToolExecutionRequest, DurableScheduler,
    LocalToolRuntime, MarkToolOutcomeUnknownRequest, ModelGatewayClient, PrepareToolBatchRequest,
    RecoveryIssue, ReduceAndCommitRequest, ReducerPolicy, RenewEventClaimRequest,
    RequestRunControl, RunControlAction, SchedulerTickRequest, SchedulerTickResult,
    SingleModelStepExecutor, StepEvidence,
};
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{LocalAgentIpcMutationExecutor, LocalAgentProfileRegistry, ProfileRegistryError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgentHostPolicy {
    pub claim_ttl: Duration,
    pub maximum_event_attempts: u32,
    pub reducer: ReducerPolicy,
}

impl Default for LocalAgentHostPolicy {
    fn default() -> Self {
        Self {
            claim_ttl: Duration::seconds(90),
            maximum_event_attempts: 3,
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
    #[error("local tool runtime failed after invocation start: {0}")]
    ToolRuntime(String),
    #[error("local tool runtime returned an invalid outcome: {0}")]
    InvalidToolOutcome(String),
    #[error("local tool batch still has uncompleted invocations")]
    IncompleteToolBatch,
    #[error("claimed event payload is invalid: {0}")]
    InvalidEventPayload(String),
}

pub struct LocalAgentHost {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    policy: LocalAgentHostPolicy,
    scheduler: Mutex<DurableScheduler>,
    profiles: LocalAgentProfileRegistry,
    model_steps: SingleModelStepExecutor,
    tool_runtime: Arc<dyn LocalToolRuntime>,
}

impl LocalAgentHost {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        storage: Arc<dyn ClientStorage>,
        gateway: Arc<dyn ModelGatewayClient>,
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
        if policy.claim_ttl <= Duration::zero() || policy.maximum_event_attempts == 0 {
            return Err(LocalAgentHostError::InvalidConfiguration(
                "claim TTL and event attempt limit must be positive",
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

    /// Applies events whose evidence is already encoded in their durable
    /// payload. Model and tool completion payloads are decoded here so native
    /// callers cannot accidentally supply different evidence.
    pub async fn commit_claimed_protocol_event(
        &self,
        claimed: &AgentEventStateRecord,
        now: DateTime<Utc>,
    ) -> Result<CommittedReduction, LocalAgentHostError> {
        let evidence = match claimed.event.event_type {
            LocalAgentEventType::ModelStepCompleted => {
                let completion: ModelStepCompletion = serde_json::from_value(
                    claimed.event.bounded_payload.clone(),
                )
                .map_err(|error| LocalAgentHostError::InvalidEventPayload(error.to_string()))?;
                StepEvidence::from(completion)
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
