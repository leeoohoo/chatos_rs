// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{require_bounded_json, require_digest, require_identifier, ProtocolError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    IdempotentWrite,
    Write,
    Billable,
    Terminal,
}

impl ToolEffect {
    pub const fn requires_approval(self) -> bool {
        !matches!(self, Self::Read)
    }

    pub const fn requires_durable_start(self) -> bool {
        !matches!(self, Self::Read)
    }

    pub const fn can_replay_after_started(self) -> bool {
        matches!(self, Self::Read | Self::IdempotentWrite)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionStatus {
    Requested,
    AwaitingApproval,
    Approved,
    Started,
    Succeeded,
    Failed,
    Rejected,
    OutcomeUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolExecution {
    pub invocation_id: String,
    pub run_id: String,
    pub batch_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub arguments_digest: String,
    pub status: ToolExecutionStatus,
    pub bounded_result: Option<Value>,
    pub approval_decided_at: Option<DateTime<Utc>>,
    pub approval_reason: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl ToolExecution {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("invocation_id", self.invocation_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("batch_id", self.batch_id.as_str()),
            ("tool_call_id", self.tool_call_id.as_str()),
            ("tool_name", self.tool_name.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        require_digest("arguments_digest", &self.arguments_digest)?;
        if let Some(result) = &self.bounded_result {
            require_bounded_json("bounded_result", result)?;
        }
        if self
            .approval_reason
            .as_deref()
            .is_some_and(|reason| reason.trim().is_empty())
        {
            return Err(ProtocolError::EmptyPayload {
                field: "approval_reason",
            });
        }
        if self.effect.requires_approval() && self.status == ToolExecutionStatus::Requested {
            return Err(ProtocolError::InvalidState {
                reason: "side-effecting tools must await a durable approval decision",
            });
        }
        if !self.effect.requires_approval()
            && matches!(
                self.status,
                ToolExecutionStatus::AwaitingApproval
                    | ToolExecutionStatus::Approved
                    | ToolExecutionStatus::Rejected
            )
        {
            return Err(ProtocolError::InvalidState {
                reason: "read-only tools cannot enter an approval state",
            });
        }
        let approval_decided = matches!(
            self.status,
            ToolExecutionStatus::Approved
                | ToolExecutionStatus::Started
                | ToolExecutionStatus::Succeeded
                | ToolExecutionStatus::Failed
                | ToolExecutionStatus::Rejected
                | ToolExecutionStatus::OutcomeUnknown
        ) && self.effect.requires_approval();
        if approval_decided != self.approval_decided_at.is_some()
            || (!self.effect.requires_approval() && self.approval_reason.is_some())
        {
            return Err(ProtocolError::InvalidState {
                reason: "tool approval decision metadata does not match its status and effect",
            });
        }
        let active = matches!(
            self.status,
            ToolExecutionStatus::Started
                | ToolExecutionStatus::Succeeded
                | ToolExecutionStatus::Failed
                | ToolExecutionStatus::OutcomeUnknown
        );
        if active != self.started_at.is_some() {
            return Err(ProtocolError::InvalidState {
                reason: "started tool status and started_at must change together",
            });
        }
        let complete = matches!(
            self.status,
            ToolExecutionStatus::Succeeded
                | ToolExecutionStatus::Failed
                | ToolExecutionStatus::Rejected
        );
        if complete != self.completed_at.is_some() {
            return Err(ProtocolError::InvalidState {
                reason: "completed tool status and completed_at must change together",
            });
        }
        Ok(())
    }
}
