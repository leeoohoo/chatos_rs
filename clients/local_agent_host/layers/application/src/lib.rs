// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Durable, client-owned Local Agent state machine for runs, leases, transitions, and events.

use chatos_local_agent_ports::{
    ClientStorageError, IdempotentCommand, LocalAgentStore, RunTransition,
};
use chatos_local_agent_protocol::{
    validate_identifier, HostCommand, HostError, HostRequestEnvelope, HostResponseEnvelope,
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

mod artifact_runtime;
mod control_plane_runtime;
#[cfg(test)]
mod conversation_query_tests;
mod conversation_runtime;
mod conversation_settings_runtime;
mod host_worker;
mod message_task_graph;
#[cfg(test)]
mod message_task_graph_tests;
mod notepad_runtime;
#[cfg(test)]
mod plugin_query_tests;
mod plugin_runtime;
mod profile;
mod remote_connection_runtime;
mod requirement_survey_runtime;
#[cfg(test)]
mod requirement_survey_tests;
mod run_factory;
#[cfg(test)]
mod task_query_tests;
mod task_runtime;

pub use profile::{LocalAgentProfile, LocalAgentProfileRegistry};
use run_factory::create_run_record;

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

    pub async fn initialize(&self, owner_user_id: &str) -> Result<u64, LocalAgentRuntimeError> {
        validate_identifier("owner_user_id", owner_user_id)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        self.store.health_check().await?;
        let now = self.now()?;
        let recovered_runs = self
            .store
            .recover_expired_claims(owner_user_id, now)
            .await?;
        let recovered_tools = self
            .store
            .recover_expired_tool_claims(owner_user_id, now)
            .await?;
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

    pub async fn next_retry_at(
        &self,
        owner_user_id: &str,
    ) -> Result<Option<i64>, LocalAgentRuntimeError> {
        validate_identifier("owner_user_id", owner_user_id)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        Ok(self.store.next_retry_at(owner_user_id).await?)
    }

    pub async fn renew_run_claim(
        &self,
        owner_user_id: &str,
        run_id: &str,
        claim_token: &str,
        expected_version: u64,
        lease_duration_ms: u64,
    ) -> Result<bool, LocalAgentRuntimeError> {
        validate_identifier("owner_user_id", owner_user_id)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        validate_identifier("run_id", run_id).map_err(LocalAgentRuntimeError::InvalidRequest)?;
        validate_identifier("claim_token", claim_token)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        let (now, claim_until) = self.claim_renewal_deadline(lease_duration_ms)?;
        Ok(self
            .store
            .renew_run_claim(
                owner_user_id,
                run_id,
                claim_token,
                expected_version,
                now,
                claim_until,
            )
            .await?)
    }

    pub async fn renew_tool_claim(
        &self,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        lease_duration_ms: u64,
    ) -> Result<bool, LocalAgentRuntimeError> {
        validate_identifier("owner_user_id", owner_user_id)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        validate_identifier("invocation_id", invocation_id)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        validate_identifier("claim_token", claim_token)
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        let (now, claim_until) = self.claim_renewal_deadline(lease_duration_ms)?;
        Ok(self
            .store
            .renew_tool_claim(
                owner_user_id,
                invocation_id,
                claim_token,
                expected_version,
                now,
                claim_until,
            )
            .await?)
    }

    pub async fn try_handle(
        &self,
        request: HostRequestEnvelope,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        self.try_handle_with_receipt_policy(request, true).await
    }

    /// Executes a trusted, one-shot in-process scheduler command without storing
    /// a replay receipt. IPC callers must continue to use `try_handle`.
    pub async fn try_handle_ephemeral(
        &self,
        request: HostRequestEnvelope,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        self.try_handle_with_receipt_policy(request, false).await
    }

    async fn try_handle_with_receipt_policy(
        &self,
        request: HostRequestEnvelope,
        persist_receipt: bool,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        request
            .validate()
            .map_err(LocalAgentRuntimeError::InvalidRequest)?;
        let idempotency = IdempotentCommand {
            command_id: request.command_id,
            request_fingerprint: serde_json::to_string(&request.command)
                .map_err(|error| LocalAgentRuntimeError::InvalidRequest(error.to_string()))?,
            persist_receipt,
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
            HostCommand::GetMemorySyncStatus(command) => {
                let status = self
                    .store
                    .get_memory_sync_status(&command.tenant_id, &command.source_id)
                    .await?;
                Ok(HostResult::MemorySyncStatus { status })
            }
            command @ (HostCommand::PutModelConfigSnapshot(_)
            | HostCommand::GetModelConfigSnapshot(_)
            | HostCommand::ListLatestModelConfigSnapshots(_)
            | HostCommand::PutCapabilityPolicySnapshot(_)
            | HostCommand::GetCapabilityPolicySnapshot(_)
            | HostCommand::GetLatestCapabilityPolicySnapshot(_)) => {
                self.handle_control_plane_command(&idempotency, command)
                    .await
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
            HostCommand::GetRun(command) => {
                let run = self
                    .store
                    .get_run_for_owner(&command.owner_user_id, &command.run_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.run_id))?;
                Ok(HostResult::Run { run })
            }
            HostCommand::ListRuns(command) => {
                let page = self
                    .store
                    .list_runs(
                        &command.owner_user_id,
                        command.scope,
                        command.status,
                        command.updated_after_unix_ms,
                        command.before_updated_at_unix_ms,
                        command.before_run_id.as_deref(),
                        command.limit,
                    )
                    .await?;
                Ok(HostResult::Runs { page })
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
                        &command.owner_user_id,
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
                    .get_run_for_owner(&command.owner_user_id, &command.run_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.run_id.clone()))?;
                if current.owner_entity_type == "task"
                    && current.profile_key == "task_execution"
                    && matches!(&command.outcome, LocalAgentStepOutcome::Succeed { .. })
                    && self
                        .store
                        .successful_tool_invocation_count(
                            &current.run_id,
                            "task_run_process_report_outcome",
                        )
                        .await?
                        == 0
                {
                    return Err(LocalAgentRuntimeError::InvalidRequest(
                        "Task execution must report its outcome before the final response"
                            .to_string(),
                    ));
                }
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
                        &command.owner_user_id,
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
            HostCommand::RenewToolClaim(command) => {
                let renewed = self
                    .renew_tool_claim(
                        &command.owner_user_id,
                        &command.invocation_id,
                        &command.claim_token,
                        command.expected_version,
                        command.lease_duration_ms,
                    )
                    .await?;
                Ok(HostResult::ToolClaimRenewed { renewed })
            }
            HostCommand::CommitTool(command) => {
                let result = self
                    .store
                    .commit_tool(
                        &idempotency,
                        &command.owner_user_id,
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
            HostCommand::ListPendingToolApprovals(command) => {
                let invocations = self
                    .store
                    .list_pending_tool_approvals(&command.owner_user_id, command.limit)
                    .await?;
                Ok(HostResult::PendingToolApprovals { invocations })
            }
            HostCommand::DecideToolApproval(command) => {
                let result = self
                    .store
                    .decide_tool_approval(
                        &idempotency,
                        &command.owner_user_id,
                        &command.invocation_id,
                        command.expected_version,
                        command.decision,
                        &command.decided_by,
                        &command.reason,
                        &new_event_id(),
                        &new_event_id(),
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::ToolApproval {
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
                    .resume_run_for_owner(
                        &idempotency,
                        &command.owner_user_id,
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
                    .cancel_run_for_owner(
                        &idempotency,
                        &command.owner_user_id,
                        &command.run_id,
                        command.expected_version,
                        &command.reason,
                        &new_event_id(),
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::Run { run })
            }
            HostCommand::GetEventCursor(command) => {
                let cursor = self
                    .store
                    .latest_event_cursor_for_owner(&command.owner_user_id)
                    .await?;
                Ok(HostResult::EventCursor { cursor })
            }
            HostCommand::ListEvents(command) => {
                let events = self
                    .store
                    .list_events_for_owner(
                        &command.owner_user_id,
                        command.after_cursor,
                        command.limit,
                        command.run_id.as_deref(),
                        command.event_type.as_deref(),
                        command.newest_first,
                        command.payload_mode,
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
                    .list_events_for_owner(
                        &command.owner_user_id,
                        command.after_cursor,
                        command.limit,
                        command.run_id.as_deref(),
                        None,
                        false,
                        command.payload_mode,
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
            command @ (HostCommand::CreateTaskGraph(_)
            | HostCommand::ListTaskGraphs(_)
            | HostCommand::GetTaskGraph(_)
            | HostCommand::GetMessageTaskGraph(_)
            | HostCommand::GetTaskRuns(_)
            | HostCommand::CancelTask(_)
            | HostCommand::RetryTask(_)
            | HostCommand::RestartTask(_)) => self.handle_task_command(&idempotency, command).await,
            command @ (HostCommand::PutPluginInstallation(_)
            | HostCommand::GetPluginInstallation(_)
            | HostCommand::ListPluginInstallations(_)
            | HostCommand::RemovePluginInstallation(_)) => {
                self.handle_plugin_command(&idempotency, command).await
            }
            command @ (HostCommand::CreateConversation(_)
            | HostCommand::GetConversation(_)
            | HostCommand::GetConversationHistory(_)
            | HostCommand::ListConversations(_)
            | HostCommand::StartConversationTurn(_)
            | HostCommand::GuideConversationTurn(_)
            | HostCommand::ResumeConversationTurn(_)
            | HostCommand::CancelConversationTurn(_)) => {
                self.handle_conversation_command(&idempotency, command)
                    .await
            }
            command @ (HostCommand::GetConversationRuntimeSettings(_)
            | HostCommand::PutConversationRuntimeSettings(_)) => {
                self.handle_conversation_settings_command(&idempotency, command)
                    .await
            }
            command @ (HostCommand::InitializeNotepad(_)
            | HostCommand::ListNotepadFolders(_)
            | HostCommand::CreateNotepadFolder(_)
            | HostCommand::RenameNotepadFolder(_)
            | HostCommand::DeleteNotepadFolder(_)
            | HostCommand::ListNotepadNotes(_)
            | HostCommand::CreateNotepadNote(_)
            | HostCommand::GetNotepadNote(_)
            | HostCommand::UpdateNotepadNote(_)
            | HostCommand::DeleteNotepadNote(_)
            | HostCommand::PutNotepadImage(_)) => {
                self.handle_notepad_command(&idempotency, command).await
            }
            command @ (HostCommand::ListRemoteConnections(_)
            | HostCommand::GetRemoteConnection(_)
            | HostCommand::CreateRemoteConnection(_)
            | HostCommand::UpdateRemoteConnection(_)
            | HostCommand::DeleteRemoteConnection(_)) => {
                self.handle_remote_connection_command(&idempotency, command)
                    .await
            }
            command @ (HostCommand::CreateArtifact(_)
            | HostCommand::ListArtifacts(_)
            | HostCommand::GetArtifactData(_)
            | HostCommand::DeleteArtifact(_)) => {
                self.handle_artifact_command(&idempotency, command).await
            }
            command @ (HostCommand::CreateRequirementSurvey(_)
            | HostCommand::ListRequirementSurveys(_)
            | HostCommand::GetRequirementSurvey(_)
            | HostCommand::ResolveRequirementSurvey(_)) => {
                self.handle_requirement_survey_command(&idempotency, command)
                    .await
            }
        }
    }

    fn now(&self) -> Result<i64, LocalAgentRuntimeError> {
        (self.clock)()
    }

    fn claim_renewal_deadline(
        &self,
        lease_duration_ms: u64,
    ) -> Result<(i64, i64), LocalAgentRuntimeError> {
        if !(1_000..=300_000).contains(&lease_duration_ms) {
            return Err(LocalAgentRuntimeError::InvalidRequest(
                "claim lease must be between 1000 and 300000 milliseconds".to_string(),
            ));
        }
        let now = self.now()?;
        let lease_duration = i64::try_from(lease_duration_ms).map_err(|_| {
            LocalAgentRuntimeError::InvalidRequest("claim lease is too large".to_string())
        })?;
        let claim_until = now.checked_add(lease_duration).ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRequest("claim lease overflow".to_string())
        })?;
        Ok((now, claim_until))
    }
}

fn transition_for_outcome(
    run: &LocalAgentRunRecord,
    claim_token: String,
    expected_version: u64,
    outcome: LocalAgentStepOutcome,
    now: i64,
) -> Result<RunTransition, LocalAgentRuntimeError> {
    requirement_survey_runtime::validate_tool_batch(&outcome)?;
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
mod runtime_tests;

#[cfg(test)]
mod memory_status_tests;
#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod run_account_tests;
#[cfg(test)]
mod run_query_tests;
#[cfg(test)]
mod runtime_tool_batch_tests;
#[cfg(test)]
mod tool_approval_tests;
#[cfg(test)]
mod tool_tests;
