// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{require_bounded_json, require_identifier, ProtocolError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageMode {
    Semantic,
    ProviderContext,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySyncStatus {
    Pending,
    Synced,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentMessage {
    pub record_id: String,
    pub run_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub sequence: u64,
    pub role: AgentMessageRole,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub structured_payload: Option<Value>,
    pub tool_call_id: Option<String>,
    pub response_id: Option<String>,
    pub message_mode: MessageMode,
    pub message_source: String,
    pub memory_sync_status: MemorySyncStatus,
    pub created_at: DateTime<Utc>,
}

impl AgentMessage {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("record_id", self.record_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("thread_id", self.thread_id.as_str()),
            ("turn_id", self.turn_id.as_str()),
            ("message_source", self.message_source.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if self.sequence == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "message sequence must be positive",
            });
        }
        if let Some(payload) = &self.structured_payload {
            require_bounded_json("structured_payload", payload)?;
        }
        if self.content.as_deref().is_none_or(str::is_empty)
            && self.reasoning.as_deref().is_none_or(str::is_empty)
            && self.structured_payload.is_none()
        {
            return Err(ProtocolError::InvalidState {
                reason: "message must contain semantic content",
            });
        }
        if self.role == AgentMessageRole::Tool {
            require_identifier(
                "tool_call_id",
                self.tool_call_id.as_deref().unwrap_or_default(),
            )?;
        }
        Ok(())
    }
}
