// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    HostRequestHandler, LocalAgentScheduler, LocalAgentSchedulerError, LocalMemorySyncError,
    LocalMemorySyncWorker, LocalToolScheduler, LocalToolSchedulerError, MemorySyncTick,
    SchedulerTick, ToolSchedulerTick,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_identifier, HostCommand, HostError, HostRequestEnvelope, HostResponseEnvelope,
    HostResult,
};
use chatos_local_agent_runtime::{LocalAgentRuntime, LocalAgentRuntimeError};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{watch, Notify};

#[derive(Debug, Error)]
pub enum LocalAgentCoordinatorError {
    #[error(transparent)]
    ModelScheduler(#[from] LocalAgentSchedulerError),
    #[error(transparent)]
    ToolScheduler(#[from] LocalToolSchedulerError),
    #[error(transparent)]
    Runtime(#[from] LocalAgentRuntimeError),
    #[error(transparent)]
    MemorySync(#[from] LocalMemorySyncError),
    #[error("system clock is before the Unix epoch")]
    Clock,
}

pub struct LocalAgentHostCoordinator {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
    model_scheduler: Option<LocalAgentScheduler>,
    tool_scheduler: Option<LocalToolScheduler>,
    memory_sync_worker: Option<LocalMemorySyncWorker>,
    wakeup: Arc<Notify>,
    activity: watch::Sender<u64>,
    reserved_ipc_tools: Vec<String>,
}

impl LocalAgentHostCoordinator {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
        model_scheduler: Option<LocalAgentScheduler>,
        tool_scheduler: Option<LocalToolScheduler>,
    ) -> Result<Self, String> {
        if model_scheduler.is_none() && tool_scheduler.is_none() {
            return Err("Local Agent coordinator requires a model or tool scheduler".to_string());
        }
        let owner_user_id = owner_user_id.into();
        validate_identifier("owner_user_id", &owner_user_id)?;
        if model_scheduler
            .as_ref()
            .is_some_and(|scheduler| scheduler.owner_user_id() != owner_user_id)
            || tool_scheduler
                .as_ref()
                .is_some_and(|scheduler| scheduler.owner_user_id() != owner_user_id)
        {
            return Err(
                "Local Agent coordinator and schedulers must use the same owner".to_string(),
            );
        }
        let (activity, _) = watch::channel(0);
        Ok(Self {
            runtime,
            owner_user_id,
            model_scheduler,
            tool_scheduler,
            memory_sync_worker: None,
            wakeup: Arc::new(Notify::new()),
            activity,
            reserved_ipc_tools: Vec::new(),
        })
    }

    pub fn with_memory_sync_worker(
        mut self,
        worker: LocalMemorySyncWorker,
    ) -> Result<Self, String> {
        if worker.tenant_id() != self.owner_user_id {
            return Err(
                "Local Agent coordinator and Memory worker must use the same owner".to_string(),
            );
        }
        self.memory_sync_worker = Some(worker);
        Ok(self)
    }

    pub fn with_reserved_ipc_tools<I, S>(mut self, tool_names: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut names = Vec::new();
        for tool_name in tool_names {
            let tool_name = tool_name.into();
            validate_identifier("reserved_ipc_tool", &tool_name)?;
            if names.contains(&tool_name) {
                return Err(format!("reserved IPC tool is duplicated: {tool_name}"));
            }
            names.push(tool_name);
        }
        if names.len() > 128 {
            return Err("at most 128 IPC tools can be reserved".to_string());
        }
        self.reserved_ipc_tools = names;
        Ok(self)
    }

    pub fn wake(&self) {
        self.wakeup.notify_one();
    }

    pub async fn run_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let mut failure_backoff = Duration::from_millis(100);
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            if self.drain_ready_work(&shutdown).await.is_err() {
                tokio::select! {
                    _ = self.wakeup.notified() => {}
                    _ = tokio::time::sleep(failure_backoff) => {}
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return Ok(());
                        }
                    }
                }
                failure_backoff = (failure_backoff * 2).min(Duration::from_secs(5));
                continue;
            }
            failure_backoff = Duration::from_millis(100);
            if *shutdown.borrow() {
                return Ok(());
            }
            let delay = match self.next_retry_delay().await {
                Ok(delay) => delay,
                Err(_) => Duration::from_secs(1),
            };
            tokio::select! {
                _ = self.wakeup.notified() => {}
                _ = tokio::time::sleep(delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn drain_ready_work(
        &self,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let mut sync_memory = true;
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            let mut progressed = false;
            if let Some(scheduler) = self.model_scheduler.as_ref() {
                let committed = matches!(scheduler.run_once().await?, SchedulerTick::Committed(_));
                if committed {
                    self.signal_activity();
                }
                progressed |= committed;
            }
            if *shutdown.borrow() {
                return Ok(());
            }
            if let Some(scheduler) = self.tool_scheduler.as_ref() {
                let committed =
                    matches!(scheduler.run_once().await?, ToolSchedulerTick::Committed(_));
                if committed {
                    self.signal_activity();
                }
                progressed |= committed;
            }
            if sync_memory {
                if let Some(worker) = self.memory_sync_worker.as_ref() {
                    match worker.run_once().await? {
                        MemorySyncTick::Idle => {}
                        MemorySyncTick::Synced { .. } => {
                            self.signal_activity();
                            progressed = true;
                        }
                        MemorySyncTick::RetryScheduled { .. } => {
                            self.signal_activity();
                            sync_memory = false;
                        }
                    }
                }
            }
            if !progressed {
                return Ok(());
            }
        }
    }

    async fn next_retry_delay(&self) -> Result<Duration, LocalAgentCoordinatorError> {
        let run_retry_at = self.runtime.next_retry_at(&self.owner_user_id).await?;
        let memory_retry_at = match self.memory_sync_worker.as_ref() {
            Some(worker) => worker.next_retry_at().await?,
            None => None,
        };
        let next_retry_at = match (run_retry_at, memory_retry_at) {
            (Some(run), Some(memory)) => Some(run.min(memory)),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
        let Some(next_retry_at) = next_retry_at else {
            return Ok(Duration::from_secs(30));
        };
        let now = system_now_unix_ms()?;
        Ok(Duration::from_millis(
            u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
        )
        .min(Duration::from_secs(30)))
    }

    fn signal_activity(&self) {
        self.activity
            .send_modify(|value| *value = value.wrapping_add(1));
    }

    async fn wait_for_events(
        &self,
        request: HostRequestEnvelope,
        timeout_ms: u64,
    ) -> HostResponseEnvelope {
        let mut activity = self.activity.subscribe();
        let mut response = self.runtime.handle(request.clone()).await;
        if !response.ok || response_has_events(&response) {
            return response;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    // A recovery transaction can persist events without an in-process activity
                    // signal. Always perform one final durable read at the timeout boundary.
                    return self.runtime.handle(request.clone()).await;
                },
                changed = activity.changed() => {
                    if changed.is_err() {
                        return response;
                    }
                    response = self.runtime.handle(request.clone()).await;
                    if !response.ok || response_has_events(&response) {
                        return response;
                    }
                }
            }
        }
    }

    fn route_external_tool_claim(&self, request: &mut HostRequestEnvelope) {
        let HostCommand::ClaimNextTool(command) = &mut request.command else {
            return;
        };
        for tool_name in &self.reserved_ipc_tools {
            if !command.exclude_tool_names.contains(tool_name) {
                command.exclude_tool_names.push(tool_name.clone());
            }
        }
    }

    async fn reject_external_reserved_tool_commit(
        &self,
        request: &HostRequestEnvelope,
    ) -> Option<HostResponseEnvelope> {
        let HostCommand::CommitTool(command) = &request.command else {
            return None;
        };
        let invocation = match self
            .runtime
            .get_tool_invocation_for_host_worker(&command.invocation_id)
            .await
        {
            Ok(invocation) => invocation,
            Err(error) => {
                return Some(HostResponseEnvelope::failure(
                    request.command_id.clone(),
                    HostError::new("host_internal_error", error.to_string(), true),
                ));
            }
        };
        invocation
            .filter(|invocation| self.reserved_ipc_tools.contains(&invocation.tool_name))
            .map(|_| {
                HostResponseEnvelope::failure(
                    request.command_id.clone(),
                    HostError::new(
                        "reserved_command",
                        "Host-owned tool results can only be committed by the built-in local worker",
                        false,
                    ),
                )
            })
    }
}

#[async_trait]
impl HostRequestHandler for LocalAgentHostCoordinator {
    async fn handle_request(&self, mut request: HostRequestEnvelope) -> HostResponseEnvelope {
        if request
            .command
            .owner_user_id()
            .is_some_and(|owner| owner != self.owner_user_id)
        {
            return HostResponseEnvelope::failure(
                request.command_id,
                HostError::new(
                    "account_mismatch",
                    "request account does not match the active Local Agent Host account",
                    false,
                ),
            );
        }
        if matches!(request.command, HostCommand::CreateRequirementSurvey(_)) {
            return HostResponseEnvelope::failure(
                request.command_id,
                HostError::new(
                    "reserved_command",
                    "requirement surveys can only be created by the built-in local tool",
                    false,
                ),
            );
        }
        if matches!(
            request.command,
            HostCommand::ClaimNextRun(_) | HostCommand::CommitStep(_)
        ) {
            return HostResponseEnvelope::failure(
                request.command_id,
                HostError::new(
                    "reserved_command",
                    "model Run claims and commits are owned by the built-in local worker",
                    false,
                ),
            );
        }
        if matches!(
            request.command,
            HostCommand::CreateRun(_) | HostCommand::CreateTaskGraph(_)
        ) {
            return HostResponseEnvelope::failure(
                request.command_id,
                HostError::new(
                    "reserved_command",
                    "Runs and Task Graphs can only be created by built-in Host workflows",
                    false,
                ),
            );
        }
        if let Some(response) = self.reject_external_reserved_tool_commit(&request).await {
            return response;
        }
        if let HostCommand::WaitEvents(command) = &request.command {
            return self
                .wait_for_events(request.clone(), command.timeout_ms)
                .await;
        }
        let wakes_scheduler = command_wakes_scheduler(&request.command);
        let activity_policy = command_activity_signal_policy(&request.command);
        self.route_external_tool_claim(&mut request);
        let response = self.runtime.handle(request).await;
        if response.ok {
            if wakes_scheduler {
                self.wake();
            }
            if response_signals_activity(activity_policy, &response) {
                self.signal_activity();
            }
        }
        response
    }
}

fn command_wakes_scheduler(command: &HostCommand) -> bool {
    match command {
        HostCommand::CreateRun(_)
        | HostCommand::CommitTool(_)
        | HostCommand::DecideToolApproval(_)
        | HostCommand::ResumeRun(_)
        | HostCommand::CancelRun(_)
        | HostCommand::CreateTaskGraph(_)
        | HostCommand::CancelTask(_)
        | HostCommand::RetryTask(_)
        | HostCommand::RestartTask(_)
        | HostCommand::StartConversationTurn(_)
        | HostCommand::GuideConversationTurn(_)
        | HostCommand::ResumeConversationTurn(_)
        | HostCommand::CancelConversationTurn(_)
        | HostCommand::ResolveRequirementSurvey(_) => true,
        HostCommand::Health
        | HostCommand::GetMemorySyncStatus(_)
        | HostCommand::PutModelConfigSnapshot(_)
        | HostCommand::GetModelConfigSnapshot(_)
        | HostCommand::PutCapabilityPolicySnapshot(_)
        | HostCommand::GetCapabilityPolicySnapshot(_)
        | HostCommand::GetRun(_)
        | HostCommand::ListRuns(_)
        | HostCommand::ClaimNextRun(_)
        | HostCommand::CommitStep(_)
        | HostCommand::ClaimNextTool(_)
        | HostCommand::RenewToolClaim(_)
        | HostCommand::ListPendingToolApprovals(_)
        | HostCommand::GetEventCursor(_)
        | HostCommand::ListEvents(_)
        | HostCommand::WaitEvents(_)
        | HostCommand::ListTaskGraphs(_)
        | HostCommand::GetTaskGraph(_)
        | HostCommand::GetTaskRuns(_)
        | HostCommand::PutPluginInstallation(_)
        | HostCommand::GetPluginInstallation(_)
        | HostCommand::ListPluginInstallations(_)
        | HostCommand::RemovePluginInstallation(_)
        | HostCommand::CreateConversation(_)
        | HostCommand::GetConversation(_)
        | HostCommand::GetConversationHistory(_)
        | HostCommand::ListConversations(_)
        | HostCommand::GetConversationRuntimeSettings(_)
        | HostCommand::PutConversationRuntimeSettings(_)
        | HostCommand::InitializeNotepad(_)
        | HostCommand::ListNotepadFolders(_)
        | HostCommand::CreateNotepadFolder(_)
        | HostCommand::RenameNotepadFolder(_)
        | HostCommand::DeleteNotepadFolder(_)
        | HostCommand::ListNotepadNotes(_)
        | HostCommand::CreateNotepadNote(_)
        | HostCommand::GetNotepadNote(_)
        | HostCommand::UpdateNotepadNote(_)
        | HostCommand::DeleteNotepadNote(_)
        | HostCommand::PutNotepadImage(_)
        | HostCommand::ListRemoteConnections(_)
        | HostCommand::GetRemoteConnection(_)
        | HostCommand::CreateRemoteConnection(_)
        | HostCommand::UpdateRemoteConnection(_)
        | HostCommand::DeleteRemoteConnection(_)
        | HostCommand::CreateArtifact(_)
        | HostCommand::ListArtifacts(_)
        | HostCommand::GetArtifactData(_)
        | HostCommand::DeleteArtifact(_)
        | HostCommand::CreateRequirementSurvey(_)
        | HostCommand::ListRequirementSurveys(_)
        | HostCommand::GetRequirementSurvey(_) => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivitySignalPolicy {
    Never,
    Always,
    ToolClaimed,
}

fn command_activity_signal_policy(command: &HostCommand) -> ActivitySignalPolicy {
    if command_wakes_scheduler(command) {
        ActivitySignalPolicy::Always
    } else if matches!(command, HostCommand::ClaimNextTool(_)) {
        ActivitySignalPolicy::ToolClaimed
    } else {
        ActivitySignalPolicy::Never
    }
}

fn response_signals_activity(
    policy: ActivitySignalPolicy,
    response: &HostResponseEnvelope,
) -> bool {
    match policy {
        ActivitySignalPolicy::Never => false,
        ActivitySignalPolicy::Always => true,
        ActivitySignalPolicy::ToolClaimed => matches!(
            response.result.as_ref(),
            Some(HostResult::ToolClaim { claim: Some(_) })
        ),
    }
}

fn response_has_events(response: &HostResponseEnvelope) -> bool {
    matches!(
        response.result.as_ref(),
        Some(HostResult::Events { events, .. }) if !events.is_empty()
    )
}

fn system_now_unix_ms() -> Result<i64, LocalAgentCoordinatorError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| LocalAgentCoordinatorError::Clock)?;
    i64::try_from(duration.as_millis()).map_err(|_| LocalAgentCoordinatorError::Clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        decode_response, read_frame, serve_stream, write_frame, LocalToolExecutor,
        LocalToolRegistry,
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
    use uuid::Uuid;

    struct ModelProfile;

    #[test]
    fn read_only_ipc_does_not_wake_scheduler_or_signal_activity() {
        let commands = [
            HostCommand::Health,
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
            }),
        };
        coordinator.route_external_tool_claim(&mut external_claim);
        let HostCommand::ClaimNextTool(routed) = external_claim.command else {
            panic!("expected tool claim")
        };
        assert_eq!(routed.exclude_tool_names, vec!["create_task"]);
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
                            payload_mode:
                                chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
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
}
