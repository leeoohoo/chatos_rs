// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text, LocalAgentRunRecord, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateConversationCommand {
    pub conversation_id: String,
    pub owner_user_id: String,
    pub title: String,
}

impl CreateConversationCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("conversation_id", &self.conversation_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_text("title", &self.title, 1_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StartConversationTurnCommand {
    pub conversation_id: String,
    pub expected_conversation_version: u64,
    pub turn_id: String,
    pub message_id: String,
    pub run_id: String,
    pub message: String,
    #[serde(default)]
    pub message_metadata: Value,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub max_iterations: u32,
}

impl StartConversationTurnCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("conversation_id", &self.conversation_id)?;
        if self.expected_conversation_version == 0 {
            return Err("expected_conversation_version must be greater than zero".to_string());
        }
        for (field, value) in [
            ("turn_id", self.turn_id.as_str()),
            ("message_id", self.message_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("model_config_ref", self.model_config_ref.as_str()),
            ("model_config_revision", self.model_config_revision.as_str()),
            (
                "capability_policy_revision",
                self.capability_policy_revision.as_str(),
            ),
        ] {
            validate_identifier(field, value)?;
        }
        validate_text("message", &self.message, LOCAL_AGENT_MAX_INPUT_BYTES)?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        let metadata_size = serde_json::to_vec(&self.message_metadata)
            .map_err(|error| format!("message_metadata is not serializable: {error}"))?
            .len();
        if metadata_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "message_metadata exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetConversationCommand {
    pub conversation_id: String,
}

impl GetConversationCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("conversation_id", &self.conversation_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListConversationsCommand {
    pub owner_user_id: String,
    pub limit: u32,
}

impl ListConversationsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.limit == 0 || self.limit > 200 {
            return Err("limit must be between 1 and 200".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConversationRecord {
    pub conversation_id: String,
    pub owner_user_id: String,
    pub title: String,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalConversationTurnStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalConversationTurnStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for LocalConversationTurnStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown local conversation turn status: {value}")),
        }
    }
}

impl fmt::Display for LocalConversationTurnStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConversationTurnRecord {
    pub turn_id: String,
    pub conversation_id: String,
    pub user_message_id: String,
    pub run_id: String,
    pub status: LocalConversationTurnStatus,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalConversationMessageRole {
    User,
    Assistant,
}

impl LocalConversationMessageRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

impl FromStr for LocalConversationMessageRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            _ => Err(format!("unknown local conversation message role: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationMessageRecord {
    pub message_id: String,
    pub conversation_id: String,
    pub turn_id: String,
    pub ordinal: u64,
    pub role: LocalConversationMessageRole,
    pub content: Value,
    pub metadata: Value,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationDetail {
    pub conversation: LocalConversationRecord,
    pub turns: Vec<LocalConversationTurnRecord>,
    pub messages: Vec<LocalConversationMessageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationTurnStart {
    pub conversation: LocalConversationRecord,
    pub turn: LocalConversationTurnRecord,
    pub message: LocalConversationMessageRecord,
    pub run: LocalAgentRunRecord,
}
