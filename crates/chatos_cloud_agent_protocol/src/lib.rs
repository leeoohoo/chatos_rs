// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stable persistence and message contracts for cloud Agent orchestration.
//!
//! This crate deliberately contains no queue client, model client, MCP client, or
//! service-specific business logic. Cloud services persist these records and use
//! their identities for CAS/idempotency; RabbitMQ delivery order is never treated
//! as the source of truth.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CloudAgentOrdering {
    pub ordering_lane_key: String,
    pub lane_seq: u64,
    pub agent_run_id: String,
    pub generation: u64,
    pub step_seq: u64,
}

impl CloudAgentOrdering {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("ordering_lane_key", &self.ordering_lane_key)?;
        validate_identifier("agent_run_id", &self.agent_run_id)?;
        validate_positive("lane_seq", self.lane_seq)?;
        validate_positive("generation", self.generation)?;
        validate_positive("step_seq", self.step_seq)
    }

    pub fn step_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.agent_run_id, self.generation, self.step_seq
        )
    }

    pub fn next_step(&self) -> Result<Self, String> {
        self.validate()?;
        let step_seq = self
            .step_seq
            .checked_add(1)
            .ok_or_else(|| "step_seq overflow".to_string())?;
        Ok(Self {
            step_seq,
            ..self.clone()
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentRunStatus {
    Created,
    ModelReady,
    ModelRequesting,
    WaitingToolResult,
    RetryScheduled,
    Draining,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

impl CloudAgentRunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentRunPhase {
    WaitingLaneTurn,
    Ready,
    ModelRequest,
    ToolBatch,
    WaitingUser,
    RetryDelay,
    Draining,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentStepStatus {
    Ready,
    Requesting,
    Succeeded,
    RetryScheduled,
    Failed,
    UnknownModelRequestState,
}

impl CloudAgentStepStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::UnknownModelRequestState
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentRunRecord {
    pub ordering: CloudAgentOrdering,
    pub owner_service: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub owner_user_id: String,
    pub agent_key: String,
    #[serde(default)]
    pub input: Value,
    pub status: CloudAgentRunStatus,
    pub phase: CloudAgentRunPhase,
    pub iteration: u32,
    pub model_config_ref: String,
    pub model_runtime_snapshot_ref: String,
    pub agent_prompt_revision: String,
    pub agent_prompt_checksum: String,
    pub capability_policy_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_runtime_session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_batch_id: Option<String>,
    #[serde(default)]
    pub pending_tool_calls: Vec<Value>,
    #[serde(default)]
    pub pending_tool_results: Vec<Value>,
    /// Durable stateless Responses input for the current Agent run.
    /// Items are append-only in official API order: original turn input,
    /// complete response.output items, then function_call_output items.
    #[serde(default)]
    pub response_input_items: Vec<Value>,
    pub current_input_items_ref: String,
    #[serde(default)]
    pub usage_accumulator: Value,
    pub max_iterations: u32,
    pub retry_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    pub cancel_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_outcome: Option<Value>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CloudAgentRunRecord {
    pub fn validate(&self) -> Result<(), String> {
        self.ordering.validate()?;
        validate_identifier("owner_service", &self.owner_service)?;
        validate_identifier("owner_entity_type", &self.owner_entity_type)?;
        validate_identifier("owner_entity_id", &self.owner_entity_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("agent_key", &self.agent_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier(
            "model_runtime_snapshot_ref",
            &self.model_runtime_snapshot_ref,
        )?;
        validate_identifier("current_input_items_ref", &self.current_input_items_ref)?;
        validate_positive("version", self.version)?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        if self.status.is_terminal() != (self.phase == CloudAgentRunPhase::Terminal) {
            return Err("terminal status and terminal phase must change together".to_string());
        }
        if self.status == CloudAgentRunStatus::WaitingToolResult
            && self
                .pending_batch_id
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err("waiting_tool_result requires pending_batch_id".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentStepRecord {
    pub ordering: CloudAgentOrdering,
    pub request_hash: String,
    pub request_input_ref: String,
    pub model_attempt: u32,
    pub status: CloudAgentStepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub usage: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

impl CloudAgentStepRecord {
    pub fn validate(&self) -> Result<(), String> {
        self.ordering.validate()?;
        validate_identifier("request_hash", &self.request_hash)?;
        validate_identifier("request_input_ref", &self.request_input_ref)?;
        validate_positive("model_attempt", u64::from(self.model_attempt))?;
        if self.status.is_terminal() && self.finished_at.is_none() {
            return Err("terminal model step requires finished_at".to_string());
        }
        Ok(())
    }

    pub fn step_id(&self) -> String {
        self.ordering.step_id()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentStartEvent {
    pub event_id: String,
    pub owner_service: String,
    pub agent_key: String,
    pub ordering: CloudAgentOrdering,
    pub causation_id: String,
    pub correlation_id: String,
    pub occurred_at: DateTime<Utc>,
    #[serde(default)]
    pub payload: Value,
}

impl CloudAgentStartEvent {
    pub fn validate(&self) -> Result<(), String> {
        validate_event_identity(
            &self.event_id,
            &self.owner_service,
            &self.agent_key,
            &self.causation_id,
            &self.correlation_id,
        )?;
        self.ordering.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentRetryContinuation {
    pub event_id: String,
    pub owner_service: String,
    pub agent_key: String,
    pub ordering: CloudAgentOrdering,
    pub model_attempt: u32,
    pub retry_at: DateTime<Utc>,
    pub reason: String,
    pub input_event_id: String,
    #[serde(default)]
    pub payload: Value,
}

impl CloudAgentRetryContinuation {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("event_id", &self.event_id)?;
        validate_identifier("owner_service", &self.owner_service)?;
        validate_identifier("agent_key", &self.agent_key)?;
        validate_identifier("reason", &self.reason)?;
        validate_identifier("input_event_id", &self.input_event_id)?;
        validate_positive("model_attempt", u64::from(self.model_attempt))?;
        self.ordering.validate()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentEventType {
    RunStarted,
    CancelRequested,
    DeadlineReached,
    RecoveryRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentEventEnvelope {
    pub event_id: String,
    pub event_type: CloudAgentEventType,
    pub owner_service: String,
    pub agent_key: String,
    pub ordering: CloudAgentOrdering,
    pub causation_id: String,
    pub correlation_id: String,
    pub occurred_at: DateTime<Utc>,
    #[serde(default)]
    pub payload: Value,
}

impl CloudAgentEventEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        validate_event_identity(
            &self.event_id,
            &self.owner_service,
            &self.agent_key,
            &self.causation_id,
            &self.correlation_id,
        )?;
        self.ordering.validate()
    }
}

fn validate_event_identity(
    event_id: &str,
    owner_service: &str,
    agent_key: &str,
    causation_id: &str,
    correlation_id: &str,
) -> Result<(), String> {
    validate_identifier("event_id", event_id)?;
    validate_identifier("owner_service", owner_service)?;
    validate_identifier("agent_key", agent_key)?;
    validate_identifier("causation_id", causation_id)?;
    validate_identifier("correlation_id", correlation_id)
}

fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    if value.len() > 512 {
        return Err(format!("{name} must not exceed 512 bytes"));
    }
    Ok(())
}

fn validate_positive(name: &str, value: u64) -> Result<(), String> {
    if value == 0 {
        Err(format!("{name} must be greater than zero"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordering() -> CloudAgentOrdering {
        CloudAgentOrdering {
            ordering_lane_key: "conversation:conversation-1".to_string(),
            lane_seq: 2,
            agent_run_id: "run-1".to_string(),
            generation: 1,
            step_seq: 3,
        }
    }

    #[test]
    fn ordering_identity_is_stable_and_advances_only_the_step() {
        let current = ordering();
        assert_eq!(current.step_id(), "run-1:1:3");
        let next = current.next_step().unwrap();
        assert_eq!(next.step_seq, 4);
        assert_eq!(next.lane_seq, 2);
        assert_eq!(next.agent_run_id, "run-1");
    }

    #[test]
    fn ordering_rejects_zero_sequences_and_empty_ids() {
        let mut value = ordering();
        value.lane_seq = 0;
        assert_eq!(
            value.validate().unwrap_err(),
            "lane_seq must be greater than zero"
        );
        value.lane_seq = 1;
        value.agent_run_id = "  ".to_string();
        assert_eq!(
            value.validate().unwrap_err(),
            "agent_run_id must not be empty"
        );
    }

    #[test]
    fn terminal_status_and_phase_must_change_together() {
        let now = Utc::now();
        let record = CloudAgentRunRecord {
            ordering: ordering(),
            owner_service: "chatos".to_string(),
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: "turn-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "chatos_conversation_agent".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::Succeeded,
            phase: CloudAgentRunPhase::Ready,
            iteration: 1,
            model_config_ref: "model-config-1".to_string(),
            model_runtime_snapshot_ref: "model-snapshot-1".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "sha256:prompt".to_string(),
            capability_policy_revision: "1".to_string(),
            mcp_runtime_session_ref: None,
            previous_response_id: None,
            continuation_mode: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            current_input_items_ref: "input-1".to_string(),
            usage_accumulator: Value::Null,
            max_iterations: 100,
            retry_count: 0,
            deadline_at: None,
            cancel_requested: false,
            terminal_outcome: Some(Value::String("done".to_string())),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        assert_eq!(
            record.validate().unwrap_err(),
            "terminal status and terminal phase must change together"
        );
    }

    #[test]
    fn waiting_tool_result_requires_a_batch_identity() {
        let now = Utc::now();
        let record = CloudAgentRunRecord {
            ordering: ordering(),
            owner_service: "task-runner".to_string(),
            owner_entity_type: "task_run".to_string(),
            owner_entity_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::WaitingToolResult,
            phase: CloudAgentRunPhase::ToolBatch,
            iteration: 1,
            model_config_ref: "model-config-1".to_string(),
            model_runtime_snapshot_ref: "model-snapshot-1".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "sha256:prompt".to_string(),
            capability_policy_revision: "1".to_string(),
            mcp_runtime_session_ref: Some("mcp-session-1".to_string()),
            previous_response_id: None,
            continuation_mode: None,
            pending_batch_id: None,
            pending_tool_calls: vec![Value::String("call".to_string())],
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            current_input_items_ref: "input-1".to_string(),
            usage_accumulator: Value::Null,
            max_iterations: 100,
            retry_count: 0,
            deadline_at: None,
            cancel_requested: false,
            terminal_outcome: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        assert_eq!(
            record.validate().unwrap_err(),
            "waiting_tool_result requires pending_batch_id"
        );
    }
}
