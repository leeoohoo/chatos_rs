// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{command_fingerprint, LocalAgentRuntimeError};
use chatos_local_agent_ports::IdempotentCommand;
use chatos_local_agent_protocol::HostCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandExecutionMode {
    Ipc,
    TrustedOneShot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandReceiptPolicy {
    ReadEphemeral,
    // Keep durable write identities and exact historical responses while cached.
    // Eviction yields an explicit error; it never authorizes another write.
    ReplayableWrite,
    // Only the in-process schedulers may use these without IPC retry receipts.
    // Their claim tokens, version checks and reconciliation remain authoritative.
    SchedulerReplayableWrite,
}

impl CommandReceiptPolicy {
    fn for_command(command: &HostCommand) -> Self {
        // Intentionally exhaustive: adding an IPC command must choose a policy.
        // Do not classify by a wire name/prefix or add a fallback arm here.
        match command {
            HostCommand::Health
            | HostCommand::SetSchedulerPressure(_)
            | HostCommand::GetMemorySyncStatus(_)
            | HostCommand::GetModelConfigSnapshot(_)
            | HostCommand::ListLatestModelConfigSnapshots(_)
            | HostCommand::GetCapabilityPolicySnapshot(_)
            | HostCommand::GetLatestCapabilityPolicySnapshot(_)
            | HostCommand::GetRunSkillActivations(_)
            | HostCommand::GetRunCapabilityDisclosures(_)
            | HostCommand::GetRunSkillBundle(_)
            | HostCommand::GetRun(_)
            | HostCommand::ListRuns(_)
            | HostCommand::ListPendingToolApprovals(_)
            | HostCommand::GetEventCursor(_)
            | HostCommand::ListEvents(_)
            | HostCommand::WaitEvents(_)
            | HostCommand::ListTaskGraphs(_)
            | HostCommand::GetTaskGraph(_)
            | HostCommand::GetMessageTaskGraph(_)
            | HostCommand::GetTaskRuns(_)
            | HostCommand::GetPluginInstallation(_)
            | HostCommand::ListPluginInstallations(_)
            | HostCommand::ListPluginHookBundles(_)
            | HostCommand::ListPluginHookInvocations(_)
            | HostCommand::GetConversation(_)
            | HostCommand::GetConversationContext(_)
            | HostCommand::GetConversationAttachment(_)
            | HostCommand::GetConversationHistory(_)
            | HostCommand::ListConversations(_)
            | HostCommand::GetConversationRuntimeSettings(_)
            // Despite its name, the current implementation only counts existing notes.
            | HostCommand::InitializeNotepad(_)
            | HostCommand::ListNotepadFolders(_)
            | HostCommand::ListNotepadNotes(_)
            | HostCommand::GetNotepadNote(_)
            | HostCommand::ListRemoteConnections(_)
            | HostCommand::GetRemoteConnection(_)
            | HostCommand::ListArtifacts(_)
            | HostCommand::GetArtifactData(_)
            | HostCommand::GetArtifactFile(_)
            | HostCommand::ListRequirementSurveys(_)
            | HostCommand::GetRequirementSurvey(_)
            | HostCommand::GetRequirementPlan(_) => Self::ReadEphemeral,

            HostCommand::ClaimNextRun(_)
            | HostCommand::ClaimNextTool(_)
            | HostCommand::CommitStep(_)
            | HostCommand::CommitTool(_) => Self::SchedulerReplayableWrite,

            HostCommand::PutModelConfigSnapshot(_)
            | HostCommand::PutCapabilityPolicySnapshot(_)
            | HostCommand::CreateRun(_)
            | HostCommand::RenewToolClaim(_)
            | HostCommand::DecideToolApproval(_)
            | HostCommand::ResumeRun(_)
            | HostCommand::CancelRun(_)
            | HostCommand::CreateTaskGraph(_)
            | HostCommand::CancelTask(_)
            | HostCommand::ResolveTaskFollowup(_)
            | HostCommand::RetryTask(_)
            | HostCommand::RestartTask(_)
            | HostCommand::PutPluginInstallation(_)
            | HostCommand::RemovePluginInstallation(_)
            | HostCommand::PutPluginHookBundle(_)
            | HostCommand::SetActivePluginHookBundles(_)
            | HostCommand::CreateConversation(_)
            | HostCommand::PutConversationRuntimeSettings(_)
            | HostCommand::StartConversationTurn(_)
            | HostCommand::GuideConversationTurn(_)
            | HostCommand::ResumeConversationTurn(_)
            | HostCommand::CancelConversationTurn(_)
            | HostCommand::CreateNotepadFolder(_)
            | HostCommand::RenameNotepadFolder(_)
            | HostCommand::DeleteNotepadFolder(_)
            | HostCommand::CreateNotepadNote(_)
            | HostCommand::UpdateNotepadNote(_)
            | HostCommand::DeleteNotepadNote(_)
            | HostCommand::PutNotepadImage(_)
            | HostCommand::CreateRemoteConnection(_)
            | HostCommand::UpdateRemoteConnection(_)
            | HostCommand::DeleteRemoteConnection(_)
            | HostCommand::CreateArtifact(_)
            | HostCommand::DeleteArtifact(_)
            | HostCommand::CreateRequirementSurvey(_)
            | HostCommand::ResolveRequirementSurvey(_)
            | HostCommand::SaveRequirementPlan(_)
            | HostCommand::ConfirmRequirementPlan(_) => Self::ReplayableWrite,
        }
    }
}

pub(super) fn prepare_idempotency(
    command_id: String,
    command: &HostCommand,
    mode: CommandExecutionMode,
) -> Result<IdempotentCommand, LocalAgentRuntimeError> {
    let persist_receipt = match (CommandReceiptPolicy::for_command(command), mode) {
        (CommandReceiptPolicy::ReadEphemeral, _) => false,
        (CommandReceiptPolicy::ReplayableWrite, CommandExecutionMode::TrustedOneShot) => {
            return Err(LocalAgentRuntimeError::InvalidRequest(
                "trusted ephemeral execution only permits reads and scheduler claim/commit commands"
                    .to_string(),
            ));
        }
        (CommandReceiptPolicy::SchedulerReplayableWrite, CommandExecutionMode::TrustedOneShot) => {
            false
        }
        (
            CommandReceiptPolicy::ReplayableWrite | CommandReceiptPolicy::SchedulerReplayableWrite,
            CommandExecutionMode::Ipc,
        ) => true,
    };
    Ok(IdempotentCommand {
        command_id,
        request_fingerprint: if persist_receipt {
            command_fingerprint::fingerprint(command)
                .map_err(|error| LocalAgentRuntimeError::InvalidRequest(error.to_string()))?
        } else {
            // Neither pure reads nor trusted one-shot scheduler calls use this field.
            String::new()
        },
        persist_receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn command(value: serde_json::Value) -> HostCommand {
        serde_json::from_value(value).expect("policy fixture")
    }

    #[test]
    fn pure_reads_skip_receipts_and_fingerprints_in_both_modes() {
        for value in [
            json!({"type": "health"}),
            json!({"type": "get_run", "owner_user_id": "alice", "run_id": "run-1"}),
            json!({"type": "get_run_skill_bundle", "owner_user_id": "alice", "run_id": "run-1"}),
            json!({"type": "initialize_notepad", "owner_user_id": "alice"}),
            json!({"type": "get_event_cursor", "owner_user_id": "alice"}),
            json!({"type": "get_conversation_context", "owner_user_id": "alice", "conversation_id": "conversation-1"}),
            json!({"type": "get_notepad_note", "owner_user_id": "alice", "note_id": "note-1"}),
        ] {
            let command = command(value);
            command.validate().expect("valid read fixture");
            assert_eq!(
                CommandReceiptPolicy::for_command(&command),
                CommandReceiptPolicy::ReadEphemeral
            );
            for mode in [
                CommandExecutionMode::Ipc,
                CommandExecutionMode::TrustedOneShot,
            ] {
                let prepared = prepare_idempotency("read-1".into(), &command, mode).unwrap();
                assert!(!prepared.persist_receipt);
                assert!(prepared.request_fingerprint.is_empty());
            }
        }
    }

    #[test]
    fn scheduler_exemption_is_limited_to_the_existing_claim_and_commit_operations() {
        for value in [
            json!({"type": "claim_next_run", "owner_user_id": "alice", "worker_id": "worker-1", "lease_duration_ms": 10_000}),
            json!({"type": "claim_next_tool", "owner_user_id": "alice", "worker_id": "worker-1", "lease_duration_ms": 10_000}),
            json!({"type": "commit_step", "owner_user_id": "alice", "run_id": "run-1", "claim_token": "claim-1", "expected_version": 2, "outcome": {"type": "succeed", "output": {"answer": 42}}}),
            json!({"type": "commit_tool", "owner_user_id": "alice", "invocation_id": "invocation-1", "claim_token": "claim-1", "expected_version": 2, "outcome": {"type": "succeeded", "output": {"answer": 42}}}),
        ] {
            let command = command(value);
            command.validate().expect("valid scheduler fixture");
            let ipc =
                prepare_idempotency("write-1".into(), &command, CommandExecutionMode::Ipc).unwrap();
            assert!(ipc.persist_receipt);
            assert_eq!(ipc.request_fingerprint.len(), 74);
            let trusted = prepare_idempotency(
                "write-1".into(),
                &command,
                CommandExecutionMode::TrustedOneShot,
            )
            .unwrap();
            assert!(!trusted.persist_receipt);
            assert!(trusted.request_fingerprint.is_empty());
        }
    }

    #[test]
    fn other_writes_cannot_use_the_trusted_scheduler_exemption() {
        for value in [
            json!({"type": "create_notepad_note", "owner_user_id": "alice", "folder": "", "title": "Draft", "content": "Body", "tags": []}),
            json!({"type": "cancel_run", "owner_user_id": "alice", "run_id": "run-1", "reason": "User cancelled"}),
            json!({"type": "renew_tool_claim", "owner_user_id": "alice", "invocation_id": "invocation-1", "claim_token": "claim-1", "expected_version": 2, "lease_duration_ms": 10_000}),
        ] {
            let command = command(value);
            command.validate().expect("valid write fixture");
            let ipc =
                prepare_idempotency("write-1".into(), &command, CommandExecutionMode::Ipc).unwrap();
            assert!(ipc.persist_receipt);
            assert_eq!(ipc.request_fingerprint.len(), 74);
            assert!(matches!(
                prepare_idempotency(
                    "write-1".into(),
                    &command,
                    CommandExecutionMode::TrustedOneShot
                ),
                Err(LocalAgentRuntimeError::InvalidRequest(_))
            ));
        }
    }
}
