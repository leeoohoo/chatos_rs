// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::HostCommand;

impl HostCommand {
    /// Returns the authenticated account scope carried by every stateful IPC
    /// command. The standalone Host compares it with the account selected at
    /// process launch before dispatching into the application runtime.
    pub fn owner_user_id(&self) -> Option<&str> {
        match self {
            Self::Health => None,
            Self::GetMemorySyncStatus(command) => Some(&command.tenant_id),
            Self::PutModelConfigSnapshot(command) => Some(&command.snapshot.owner_user_id),
            Self::GetModelConfigSnapshot(command) => Some(&command.owner_user_id),
            Self::PutCapabilityPolicySnapshot(command) => Some(&command.snapshot.owner_user_id),
            Self::GetCapabilityPolicySnapshot(command) => Some(&command.owner_user_id),
            Self::CreateRun(command) => Some(&command.owner_user_id),
            Self::GetRun(command) => Some(&command.owner_user_id),
            Self::ListRuns(command) => Some(&command.owner_user_id),
            Self::ClaimNextRun(command) => Some(&command.owner_user_id),
            Self::CommitStep(command) => Some(&command.owner_user_id),
            Self::ClaimNextTool(command) => Some(&command.owner_user_id),
            Self::CommitTool(command) => Some(&command.owner_user_id),
            Self::ListPendingToolApprovals(command) => Some(&command.owner_user_id),
            Self::DecideToolApproval(command) => Some(&command.owner_user_id),
            Self::ResumeRun(command) => Some(&command.owner_user_id),
            Self::CancelRun(command) => Some(&command.owner_user_id),
            Self::ListEvents(command) => Some(&command.owner_user_id),
            Self::WaitEvents(command) => Some(&command.owner_user_id),
            Self::CreateTaskGraph(command) => Some(&command.owner_user_id),
            Self::ListTaskGraphs(command) => Some(&command.owner_user_id),
            Self::GetTaskGraph(command) => Some(&command.owner_user_id),
            Self::GetTaskRuns(command) => Some(&command.owner_user_id),
            Self::CancelTask(command) => Some(&command.owner_user_id),
            Self::RetryTask(command) => Some(&command.owner_user_id),
            Self::RestartTask(command) => Some(&command.owner_user_id),
            Self::PutPluginInstallation(command) => Some(&command.installation.owner_user_id),
            Self::GetPluginInstallation(command) => Some(&command.owner_user_id),
            Self::ListPluginInstallations(command) => Some(&command.owner_user_id),
            Self::RemovePluginInstallation(command) => Some(&command.owner_user_id),
            Self::CreateConversation(command) => Some(&command.owner_user_id),
            Self::GetConversation(command) => Some(&command.owner_user_id),
            Self::GetConversationHistory(command) => Some(&command.owner_user_id),
            Self::ListConversations(command) => Some(&command.owner_user_id),
            Self::GetConversationRuntimeSettings(command) => Some(&command.owner_user_id),
            Self::PutConversationRuntimeSettings(command) => Some(&command.owner_user_id),
            Self::StartConversationTurn(command) => Some(&command.owner_user_id),
            Self::GuideConversationTurn(command) => Some(&command.owner_user_id),
            Self::ResumeConversationTurn(command) => Some(&command.owner_user_id),
            Self::CancelConversationTurn(command) => Some(&command.owner_user_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommitStepCommand, CreateRunCommand, LocalAgentStepOutcome};
    use serde_json::Value;

    #[test]
    fn every_stateful_command_exposes_its_account_scope() {
        assert_eq!(HostCommand::Health.owner_user_id(), None);
        let create = HostCommand::CreateRun(CreateRunCommand {
            run_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "test".to_string(),
            owner_entity_id: "entity-1".to_string(),
            profile_key: "main_chat".to_string(),
            model_config_ref: "default".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: Value::Null,
            max_iterations: 4,
        });
        assert_eq!(create.owner_user_id(), Some("user-1"));
        let commit = HostCommand::CommitStep(CommitStepCommand {
            owner_user_id: "user-2".to_string(),
            run_id: "run-2".to_string(),
            claim_token: "claim-2".to_string(),
            expected_version: 2,
            outcome: LocalAgentStepOutcome::Succeed {
                output: Value::Null,
            },
        });
        assert_eq!(commit.owner_user_id(), Some("user-2"));
    }
}
