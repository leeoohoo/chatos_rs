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
    memory_wakeup: Arc<Notify>,
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
            memory_wakeup: Arc::new(Notify::new()),
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
        self.memory_wakeup.notify_one();
    }

    pub async fn run_until_shutdown(
        &self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        if self.memory_sync_worker.is_none() {
            return self.run_schedulers_until_shutdown(shutdown).await;
        }
        let scheduler_loop = self.run_schedulers_until_shutdown(shutdown.clone());
        let memory_loop = self.run_memory_until_shutdown(shutdown);
        let (scheduler_result, memory_result) = tokio::join!(scheduler_loop, memory_loop);
        scheduler_result?;
        memory_result?;
        Ok(())
    }

    async fn run_schedulers_until_shutdown(
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
            let delay = match self.next_run_retry_delay().await {
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
            if !progressed {
                return Ok(());
            }
        }
    }

    async fn next_run_retry_delay(&self) -> Result<Duration, LocalAgentCoordinatorError> {
        let next_retry_at = self.runtime.next_retry_at(&self.owner_user_id).await?;
        let Some(next_retry_at) = next_retry_at else {
            return Ok(Duration::from_secs(30));
        };
        let now = system_now_unix_ms()?;
        Ok(Duration::from_millis(
            u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
        )
        .min(Duration::from_secs(30)))
    }

    async fn run_memory_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let Some(worker) = self.memory_sync_worker.as_ref() else {
            return Ok(());
        };
        let mut failure_backoff = Duration::from_millis(100);
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            match worker.run_once().await {
                Ok(MemorySyncTick::Synced { .. }) => {
                    self.signal_activity();
                    failure_backoff = Duration::from_millis(100);
                    continue;
                }
                Ok(MemorySyncTick::RetryScheduled { .. }) => {
                    self.signal_activity();
                    failure_backoff = Duration::from_millis(100);
                }
                Ok(MemorySyncTick::Idle) => {
                    failure_backoff = Duration::from_millis(100);
                }
                Err(_) => {
                    tokio::select! {
                        _ = self.memory_wakeup.notified() => {}
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
            }
            let delay = match worker.next_retry_at().await {
                Ok(Some(next_retry_at)) => {
                    let now = system_now_unix_ms()?;
                    Duration::from_millis(
                        u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
                    )
                    .min(Duration::from_secs(30))
                }
                Ok(None) => Duration::from_secs(30),
                Err(_) => Duration::from_secs(1),
            };
            tokio::select! {
                _ = self.memory_wakeup.notified() => {}
                _ = tokio::time::sleep(delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
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
        | HostCommand::GetMessageTaskGraph(_)
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
#[path = "coordinator_tests.rs"]
mod coordinator_tests;
