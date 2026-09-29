// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stable, cross-platform contracts for the client-owned Local Agent Host.
//!
//! The protocol uses length-prefixed JSON frames. It deliberately contains no
//! database, model-provider, plugin, or platform-specific IPC implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

mod tool;

pub use tool::{
    ClaimNextToolCommand, CommitToolCommand, LocalAgentToolBatch, LocalAgentToolCall,
    LocalAgentToolClaim, LocalAgentToolCommitResult, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalAgentToolStatus,
};

pub const LOCAL_AGENT_PROTOCOL_VERSION: u32 = 1;
pub const LOCAL_AGENT_MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const LOCAL_AGENT_MAX_INPUT_BYTES: usize = 256 * 1024;
pub const LOCAL_AGENT_MAX_EVENT_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostRequestEnvelope {
    pub protocol_version: u32,
    pub command_id: String,
    pub command: HostCommand,
}

impl HostRequestEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION {
            return Err(format!(
                "unsupported protocol version {}; expected {}",
                self.protocol_version, LOCAL_AGENT_PROTOCOL_VERSION
            ));
        }
        validate_identifier("command_id", &self.command_id)?;
        self.command.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Health,
    CreateRun(CreateRunCommand),
    GetRun { run_id: String },
    ClaimNextRun(ClaimNextRunCommand),
    CommitStep(CommitStepCommand),
    ClaimNextTool(ClaimNextToolCommand),
    CommitTool(CommitToolCommand),
    ResumeRun(ResumeRunCommand),
    CancelRun(CancelRunCommand),
    ListEvents(ListEventsCommand),
}

impl HostCommand {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Health => Ok(()),
            Self::CreateRun(command) => command.validate(),
            Self::GetRun { run_id } => validate_identifier("run_id", run_id),
            Self::ClaimNextRun(command) => command.validate(),
            Self::CommitStep(command) => command.validate(),
            Self::ClaimNextTool(command) => command.validate(),
            Self::CommitTool(command) => command.validate(),
            Self::ResumeRun(command) => command.validate(),
            Self::CancelRun(command) => command.validate(),
            Self::ListEvents(command) => command.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRunCommand {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    #[serde(default)]
    pub input: Value,
    pub max_iterations: u32,
}

impl CreateRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("owner_entity_type", &self.owner_entity_type)?;
        validate_identifier("owner_entity_id", &self.owner_entity_id)?;
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "input exceeds the {} byte limit",
                LOCAL_AGENT_MAX_INPUT_BYTES
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimNextRunCommand {
    pub worker_id: String,
    pub lease_duration_ms: u64,
}

impl ClaimNextRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("worker_id", &self.worker_id)?;
        if !(1_000..=300_000).contains(&self.lease_duration_ms) {
            return Err("lease_duration_ms must be between 1000 and 300000".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitStepCommand {
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub outcome: LocalAgentStepOutcome,
}

impl CommitStepCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("claim_token", &self.claim_token)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        self.outcome.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelRunCommand {
    pub run_id: String,
    pub expected_version: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeRunCommand {
    pub run_id: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub reason: String,
    #[serde(default)]
    pub input: Value,
}

impl ResumeRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if !matches!(
            self.expected_status,
            LocalAgentRunStatus::WaitingUser
                | LocalAgentRunStatus::Paused
                | LocalAgentRunStatus::NeedsReview
        ) {
            return Err(
                "only waiting_user, paused, or needs_review runs can be resumed".to_string(),
            );
        }
        validate_text("reason", &self.reason, 4_000)?;
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("resume input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "resume input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

impl CancelRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListEventsCommand {
    pub after_cursor: i64,
    pub limit: u32,
    pub run_id: Option<String>,
}

impl ListEventsCommand {
    pub fn validate(&self) -> Result<(), String> {
        if self.after_cursor < 0 {
            return Err("after_cursor must not be negative".to_string());
        }
        if self.limit == 0 || self.limit > LOCAL_AGENT_MAX_EVENT_PAGE_SIZE {
            return Err(format!(
                "limit must be between 1 and {LOCAL_AGENT_MAX_EVENT_PAGE_SIZE}"
            ));
        }
        if let Some(run_id) = self.run_id.as_deref() {
            validate_identifier("run_id", run_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalAgentStepOutcome {
    Continue {
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForTool {
        batch_id: String,
        #[serde(default)]
        tool_calls: Vec<LocalAgentToolCall>,
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForUser {
        #[serde(default)]
        prompt: Value,
        #[serde(default)]
        checkpoint: Value,
    },
    Retry {
        resume_at_unix_ms: i64,
        reason: String,
    },
    Pause {
        reason: String,
    },
    NeedsReview {
        reason: String,
        #[serde(default)]
        detail: Value,
    },
    Succeed {
        #[serde(default)]
        output: Value,
    },
    Fail {
        error: String,
        #[serde(default)]
        detail: Value,
    },
}

impl LocalAgentStepOutcome {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Continue { .. } | Self::WaitForUser { .. } | Self::Succeed { .. } => Ok(()),
            Self::WaitForTool {
                batch_id,
                tool_calls,
                ..
            } => {
                validate_identifier("batch_id", batch_id)?;
                if tool_calls.is_empty() {
                    return Err("wait_for_tool requires at least one tool call".to_string());
                }
                LocalAgentToolBatch {
                    batch_id: batch_id.clone(),
                    calls: tool_calls.clone(),
                }
                .validate()?;
                Ok(())
            }
            Self::Retry {
                resume_at_unix_ms,
                reason,
            } => {
                if *resume_at_unix_ms <= 0 {
                    return Err("resume_at_unix_ms must be positive".to_string());
                }
                validate_text("reason", reason, 4_000)
            }
            Self::Pause { reason } | Self::NeedsReview { reason, .. } => {
                validate_text("reason", reason, 4_000)
            }
            Self::Fail { error, .. } => validate_text("error", error, 8_000),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentRunStatus {
    Queued,
    ModelReady,
    ModelRunning,
    WaitingToolResult,
    ContinuationReady,
    WaitingUser,
    RetryScheduled,
    Paused,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalAgentRunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::ModelReady => "model_ready",
            Self::ModelRunning => "model_running",
            Self::WaitingToolResult => "waiting_tool_result",
            Self::ContinuationReady => "continuation_ready",
            Self::WaitingUser => "waiting_user",
            Self::RetryScheduled => "retry_scheduled",
            Self::Paused => "paused",
            Self::NeedsReview => "needs_review",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for LocalAgentRunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LocalAgentRunStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "model_ready" => Ok(Self::ModelReady),
            "model_running" => Ok(Self::ModelRunning),
            "waiting_tool_result" => Ok(Self::WaitingToolResult),
            "continuation_ready" => Ok(Self::ContinuationReady),
            "waiting_user" => Ok(Self::WaitingUser),
            "retry_scheduled" => Ok(Self::RetryScheduled),
            "paused" => Ok(Self::Paused),
            "needs_review" => Ok(Self::NeedsReview),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown Local Agent run status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunRecord {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub input: Value,
    pub status: LocalAgentRunStatus,
    pub iteration: u32,
    pub max_iterations: u32,
    pub version: u64,
    pub claim_token: Option<String>,
    pub claim_until_unix_ms: Option<i64>,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub checkpoint: Value,
    pub continuation_input: Option<Value>,
    pub terminal_outcome: Option<Value>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunClaim {
    pub worker_id: String,
    pub claim_token: String,
    pub run: LocalAgentRunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentEventRecord {
    pub cursor: i64,
    pub event_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostResponseEnvelope {
    pub protocol_version: u32,
    pub command_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<HostResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<HostError>,
}

impl HostResponseEnvelope {
    pub fn success(command_id: String, result: HostResult) -> Self {
        Self {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(command_id: String, error: HostError) -> Self {
        Self {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostResult {
    Health {
        service: String,
        storage_ready: bool,
        recovered_claims: u64,
    },
    Run {
        run: LocalAgentRunRecord,
    },
    Claim {
        claim: Option<LocalAgentRunClaim>,
    },
    ToolClaim {
        claim: Option<LocalAgentToolClaim>,
    },
    ToolCommit {
        result: Box<LocalAgentToolCommitResult>,
    },
    Events {
        events: Vec<LocalAgentEventRecord>,
        next_cursor: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl HostError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

pub fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be 1..=256 non-control characters"));
    }
    Ok(())
}

pub fn validate_text(name: &str, value: &str, maximum_length: usize) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum_length || value.chars().any(|value| value == '\0')
    {
        return Err(format!(
            "{name} must be 1..={maximum_length} characters without NUL"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_round_trips_through_stable_wire_value() {
        for status in [
            LocalAgentRunStatus::Queued,
            LocalAgentRunStatus::ModelReady,
            LocalAgentRunStatus::ModelRunning,
            LocalAgentRunStatus::WaitingToolResult,
            LocalAgentRunStatus::ContinuationReady,
            LocalAgentRunStatus::WaitingUser,
            LocalAgentRunStatus::RetryScheduled,
            LocalAgentRunStatus::Paused,
            LocalAgentRunStatus::NeedsReview,
            LocalAgentRunStatus::Succeeded,
            LocalAgentRunStatus::Failed,
            LocalAgentRunStatus::Cancelled,
        ] {
            assert_eq!(LocalAgentRunStatus::from_str(status.as_str()), Ok(status));
        }
    }

    #[test]
    fn request_rejects_zero_version_and_unbounded_event_page() {
        let commit = CommitStepCommand {
            run_id: "run-1".to_string(),
            claim_token: "claim-1".to_string(),
            expected_version: 0,
            outcome: LocalAgentStepOutcome::Succeed {
                output: Value::Null,
            },
        };
        assert!(commit.validate().is_err());
        assert!(ListEventsCommand {
            after_cursor: 0,
            limit: LOCAL_AGENT_MAX_EVENT_PAGE_SIZE + 1,
            run_id: None,
        }
        .validate()
        .is_err());
    }
}
