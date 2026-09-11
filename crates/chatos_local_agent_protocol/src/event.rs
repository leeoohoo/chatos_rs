// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{require_bounded_json, require_identifier, ProtocolError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentEventType {
    RunStarted,
    ModelStepRequested,
    ModelStepCompleted,
    ToolBatchRequested,
    ToolBatchCompleted,
    ContinuationRequested,
    RetryDue,
    PauseRequested,
    ResumeRequested,
    CancelRequested,
    MemorySyncDue,
    RunTerminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentEventStatus {
    Pending,
    Claimed,
    Applied,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentEvent {
    pub event_id: String,
    pub run_id: String,
    pub event_type: LocalAgentEventType,
    pub expected_version: u64,
    pub available_at: DateTime<Utc>,
    pub status: LocalAgentEventStatus,
    pub attempt_count: u32,
    pub claimed_by_device_id: Option<String>,
    pub claim_token: Option<String>,
    pub claim_until: Option<DateTime<Utc>>,
    pub causation_id: String,
    pub correlation_id: String,
    pub bounded_payload: Value,
    pub last_error: Option<String>,
}

impl LocalAgentEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("event_id", self.event_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("causation_id", self.causation_id.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if self.expected_version == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "expected_version must be positive",
            });
        }
        let has_any_claim_field = self.claimed_by_device_id.is_some()
            || self.claim_token.is_some()
            || self.claim_until.is_some();
        let has_complete_claim = self.claimed_by_device_id.is_some()
            && self.claim_token.is_some()
            && self.claim_until.is_some();
        if has_any_claim_field != has_complete_claim
            || (self.status == LocalAgentEventStatus::Claimed) != has_complete_claim
        {
            return Err(ProtocolError::InvalidState {
                reason: "claimed event status and lease fields must change together",
            });
        }
        if let Some(device_id) = &self.claimed_by_device_id {
            require_identifier("claimed_by_device_id", device_id)?;
        }
        if let Some(claim_token) = &self.claim_token {
            require_identifier("claim_token", claim_token)?;
        }
        require_bounded_json("bounded_payload", &self.bounded_payload)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ModelStepResult {
    ToolCommand(Value),
    Continue(Value),
    Retry(Value),
    AskUser(Value),
    Final(Value),
    Failed(Value),
    Cancelled,
}
