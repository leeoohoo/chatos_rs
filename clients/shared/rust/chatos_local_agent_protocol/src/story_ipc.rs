// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    require_identifier, require_json_with_limit, require_nonempty_bounded_text, ProtocolError,
};

const MAXIMUM_STORY_STATE_BYTES: usize = 6 * 1024 * 1024;
const MAXIMUM_STORY_STATUS_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalStoryKind {
    Project,
    AgentRun,
    MediaBatch,
}

impl LocalStoryKind {
    pub fn record_prefix(self) -> &'static str {
        match self {
            Self::Project => "project:",
            Self::AgentRun => "agent-run:",
            Self::MediaBatch => "media-batch:",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalStoryDraft {
    pub project_id: String,
    pub kind: LocalStoryKind,
    pub status: Option<String>,
    pub state: Value,
}

impl LocalStoryDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("story_project_id", &self.project_id)?;
        if let Some(status) = &self.status {
            require_nonempty_bounded_text("story_status", status, MAXIMUM_STORY_STATUS_BYTES)?;
        }
        require_json_with_limit("story_state", &self.state, MAXIMUM_STORY_STATE_BYTES)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalStorySnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalStoryDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalStorySnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("story_record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if !self.record_id.starts_with(self.draft.kind.record_prefix()) {
            return Err(ProtocolError::InvalidState {
                reason: "story record identity does not match its kind",
            });
        }
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "story revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetStoryCommand {
    pub record_id: String,
}

impl GetStoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("story_record_id", &self.record_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListStoriesCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListStoriesCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "story page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PutStoryCommand {
    pub record_id: String,
    pub expected_revision: Option<u64>,
    pub draft: LocalStoryDraft,
}

impl PutStoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("story_record_id", &self.record_id)?;
        if !self.record_id.starts_with(self.draft.kind.record_prefix()) {
            return Err(ProtocolError::InvalidState {
                reason: "story record identity does not match its kind",
            });
        }
        if self.expected_revision == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "story expected_revision must be positive",
            });
        }
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteStoryCommand {
    pub record_id: String,
    pub expected_revision: u64,
}

impl DeleteStoryCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("story_record_id", &self.record_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "story expected_revision must be positive",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_and_record_identity_must_match() {
        let command = PutStoryCommand {
            record_id: "agent-run:run-1".to_string(),
            expected_revision: None,
            draft: LocalStoryDraft {
                project_id: "project-1".to_string(),
                kind: LocalStoryKind::Project,
                status: Some("draft".to_string()),
                state: serde_json::json!({"id": "project-1"}),
            },
        };
        assert!(command.validate().is_err());
    }

    #[test]
    fn story_state_is_bounded_below_the_ipc_frame_limit() {
        let command = PutStoryCommand {
            record_id: "project:project-1".to_string(),
            expected_revision: None,
            draft: LocalStoryDraft {
                project_id: "project-1".to_string(),
                kind: LocalStoryKind::Project,
                status: Some("draft".to_string()),
                state: Value::String("x".repeat(MAXIMUM_STORY_STATE_BYTES)),
            },
        };
        assert!(command.validate().is_err());
    }
}
