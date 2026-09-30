// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text, LocalAgentRunRecord, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, fmt, str::FromStr};

pub const LOCAL_CONVERSATION_MAX_ATTACHMENTS: usize = 32;
pub const LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetConversationRuntimeSettingsCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
}

impl GetConversationRuntimeSettingsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("conversation_id", &self.conversation_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutConversationRuntimeSettingsCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub selected_model_config_ref: String,
    pub selected_model_config_revision: String,
    pub selected_thinking_level: Option<String>,
    pub remote_connection_id: Option<String>,
    pub reasoning_enabled: bool,
    pub expected_version: Option<u64>,
}

impl PutConversationRuntimeSettingsCommand {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("owner_user_id", self.owner_user_id.as_str()),
            ("conversation_id", self.conversation_id.as_str()),
            (
                "selected_model_config_ref",
                self.selected_model_config_ref.as_str(),
            ),
            (
                "selected_model_config_revision",
                self.selected_model_config_revision.as_str(),
            ),
        ] {
            validate_identifier(field, value)?;
        }
        if let Some(level) = self.selected_thinking_level.as_deref() {
            validate_identifier("selected_thinking_level", level)?;
        }
        if let Some(connection_id) = self.remote_connection_id.as_deref() {
            validate_identifier("remote_connection_id", connection_id)?;
        }
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConversationRuntimeSettings {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub selected_model_config_ref: String,
    pub selected_model_config_revision: String,
    pub selected_thinking_level: Option<String>,
    pub remote_connection_id: Option<String>,
    pub reasoning_enabled: bool,
    pub version: u64,
    pub updated_at_unix_ms: i64,
}

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
    pub owner_user_id: String,
    pub conversation_id: String,
    pub expected_conversation_version: u64,
    pub turn_id: String,
    pub message_id: String,
    pub run_id: String,
    pub message: String,
    #[serde(default)]
    pub message_metadata: Value,
    #[serde(default)]
    pub attachments: Vec<LocalConversationAttachmentSpec>,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub max_iterations: u32,
}

impl StartConversationTurnCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
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
        validate_message_payload(&self.message, &self.message_metadata, &self.attachments)?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeConversationTurnCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub expected_conversation_version: u64,
    pub turn_id: String,
    pub expected_run_version: u64,
    pub expected_run_status: crate::LocalAgentRunStatus,
    pub message_id: String,
    pub message: String,
    #[serde(default)]
    pub message_metadata: Value,
    #[serde(default)]
    pub attachments: Vec<LocalConversationAttachmentSpec>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuideConversationTurnCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub expected_conversation_version: u64,
    pub turn_id: String,
    pub expected_run_version: Option<u64>,
    pub message_id: String,
    pub message: String,
    #[serde(default)]
    pub message_metadata: Value,
    #[serde(default)]
    pub attachments: Vec<LocalConversationAttachmentSpec>,
}

impl GuideConversationTurnCommand {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("owner_user_id", self.owner_user_id.as_str()),
            ("conversation_id", self.conversation_id.as_str()),
            ("turn_id", self.turn_id.as_str()),
            ("message_id", self.message_id.as_str()),
        ] {
            validate_identifier(field, value)?;
        }
        if self.expected_conversation_version == 0 || self.expected_run_version == Some(0) {
            return Err("expected versions must be greater than zero".to_string());
        }
        validate_message_payload(&self.message, &self.message_metadata, &self.attachments)
    }
}

impl ResumeConversationTurnCommand {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("owner_user_id", self.owner_user_id.as_str()),
            ("conversation_id", self.conversation_id.as_str()),
            ("turn_id", self.turn_id.as_str()),
            ("message_id", self.message_id.as_str()),
        ] {
            validate_identifier(field, value)?;
        }
        if self.expected_conversation_version == 0 || self.expected_run_version == 0 {
            return Err("expected versions must be greater than zero".to_string());
        }
        if !matches!(
            self.expected_run_status,
            crate::LocalAgentRunStatus::WaitingUser
                | crate::LocalAgentRunStatus::Paused
                | crate::LocalAgentRunStatus::NeedsReview
        ) {
            return Err(
                "only waiting_user, paused, or needs_review Turns can be resumed".to_string(),
            );
        }
        validate_text("reason", &self.reason, 4_000)?;
        validate_message_payload(&self.message, &self.message_metadata, &self.attachments)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelConversationTurnCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub expected_conversation_version: u64,
    pub turn_id: String,
    pub expected_run_version: Option<u64>,
    pub reason: String,
}

impl CancelConversationTurnCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("conversation_id", &self.conversation_id)?;
        validate_identifier("turn_id", &self.turn_id)?;
        if self.expected_conversation_version == 0 || self.expected_run_version == Some(0) {
            return Err("expected versions must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)
    }
}

fn validate_message_payload(
    message: &str,
    message_metadata: &Value,
    attachments: &[LocalConversationAttachmentSpec],
) -> Result<(), String> {
    if message.trim().is_empty() {
        if attachments.is_empty() {
            return Err("message or attachments must be provided".to_string());
        }
    } else {
        validate_text("message", message, LOCAL_AGENT_MAX_INPUT_BYTES)?;
    }
    if attachments.len() > LOCAL_CONVERSATION_MAX_ATTACHMENTS {
        return Err(format!(
            "attachments exceeds the {LOCAL_CONVERSATION_MAX_ATTACHMENTS} item limit"
        ));
    }
    let mut attachment_ids = HashSet::with_capacity(attachments.len());
    for attachment in attachments {
        attachment.validate()?;
        if !attachment_ids.insert(attachment.attachment_id.as_str()) {
            return Err(format!(
                "duplicate attachment_id: {}",
                attachment.attachment_id
            ));
        }
    }
    let attachment_size = serde_json::to_vec(attachments)
        .map_err(|error| format!("attachments are not serializable: {error}"))?
        .len();
    let metadata_size = serde_json::to_vec(message_metadata)
        .map_err(|error| format!("message_metadata is not serializable: {error}"))?
        .len();
    if metadata_size > LOCAL_AGENT_MAX_INPUT_BYTES {
        return Err(format!(
            "message_metadata exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
        ));
    }
    let total_input_size = message
        .len()
        .saturating_add(metadata_size)
        .saturating_add(attachment_size);
    if total_input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
        return Err(format!(
            "message, metadata, and attachments exceed the \
             {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationAttachmentSpec {
    pub attachment_id: String,
    pub display_name: String,
    pub media_type: String,
    pub byte_size: u64,
    pub sha256: String,
    /// Opaque reference issued by a native-client authorization adapter. This
    /// is not an unrestricted filesystem path and is resolved only at use time.
    pub authorized_local_ref: String,
    #[serde(default)]
    pub metadata: Value,
}

impl LocalConversationAttachmentSpec {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("attachment_id", &self.attachment_id)?;
        validate_text("display_name", &self.display_name, 1_000)?;
        validate_text("media_type", &self.media_type, 255)?;
        let reference_token = self
            .authorized_local_ref
            .strip_prefix("local-attachment:")
            .ok_or_else(|| {
                "authorized_local_ref must use the local-attachment:<token> format".to_string()
            })?;
        if reference_token.is_empty()
            || reference_token.len() > 200
            || !reference_token
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || b"-_.".contains(&value))
        {
            return Err(
                "authorized_local_ref token must contain 1..=200 ASCII letters, digits, '-', '_' or '.'"
                    .to_string(),
            );
        }
        if self.byte_size > i64::MAX as u64 {
            return Err("byte_size exceeds the local database limit".to_string());
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err("sha256 must be 64 lowercase hexadecimal characters".to_string());
        }
        let metadata_size = serde_json::to_vec(&self.metadata)
            .map_err(|error| format!("attachment metadata is not serializable: {error}"))?
            .len();
        if metadata_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "attachment metadata exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetConversationCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
}

impl GetConversationCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("conversation_id", &self.conversation_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetConversationHistoryCommand {
    pub owner_user_id: String,
    pub conversation_id: String,
    pub before_ordinal: Option<u64>,
    pub limit: u32,
}

impl GetConversationHistoryCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("conversation_id", &self.conversation_id)?;
        if self.before_ordinal == Some(0) {
            return Err("before_ordinal must be greater than zero".to_string());
        }
        if self.limit == 0 || self.limit > LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE {
            return Err(format!(
                "limit must be between 1 and {LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListConversationsCommand {
    pub owner_user_id: String,
    pub before_updated_at_unix_ms: Option<i64>,
    pub before_conversation_id: Option<String>,
    pub limit: u32,
}

impl ListConversationsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.limit == 0 || self.limit > 200 {
            return Err("limit must be between 1 and 200".to_string());
        }
        match (
            self.before_updated_at_unix_ms,
            self.before_conversation_id.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(timestamp), Some(conversation_id)) if timestamp >= 0 => {
                validate_identifier("before_conversation_id", conversation_id)
            }
            _ => Err(
                "before_updated_at_unix_ms and before_conversation_id must be supplied together"
                    .to_string(),
            ),
        }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConversationPage {
    pub conversations: Vec<LocalConversationRecord>,
    pub next_before_updated_at_unix_ms: Option<i64>,
    pub next_before_conversation_id: Option<String>,
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
pub struct LocalConversationAttachmentRecord {
    pub attachment_id: String,
    pub conversation_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub ordinal: u64,
    pub display_name: String,
    pub media_type: String,
    pub byte_size: u64,
    pub sha256: String,
    pub authorized_local_ref: String,
    pub metadata: Value,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationDetail {
    pub conversation: LocalConversationRecord,
    pub turns: Vec<LocalConversationTurnRecord>,
    pub messages: Vec<LocalConversationMessageRecord>,
    pub attachments: Vec<LocalConversationAttachmentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationHistoryPage {
    pub conversation: LocalConversationRecord,
    pub turns: Vec<LocalConversationTurnRecord>,
    pub messages: Vec<LocalConversationMessageRecord>,
    pub attachments: Vec<LocalConversationAttachmentRecord>,
    pub next_before_ordinal: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationTurnStart {
    pub conversation: LocalConversationRecord,
    pub turn: LocalConversationTurnRecord,
    pub message: LocalConversationMessageRecord,
    pub attachments: Vec<LocalConversationAttachmentRecord>,
    pub run: LocalAgentRunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConversationTurnUpdate {
    pub conversation: LocalConversationRecord,
    pub turn: LocalConversationTurnRecord,
    pub message: Option<LocalConversationMessageRecord>,
    pub attachments: Vec<LocalConversationAttachmentRecord>,
    pub run: LocalAgentRunRecord,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn attachment(attachment_id: &str) -> LocalConversationAttachmentSpec {
        LocalConversationAttachmentSpec {
            attachment_id: attachment_id.to_string(),
            display_name: "brief.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            byte_size: 42,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:authority-1".to_string(),
            metadata: json!({}),
        }
    }

    fn command() -> StartConversationTurnCommand {
        StartConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version: 1,
            turn_id: "turn-1".to_string(),
            message_id: "message-1".to_string(),
            run_id: "run-1".to_string(),
            message: String::new(),
            message_metadata: json!({}),
            attachments: vec![attachment("attachment-1")],
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            max_iterations: 8,
        }
    }

    #[test]
    fn attachment_only_turn_is_valid_but_duplicate_or_noncanonical_hash_is_not() {
        assert!(command().validate().is_ok());

        let mut duplicate = command();
        duplicate.attachments.push(attachment("attachment-1"));
        assert!(duplicate.validate().is_err());

        let mut uppercase_hash = command();
        uppercase_hash.attachments[0].sha256 = "A".repeat(64);
        assert!(uppercase_hash.validate().is_err());

        let mut raw_path = command();
        raw_path.attachments[0].authorized_local_ref = "/tmp/brief.pdf".to_string();
        assert!(raw_path.validate().is_err());
    }

    #[test]
    fn conversation_turn_updates_require_versions_and_resumable_status() {
        let mut resume = ResumeConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version: 2,
            turn_id: "turn-1".to_string(),
            expected_run_version: 3,
            expected_run_status: crate::LocalAgentRunStatus::WaitingUser,
            message_id: "message-2".to_string(),
            message: "yes".to_string(),
            message_metadata: json!({}),
            attachments: Vec::new(),
            reason: "user replied".to_string(),
        };
        assert!(resume.validate().is_ok());
        resume.expected_run_status = crate::LocalAgentRunStatus::Queued;
        assert!(resume.validate().is_err());
        resume.expected_run_status = crate::LocalAgentRunStatus::WaitingUser;
        resume.expected_conversation_version = 0;
        assert!(resume.validate().is_err());

        assert!(CancelConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version: 2,
            turn_id: "turn-1".to_string(),
            expected_run_version: Some(3),
            reason: "user stopped".to_string(),
        }
        .validate()
        .is_ok());

        assert!(GuideConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version: 2,
            turn_id: "turn-1".to_string(),
            expected_run_version: None,
            message_id: "message-guidance".to_string(),
            message: "also inspect the tests".to_string(),
            message_metadata: json!({}),
            attachments: Vec::new(),
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn history_page_requires_a_bounded_positive_cursor() {
        assert!(GetConversationHistoryCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            before_ordinal: None,
            limit: LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE,
        }
        .validate()
        .is_ok());
        assert!(GetConversationHistoryCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            before_ordinal: Some(0),
            limit: 10,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn conversation_list_cursor_is_paired_and_owner_scoped() {
        let valid = ListConversationsCommand {
            owner_user_id: "user-1".to_string(),
            before_updated_at_unix_ms: Some(1_000),
            before_conversation_id: Some("conversation-1".to_string()),
            limit: 50,
        };
        assert!(valid.validate().is_ok());
        assert!(ListConversationsCommand {
            before_conversation_id: None,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListConversationsCommand {
            owner_user_id: String::new(),
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListConversationsCommand {
            limit: 201,
            ..valid
        }
        .validate()
        .is_err());
    }
}
