fn effective_worker_limit(
    configured: usize,
    pressure_level: LocalAgentResourcePressureLevel,
) -> usize {
    let effective = match pressure_level {
        LocalAgentResourcePressureLevel::Normal => configured,
        LocalAgentResourcePressureLevel::Elevated => configured.saturating_mul(3).div_ceil(4),
        LocalAgentResourcePressureLevel::Serious => configured.div_ceil(2),
        LocalAgentResourcePressureLevel::Critical => 1,
    };
    effective.clamp(MIN_CONCURRENT_WORK_ITEMS, configured)
}

fn default_maximum_concurrent_work_items() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| (parallelism.get() / 2).clamp(2, 8))
        .unwrap_or(2)
        .clamp(MIN_CONCURRENT_WORK_ITEMS, MAX_CONCURRENT_WORK_ITEMS)
}

#[async_trait]
impl HostRequestHandler for LocalAgentHostCoordinator {
    fn client_connected(&self) {
        if let Some(hooks) = self.hook_coordinator.as_ref() {
            hooks.client_connected();
        }
    }

    fn client_disconnected(&self) {
        if let Some(hooks) = self.hook_coordinator.as_ref() {
            hooks.client_disconnected();
        }
    }

    async fn handle_request(&self, mut request: HostRequestEnvelope) -> HostResponseEnvelope {
        if matches!(request.command, HostCommand::Health) {
            return self.health_response(request).await;
        }
        if let HostCommand::SetSchedulerPressure(command) = request.command.clone() {
            if let Err(message) = request.validate() {
                return HostResponseEnvelope::failure(
                    request.command_id,
                    HostError::new("invalid_request", message, false),
                );
            }
            return self.update_scheduler_pressure(
                request,
                command.pressure_level,
                command.target_concurrent_work_items,
            );
        }
        if let HostCommand::SetSchedulerAdmission(command) = request.command.clone() {
            return self
                .update_scheduler_admission(request, command.accepting_new_work)
                .await;
        }
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
        if let HostCommand::ResolveAgentTeamHookContext(command) = &request.command {
            if let Err(message) = request.validate() {
                return HostResponseEnvelope::failure(
                    request.command_id,
                    HostError::new("invalid_request", message, false),
                );
            }
            let Some(hooks) = self.hook_coordinator.as_ref() else {
                return HostResponseEnvelope::success(
                    request.command_id,
                    HostResult::AgentTeamHookContext {
                        contexts: Vec::new(),
                    },
                );
            };
            return match hooks.resolve_agent_team_context(command).await {
                Ok(contexts) => HostResponseEnvelope::success(
                    request.command_id,
                    HostResult::AgentTeamHookContext { contexts },
                ),
                Err(message) => HostResponseEnvelope::failure(
                    request.command_id,
                    HostError::new("hook_execution_failed", message, true),
                ),
            };
        }
        if let HostCommand::StartConversationTurn(command) = &request.command {
            if let Err(message) = request.validate() {
                return HostResponseEnvelope::failure(
                    request.command_id,
                    HostError::new("invalid_request", message, false),
                );
            }
            if let Some(hooks) = self.hook_coordinator.as_ref() {
                match hooks.gate_user_prompt(command).await {
                    Ok(LocalHookPromptDecision::Allow) => {}
                    Ok(LocalHookPromptDecision::Block { reason }) => {
                        return HostResponseEnvelope::failure(
                            request.command_id,
                            HostError::new("hook_blocked", reason, false),
                        );
                    }
                    Err(message) => {
                        return HostResponseEnvelope::failure(
                            request.command_id,
                            HostError::new("hook_execution_failed", message, true),
                        );
                    }
                }
            }
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
        let interrupt_reason = match &request.command {
            HostCommand::CancelRun(command) => Some(command.reason.clone()),
            HostCommand::CancelConversationTurn(command) => Some(command.reason.clone()),
            _ => None,
        };
        let wakes_scheduler = command_wakes_scheduler(&request.command);
        let activity_policy = command_activity_signal_policy(&request.command);
        self.route_external_tool_claim(&mut request);
        let response = self.runtime.handle(request).await;
        let response = self.apply_hook_preflight_to_external_claim(response).await;
        if response.ok {
            if let (Some(reason), Some(hooks), Some(run)) = (
                interrupt_reason.as_deref(),
                self.hook_coordinator.as_ref(),
                cancelled_run(&response),
            ) {
                let _ = hooks.enqueue_interrupt(run, reason).await;
            }
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

fn cancelled_run(response: &HostResponseEnvelope) -> Option<&LocalAgentRunRecord> {
    let run = match response.result.as_ref()? {
        HostResult::Run { run } => run,
        HostResult::ConversationTurnUpdated { result } => &result.run,
        _ => return None,
    };
    (run.status == LocalAgentRunStatus::Cancelled).then_some(run)
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
        | HostCommand::ResolveTaskFollowup(_)
        | HostCommand::RetryTask(_)
        | HostCommand::RestartTask(_)
        | HostCommand::StartConversationTurn(_)
        | HostCommand::GuideConversationTurn(_)
        | HostCommand::ResumeConversationTurn(_)
        | HostCommand::CancelConversationTurn(_)
        | HostCommand::ResolveRequirementSurvey(_) => true,
        HostCommand::Health
        | HostCommand::SetSchedulerPressure(_)
        | HostCommand::SetSchedulerAdmission(_)
        | HostCommand::GetMemorySyncStatus(_)
        | HostCommand::PutModelConfigSnapshot(_)
        | HostCommand::GetModelConfigSnapshot(_)
        | HostCommand::ListLatestModelConfigSnapshots(_)
        | HostCommand::PutCapabilityPolicySnapshot(_)
        | HostCommand::GetCapabilityPolicySnapshot(_)
        | HostCommand::GetLatestCapabilityPolicySnapshot(_)
        | HostCommand::GetRunSkillActivations(_)
        | HostCommand::GetRunCapabilityDisclosures(_)
        | HostCommand::GetRunSkillBundle(_)
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
        | HostCommand::PutPluginHookBundle(_)
        | HostCommand::ListPluginHookBundles(_)
        | HostCommand::ListPluginHookBundleRecords(_)
        | HostCommand::ListPluginHookInvocations(_)
        | HostCommand::SetActivePluginHookBundles(_)
        | HostCommand::ResolveAgentTeamHookContext(_)
        | HostCommand::CreateConversation(_)
        | HostCommand::GetConversation(_)
        | HostCommand::GetConversationContext(_)
        | HostCommand::GetConversationAttachment(_)
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
        | HostCommand::GetArtifactFile(_)
        | HostCommand::DeleteArtifact(_)
        | HostCommand::CreateRequirementSurvey(_)
        | HostCommand::ListRequirementSurveys(_)
        | HostCommand::GetRequirementSurvey(_)
        | HostCommand::GetRequirementPlan(_)
        | HostCommand::SaveRequirementPlan(_)
        | HostCommand::ConfirmRequirementPlan(_) => false,
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
