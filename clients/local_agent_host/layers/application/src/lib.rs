// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Durable, client-owned Local Agent state machine.
//!
//! Owns run creation, claim leases, one-step transitions, cancellation, event
//! replay, and conservative recovery behind durable state contracts.

use chatos_local_agent_ports::{
    ClientStorageError, IdempotentCommand, LocalAgentStore, RunTransition,
};
use chatos_local_agent_protocol::{
    CreateRunCommand, HostCommand, HostError, HostRequestEnvelope, HostResponseEnvelope,
    HostResult, LocalAgentRunRecord, LocalAgentRunStatus, LocalAgentStepOutcome,
    LocalAgentToolBatch,
};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use uuid::Uuid;

mod profile;
mod task_runtime;

pub use profile::{LocalAgentProfile, LocalAgentProfileRegistry};

#[derive(Debug, Error)]
pub enum LocalAgentRuntimeError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Storage(#[from] ClientStorageError),
    #[error("run state is invalid: {0}")]
    InvalidRunState(String),
    #[error("clock is unavailable: {0}")]
    Clock(String),
}

impl LocalAgentRuntimeError {
    fn host_error(&self) -> HostError {
        match self {
            Self::InvalidRequest(message) => {
                HostError::new("invalid_request", message.clone(), false)
            }
            Self::Storage(error) => {
                HostError::new(error.code(), error.to_string(), error.retryable())
            }
            Self::InvalidRunState(message) => {
                HostError::new("invalid_run_state", message.clone(), false)
            }
            Self::Clock(message) => HostError::new("clock_unavailable", message.clone(), true),
        }
    }
}

type RuntimeClock = Arc<dyn Fn() -> Result<i64, LocalAgentRuntimeError> + Send + Sync>;

pub struct LocalAgentRuntime {
    store: Arc<dyn LocalAgentStore>,
    clock: RuntimeClock,
    recovered_claims: AtomicU64,
}

impl LocalAgentRuntime {
    pub fn new(store: Arc<dyn LocalAgentStore>) -> Self {
        Self::with_clock(store, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock(store: Arc<dyn LocalAgentStore>, clock: RuntimeClock) -> Self {
        Self {
            store,
            clock,
            recovered_claims: AtomicU64::new(0),
        }
    }

    pub async fn initialize(&self) -> Result<u64, LocalAgentRuntimeError> {
        self.store.health_check().await?;
        let now = self.now()?;
        let recovered_runs = self.store.recover_expired_claims(now).await?;
        let recovered_tools = self.store.recover_expired_tool_claims(now).await?;
        let recovered = recovered_runs.saturating_add(recovered_tools);
        self.recovered_claims.store(recovered, Ordering::Release);
        Ok(recovered)
    }

    pub async fn handle(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
        let command_id = request.command_id.clone();
        let response = self.try_handle(request).await;
        match response {
            Ok(result) => HostResponseEnvelope::success(command_id, result),
            Err(error) => HostResponseEnvelope::failure(command_id, error.host_error()),
        }
    }

    pub async fn next_retry_at(&self) -> Result<Option<i64>, LocalAgentRuntimeError> {
        Ok(self.store.next_retry_at().await?)
    }

    pub async fn try_handle(
        &self,
        request: HostRequestEnvelope,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        request
            .validate()
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        let idempotency = IdempotentCommand {
            command_id: request.command_id,
            request_fingerprint: serde_json::to_string(&request.command)
                .map_err(|error| LocalAgentRuntimeError::InvalidRequest(error.to_string()))?,
        };
        match request.command {
            HostCommand::Health => {
                self.store.health_check().await?;
                Ok(HostResult::Health {
                    service: "chatos-local-agent-host".to_string(),
                    storage_ready: true,
                    recovered_claims: self.recovered_claims.load(Ordering::Acquire),
                })
            }
            HostCommand::CreateRun(command) => {
                let now = self.now()?;
                let run = create_run_record(command, now);
                let created = self
                    .store
                    .create_run(&idempotency, &run, &new_event_id())
                    .await?;
                Ok(HostResult::Run { run: created })
            }
            HostCommand::GetRun { run_id } => {
                let run = self
                    .store
                    .get_run(&run_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(run_id))?;
                Ok(HostResult::Run { run })
            }
            HostCommand::ClaimNextRun(command) => {
                let now = self.now()?;
                let lease_duration = i64::try_from(command.lease_duration_ms).map_err(|_| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "lease_duration_ms is too large".to_string(),
                    )
                })?;
                let claim_until = now.checked_add(lease_duration).ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest("claim lease overflow".to_string())
                })?;
                let claim = self
                    .store
                    .claim_next_run(
                        &idempotency,
                        &command.worker_id,
                        &Uuid::new_v4().to_string(),
                        now,
                        claim_until,
                        &new_event_id(),
                    )
                    .await?;
                Ok(HostResult::Claim { claim })
            }
            HostCommand::CommitStep(command) => {
                let current = self
                    .store
                    .get_run(&command.run_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.run_id.clone()))?;
                let now = self.now()?;
                let transition = transition_for_outcome(
                    &current,
                    command.claim_token,
                    command.expected_version,
                    command.outcome,
                    now,
                )?;
                let run = self
                    .store
                    .apply_transition(&idempotency, &transition)
                    .await?;
                Ok(HostResult::Run { run })
            }
            HostCommand::ClaimNextTool(command) => {
                let now = self.now()?;
                let lease_duration = i64::try_from(command.lease_duration_ms).map_err(|_| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "lease_duration_ms is too large".to_string(),
                    )
                })?;
                let claim_until = now.checked_add(lease_duration).ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest("claim lease overflow".to_string())
                })?;
                let claim = self
                    .store
                    .claim_next_tool(
                        &idempotency,
                        &command.worker_id,
                        &Uuid::new_v4().to_string(),
                        now,
                        claim_until,
                        &new_event_id(),
                        command.include_tool_names.as_deref(),
                        &command.exclude_tool_names,
                    )
                    .await?;
                Ok(HostResult::ToolClaim { claim })
            }
            HostCommand::CommitTool(command) => {
                let result = self
                    .store
                    .commit_tool(
                        &idempotency,
                        &command.invocation_id,
                        &command.claim_token,
                        command.expected_version,
                        &command.outcome,
                        &new_event_id(),
                        &new_event_id(),
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::ToolCommit {
                    result: Box::new(result),
                })
            }
            HostCommand::ResumeRun(command) => {
                let continuation_input = json!({
                    "type": "resume",
                    "reason": command.reason,
                    "input": command.input
                });
                let run = self
                    .store
                    .resume_run(
                        &idempotency,
                        &command.run_id,
                        command.expected_version,
                        command.expected_status,
                        &continuation_input,
                        &new_event_id(),
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::Run { run })
            }
            HostCommand::CancelRun(command) => {
                let run = self
                    .store
                    .cancel_run(
                        &idempotency,
                        &command.run_id,
                        command.expected_version,
                        &command.reason,
                        &new_event_id(),
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::Run { run })
            }
            HostCommand::ListEvents(command) => {
                let events = self
                    .store
                    .list_events(
                        command.after_cursor,
                        command.limit,
                        command.run_id.as_deref(),
                    )
                    .await?;
                let next_cursor = events
                    .last()
                    .map(|event| event.cursor)
                    .unwrap_or(command.after_cursor);
                Ok(HostResult::Events {
                    events,
                    next_cursor,
                })
            }
            HostCommand::WaitEvents(command) => {
                let events = self
                    .store
                    .list_events(
                        command.after_cursor,
                        command.limit,
                        command.run_id.as_deref(),
                    )
                    .await?;
                let next_cursor = events
                    .last()
                    .map(|event| event.cursor)
                    .unwrap_or(command.after_cursor);
                Ok(HostResult::Events {
                    events,
                    next_cursor,
                })
            }
            HostCommand::CreateTaskGraph(command) => Ok(HostResult::TaskGraph {
                graph: self.create_task_graph(&idempotency, command).await?,
            }),
            HostCommand::GetTaskGraph(command) => Ok(HostResult::TaskGraph {
                graph: self.get_task_graph(&command.graph_id).await?,
            }),
            HostCommand::GetTaskRuns(command) => {
                let task_id = command.task_id.clone();
                let runs = self.get_task_runs(command).await?;
                Ok(HostResult::TaskRuns { task_id, runs })
            }
            HostCommand::CancelTask(command) => Ok(HostResult::TaskGraph {
                graph: self.cancel_task(&idempotency, command).await?,
            }),
            HostCommand::RetryTask(command) => Ok(HostResult::TaskGraph {
                graph: self.retry_task(&idempotency, command).await?,
            }),
        }
    }

    fn now(&self) -> Result<i64, LocalAgentRuntimeError> {
        (self.clock)()
    }
}

fn create_run_record(command: CreateRunCommand, now: i64) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: command.run_id,
        owner_user_id: command.owner_user_id,
        owner_entity_type: command.owner_entity_type,
        owner_entity_id: command.owner_entity_id,
        profile_key: command.profile_key,
        model_config_ref: command.model_config_ref,
        model_config_revision: command.model_config_revision,
        capability_policy_revision: command.capability_policy_revision,
        input: command.input,
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: command.max_iterations,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: serde_json::Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    }
}

fn transition_for_outcome(
    run: &LocalAgentRunRecord,
    claim_token: String,
    expected_version: u64,
    outcome: LocalAgentStepOutcome,
    now: i64,
) -> Result<RunTransition, LocalAgentRuntimeError> {
    let tool_batch = match &outcome {
        LocalAgentStepOutcome::WaitForTool {
            batch_id,
            tool_calls,
            ..
        } => Some(LocalAgentToolBatch {
            batch_id: batch_id.clone(),
            calls: tool_calls.clone(),
        }),
        _ => None,
    };
    let checkpoint = match &outcome {
        LocalAgentStepOutcome::Continue { checkpoint }
        | LocalAgentStepOutcome::WaitForTool { checkpoint, .. }
        | LocalAgentStepOutcome::WaitForUser { checkpoint, .. } => Some(checkpoint.clone()),
        _ => None,
    };
    let next_model_attempt = match &outcome {
        LocalAgentStepOutcome::Retry {
            next_model_attempt, ..
        } => *next_model_attempt,
        _ => 1,
    };
    let clear_continuation_input = !matches!(&outcome, LocalAgentStepOutcome::Retry { .. });
    let (next_status, next_attempt, pending_tool_batch, terminal_outcome, event_type, payload) =
        match outcome {
            LocalAgentStepOutcome::Continue { checkpoint }
                if run.iteration >= run.max_iterations =>
            {
                (
                    LocalAgentRunStatus::NeedsReview,
                    None,
                    None,
                    None,
                    "iteration_limit_reached",
                    json!({
                        "reason": "maximum model iterations reached",
                        "checkpoint": checkpoint
                    }),
                )
            }
            LocalAgentStepOutcome::Continue { checkpoint } => (
                LocalAgentRunStatus::ContinuationReady,
                None,
                None,
                None,
                "continuation_requested",
                json!({"checkpoint": checkpoint}),
            ),
            LocalAgentStepOutcome::WaitForTool {
                batch_id,
                tool_calls,
                ..
            } => {
                let batch = json!({"batch_id": batch_id, "tool_calls": tool_calls});
                (
                    LocalAgentRunStatus::WaitingToolResult,
                    None,
                    Some(batch.clone()),
                    None,
                    "tool_batch_requested",
                    batch,
                )
            }
            LocalAgentStepOutcome::WaitForUser { prompt, .. } => (
                LocalAgentRunStatus::WaitingUser,
                None,
                None,
                None,
                "user_input_requested",
                json!({"prompt": prompt}),
            ),
            LocalAgentStepOutcome::Retry {
                resume_at_unix_ms,
                next_model_attempt,
                reason,
            } => (
                LocalAgentRunStatus::RetryScheduled,
                Some(resume_at_unix_ms),
                None,
                None,
                "retry_scheduled",
                json!({
                    "resume_at_unix_ms": resume_at_unix_ms,
                    "next_model_attempt": next_model_attempt,
                    "reason": reason
                }),
            ),
            LocalAgentStepOutcome::Pause { reason } => (
                LocalAgentRunStatus::Paused,
                None,
                None,
                None,
                "run_paused",
                json!({"reason": reason}),
            ),
            LocalAgentStepOutcome::NeedsReview { reason, detail } => (
                LocalAgentRunStatus::NeedsReview,
                None,
                None,
                None,
                "run_needs_review",
                json!({"reason": reason, "detail": detail}),
            ),
            LocalAgentStepOutcome::Succeed { output } => (
                LocalAgentRunStatus::Succeeded,
                None,
                None,
                Some(output.clone()),
                "run_succeeded",
                json!({"output": output}),
            ),
            LocalAgentStepOutcome::Fail { error, detail } => {
                let terminal = json!({"error": error, "detail": detail});
                (
                    LocalAgentRunStatus::Failed,
                    None,
                    None,
                    Some(terminal.clone()),
                    "run_failed",
                    terminal,
                )
            }
        };
    Ok(RunTransition {
        run_id: run.run_id.clone(),
        claim_token,
        expected_version,
        expected_status: LocalAgentRunStatus::ModelRunning,
        next_status,
        next_model_attempt,
        next_attempt_at_unix_ms: next_attempt,
        pending_tool_batch,
        tool_batch,
        checkpoint,
        clear_continuation_input,
        terminal_outcome,
        event_id: new_event_id(),
        event_type: event_type.to_string(),
        event_payload: payload,
        occurred_at_unix_ms: now,
    })
}

fn new_event_id() -> String {
    Uuid::new_v4().to_string()
}

fn system_now_unix_ms() -> Result<i64, LocalAgentRuntimeError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| LocalAgentRuntimeError::Clock(error.to_string()))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| LocalAgentRuntimeError::Clock("Unix time overflow".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
        LocalAgentToolCall, LocalAgentToolOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use serde_json::Value;

    fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    fn create_command() -> HostCommand {
        HostCommand::CreateRun(CreateRunCommand {
            run_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: "conversation-1".to_string(),
            profile_key: "main_chat".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"message": "hello"}),
            max_iterations: 4,
        })
    }

    #[tokio::test]
    async fn runtime_drives_one_durable_step_and_replays_command() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");

        let created = runtime.handle(envelope("create-1", create_command())).await;
        assert!(created.ok);
        let replay = runtime.handle(envelope("create-1", create_command())).await;
        assert_eq!(created, replay);

        let claimed = runtime
            .handle(envelope(
                "claim-1",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    worker_id: "worker-1".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await;
        let claim = match claimed.result.expect("claim result") {
            HostResult::Claim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        let completed = runtime
            .handle(envelope(
                "commit-1",
                HostCommand::CommitStep(CommitStepCommand {
                    run_id: claim.run.run_id.clone(),
                    claim_token: claim.claim_token.clone(),
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::Succeed {
                        output: json!({"answer": 42}),
                    },
                }),
            ))
            .await;
        let completed_replay = runtime
            .handle(envelope(
                "commit-1",
                HostCommand::CommitStep(CommitStepCommand {
                    run_id: claim.run.run_id.clone(),
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::Succeed {
                        output: json!({"answer": 42}),
                    },
                }),
            ))
            .await;
        assert_eq!(completed, completed_replay);
        let run = match completed.result.expect("commit result") {
            HostResult::Run { run } => run,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(run.status, LocalAgentRunStatus::Succeeded);
        assert_eq!(run.terminal_outcome, Some(json!({"answer": 42})));
        assert_eq!(run.version, 3);
    }

    #[tokio::test]
    async fn iteration_limit_becomes_review_instead_of_success() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        let mut command = match create_command() {
            HostCommand::CreateRun(command) => command,
            _ => unreachable!(),
        };
        command.max_iterations = 1;
        runtime
            .handle(envelope("create-1", HostCommand::CreateRun(command)))
            .await;
        let claimed = runtime
            .handle(envelope(
                "claim-1",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    worker_id: "worker-1".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await;
        let claim = match claimed.result.expect("claim result") {
            HostResult::Claim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        let continued = runtime
            .handle(envelope(
                "commit-1",
                HostCommand::CommitStep(CommitStepCommand {
                    run_id: claim.run.run_id.clone(),
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::Continue {
                        checkpoint: Value::Null,
                    },
                }),
            ))
            .await;
        let run = match continued.result.expect("commit result") {
            HostResult::Run { run } => run,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(run.status, LocalAgentRunStatus::NeedsReview);
    }

    #[tokio::test]
    async fn tool_batch_is_durable_and_resumes_after_all_results() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        runtime.handle(envelope("create-1", create_command())).await;
        let claimed = runtime
            .handle(envelope(
                "claim-run-1",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await;
        let claim = match claimed.result.expect("claim result") {
            HostResult::Claim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        let waiting = runtime
            .handle(envelope(
                "commit-model-1",
                HostCommand::CommitStep(CommitStepCommand {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch-1".to_string(),
                        tool_calls: vec![
                            LocalAgentToolCall {
                                call_id: "call-read".to_string(),
                                tool_name: "read_file".to_string(),
                                arguments: json!({"path": "README.md"}),
                                side_effecting: false,
                            },
                            LocalAgentToolCall {
                                call_id: "call-write".to_string(),
                                tool_name: "write_file".to_string(),
                                arguments: json!({"path": "result.txt"}),
                                side_effecting: true,
                            },
                        ],
                        checkpoint: json!({"response_id": "response-1"}),
                    },
                }),
            ))
            .await;
        let waiting_run = match waiting.result.expect("waiting result") {
            HostResult::Run { run } => run,
            result => panic!("unexpected result: {result:?}"),
        };
        assert_eq!(waiting_run.status, LocalAgentRunStatus::WaitingToolResult);

        for index in 0..2 {
            let claimed = runtime
                .handle(envelope(
                    &format!("claim-tool-{index}"),
                    HostCommand::ClaimNextTool(ClaimNextToolCommand {
                        worker_id: "tool-worker".to_string(),
                        lease_duration_ms: 10_000,
                        include_tool_names: None,
                        exclude_tool_names: Vec::new(),
                    }),
                ))
                .await;
            let claim = match claimed.result.expect("tool claim result") {
                HostResult::ToolClaim { claim: Some(claim) } => claim,
                result => panic!("unexpected result: {result:?}"),
            };
            let outcome = if claim.invocation.tool_name == "read_file" {
                LocalAgentToolOutcome::Succeeded {
                    output: json!({"content": "ok"}),
                }
            } else {
                LocalAgentToolOutcome::Failed {
                    error: "write rejected".to_string(),
                    detail: json!({"code": "permission_denied"}),
                }
            };
            let committed = runtime
                .handle(envelope(
                    &format!("commit-tool-{index}"),
                    HostCommand::CommitTool(CommitToolCommand {
                        invocation_id: claim.invocation.invocation_id,
                        claim_token: claim.claim_token,
                        expected_version: claim.invocation.version,
                        outcome,
                    }),
                ))
                .await;
            let result = match committed.result.expect("tool commit result") {
                HostResult::ToolCommit { result } => result,
                result => panic!("unexpected result: {result:?}"),
            };
            if index == 0 {
                assert_eq!(result.run.status, LocalAgentRunStatus::WaitingToolResult);
            } else {
                assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
                assert!(result.run.pending_tool_batch.is_none());
                assert_eq!(result.run.checkpoint, json!({"response_id": "response-1"}));
                assert_eq!(
                    result
                        .run
                        .continuation_input
                        .as_ref()
                        .and_then(|value| value.get("type"))
                        .and_then(serde_json::Value::as_str),
                    Some("tool_results")
                );
            }
        }

        let next = runtime
            .handle(envelope(
                "claim-run-2",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await;
        assert!(matches!(
            next.result,
            Some(HostResult::Claim { claim: Some(_) })
        ));
    }
}

#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod tool_tests;
