// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    require_bounded_json, require_identifier, LocalAgentRun, ProtocolError,
    LOCAL_AGENT_PROTOCOL_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentIpcRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub owner_user_id: String,
    pub command: LocalAgentCommand,
}

impl LocalAgentIpcRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidState {
                reason: "unsupported local Agent protocol version",
            });
        }
        require_identifier("request_id", &self.request_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.command.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentCommand {
    CreateMainChatTurn(Value),
    CreateTask(Value),
    PauseRun {
        run_id: String,
    },
    ResumeRun {
        run_id: String,
    },
    CancelRun {
        run_id: String,
    },
    AnswerUserQuestion {
        run_id: String,
        answer: Value,
    },
    ApproveOrRejectTool {
        invocation_id: String,
        approved: bool,
    },
    GetRun {
        run_id: String,
    },
    ListRuns {
        cursor: Option<String>,
        limit: u32,
    },
    SubscribeRunEvents {
        after_seq: u64,
    },
    GetStorageProfile,
    TestPostgresConnection(Value),
    ApplyStorageProfile(Value),
    ExportClientData(Value),
    ImportClientData(Value),
}

impl LocalAgentCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::CreateMainChatTurn(payload)
            | Self::CreateTask(payload)
            | Self::TestPostgresConnection(payload)
            | Self::ApplyStorageProfile(payload)
            | Self::ExportClientData(payload)
            | Self::ImportClientData(payload) => require_bounded_json("command_payload", payload),
            Self::PauseRun { run_id }
            | Self::ResumeRun { run_id }
            | Self::CancelRun { run_id }
            | Self::GetRun { run_id } => require_identifier("run_id", run_id),
            Self::AnswerUserQuestion { run_id, answer } => {
                require_identifier("run_id", run_id)?;
                require_bounded_json("answer", answer)
            }
            Self::ApproveOrRejectTool { invocation_id, .. } => {
                require_identifier("invocation_id", invocation_id)
            }
            Self::ListRuns { cursor, limit } => {
                if let Some(cursor) = cursor {
                    require_identifier("cursor", cursor)?;
                }
                if *limit == 0 || *limit > 500 {
                    return Err(ProtocolError::InvalidState {
                        reason: "list_runs limit must be between 1 and 500",
                    });
                }
                Ok(())
            }
            Self::SubscribeRunEvents { .. } | Self::GetStorageProfile => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentIpcResponse {
    Accepted {
        operation_id: String,
    },
    Run(Box<LocalAgentRun>),
    Runs {
        runs: Vec<LocalAgentRun>,
        next_cursor: Option<String>,
    },
    Event(LocalAgentUiEvent),
    StorageProfile(Value),
    Export(Value),
    Success,
    Error(LocalAgentIpcError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentUiEvent {
    pub event_seq: u64,
    pub run_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentIpcError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_serialization_uses_stable_snake_case_tags() {
        let value = serde_json::to_value(LocalAgentCommand::PauseRun {
            run_id: "run-1".to_string(),
        })
        .unwrap();
        assert_eq!(value["type"], "pause_run");
        assert_eq!(value["payload"]["run_id"], "run-1");
    }

    #[test]
    fn rejects_an_unknown_protocol_version() {
        let request = LocalAgentIpcRequest {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION + 1,
            request_id: "request-1".to_string(),
            owner_user_id: "user-1".to_string(),
            command: LocalAgentCommand::GetStorageProfile,
        };
        assert!(matches!(
            request.validate(),
            Err(ProtocolError::InvalidState { .. })
        ));
    }
}
