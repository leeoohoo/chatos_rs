// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{require_identifier, require_nonempty_bounded_text, ProtocolError};

const MAXIMUM_COMMAND_BYTES: usize = 64 * 1024;
const MAXIMUM_PATH_BYTES: usize = 16 * 1024;
const MAXIMUM_REASON_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalApprovalHistoryDraft {
    pub command: String,
    pub cwd: String,
    pub source: String,
    pub mode: String,
    pub decision: String,
    pub risk: String,
    pub reason: Option<String>,
}

impl LocalApprovalHistoryDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_nonempty_bounded_text("approval_command", &self.command, MAXIMUM_COMMAND_BYTES)?;
        require_nonempty_bounded_text("approval_cwd", &self.cwd, MAXIMUM_PATH_BYTES)?;
        require_identifier("approval_source", &self.source)?;
        require_identifier("approval_mode", &self.mode)?;
        require_identifier("approval_decision", &self.decision)?;
        require_identifier("approval_risk", &self.risk)?;
        if let Some(reason) = &self.reason {
            require_nonempty_bounded_text("approval_reason", reason, MAXIMUM_REASON_BYTES)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalApprovalHistorySnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalApprovalHistoryDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalApprovalHistorySnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("approval_history_record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "approval history revision or timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppendApprovalHistoryCommand {
    pub record_id: String,
    pub draft: LocalApprovalHistoryDraft,
}

impl AppendApprovalHistoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("approval_history_record_id", &self.record_id)?;
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListApprovalHistoryCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListApprovalHistoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "approval history page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_history_rejects_empty_decisions_and_unbounded_pages() {
        let invalid = AppendApprovalHistoryCommand {
            record_id: "approval:1".to_string(),
            draft: LocalApprovalHistoryDraft {
                command: "git push".to_string(),
                cwd: "/project".to_string(),
                source: "native-terminal".to_string(),
                mode: "request_approval".to_string(),
                decision: String::new(),
                risk: "high".to_string(),
                reason: None,
            },
        };
        assert!(invalid.validate().is_err());
        assert!(ListApprovalHistoryCommand {
            cursor: None,
            limit: 501,
        }
        .validate()
        .is_err());
    }
}
