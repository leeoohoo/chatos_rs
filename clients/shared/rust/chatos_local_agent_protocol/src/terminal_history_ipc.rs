// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    require_bounded_json, require_identifier, require_nonempty_bounded_text, ProtocolError,
};

const MAXIMUM_COMMAND_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalTerminalHistoryDraft {
    pub project_id: Option<String>,
    pub terminal_session_id: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub state: Value,
}

impl LocalTerminalHistoryDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(project_id) = &self.project_id {
            require_identifier("project_id", project_id)?;
        }
        require_identifier("terminal_session_id", &self.terminal_session_id)?;
        require_nonempty_bounded_text("terminal_command", &self.command, MAXIMUM_COMMAND_BYTES)?;
        require_bounded_json("terminal_history_state", &self.state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalTerminalHistorySnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalTerminalHistoryDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalTerminalHistorySnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("terminal_history_record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "terminal history revision or timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppendTerminalHistoryCommand {
    pub record_id: String,
    pub draft: LocalTerminalHistoryDraft,
}

impl AppendTerminalHistoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("terminal_history_record_id", &self.record_id)?;
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListTerminalHistoryCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListTerminalHistoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "terminal history page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteTerminalHistoryCommand {
    pub record_id: String,
    pub expected_revision: u64,
}

impl DeleteTerminalHistoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("terminal_history_record_id", &self.record_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "terminal history expected_revision must be positive",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terminal_history_rejects_empty_commands_and_unbounded_pages() {
        let invalid = AppendTerminalHistoryCommand {
            record_id: "terminal:1".to_string(),
            draft: LocalTerminalHistoryDraft {
                project_id: None,
                terminal_session_id: "native-terminal".to_string(),
                command: String::new(),
                exit_code: None,
                state: json!({}),
            },
        };
        assert!(invalid.validate().is_err());
        assert!(ListTerminalHistoryCommand {
            cursor: None,
            limit: 501,
        }
        .validate()
        .is_err());
    }
}
