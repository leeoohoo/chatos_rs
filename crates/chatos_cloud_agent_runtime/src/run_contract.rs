// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::CloudAgentClaim;
use crate::reducer::CloudAgentModelTrigger;
use crate::CloudAgentStateStore;
use async_trait::async_trait;
use chatos_cloud_agent_protocol::{
    CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunRecord, CloudAgentRunStatus,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentOutboxIntent {
    pub event_id: String,
    pub topic: String,
    pub routing_key: String,
    pub ordering: CloudAgentOrdering,
    pub causation_id: String,
    pub correlation_id: String,
    pub available_at: DateTime<Utc>,
    pub payload: Value,
}

impl CloudAgentOutboxIntent {
    pub fn validate(&self) -> Result<(), String> {
        self.ordering.validate()?;
        for (name, value) in [
            ("event_id", self.event_id.as_str()),
            ("topic", self.topic.as_str()),
            ("routing_key", self.routing_key.as_str()),
            ("causation_id", self.causation_id.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAgentAtomicTransition {
    pub claim: CloudAgentClaim,
    #[serde(default)]
    pub next_input: Value,
    pub next_status: CloudAgentRunStatus,
    pub next_phase: CloudAgentRunPhase,
    pub next_step_seq: u64,
    pub next_iteration: u32,
    pub next_retry_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_mode: Option<String>,
    pub current_input_items_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_runtime_session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_batch_id: Option<String>,
    #[serde(default)]
    pub pending_tool_calls: Vec<Value>,
    #[serde(default)]
    pub pending_tool_results: Vec<Value>,
    #[serde(default)]
    pub response_input_items: Vec<Value>,
    #[serde(default)]
    pub usage_accumulator: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_outcome: Option<Value>,
    #[serde(default)]
    pub outbox: Vec<CloudAgentOutboxIntent>,
}

impl CloudAgentAtomicTransition {
    pub fn validate(&self) -> Result<(), String> {
        self.claim.validate()?;
        if self.next_step_seq < self.claim.ordering.step_seq {
            return Err("next_step_seq cannot move backwards".to_string());
        }
        if self.current_input_items_ref.trim().is_empty() {
            return Err("current_input_items_ref must not be empty".to_string());
        }
        if self.next_status.is_terminal() != (self.next_phase == CloudAgentRunPhase::Terminal) {
            return Err("terminal status and terminal phase must change together".to_string());
        }
        if self.next_status == CloudAgentRunStatus::WaitingToolResult
            && self
                .pending_batch_id
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err("waiting_tool_result transition requires pending_batch_id".to_string());
        }
        for intent in &self.outbox {
            intent.validate()?;
            if intent.ordering.agent_run_id != self.claim.ordering.agent_run_id
                || intent.ordering.generation != self.claim.ordering.generation
                || intent.ordering.ordering_lane_key != self.claim.ordering.ordering_lane_key
                || intent.ordering.lane_seq != self.claim.ordering.lane_seq
            {
                return Err("outbox intent escaped the claimed ordering lane".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudAgentClaimResult {
    Acquired,
    Duplicate,
    OutOfOrder,
    Conflict,
    Terminal,
}

#[async_trait]
pub trait CloudAgentRunStore: Send + Sync {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String>;

    /// Must atomically compare lane, generation, step, phase and version.
    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String>;

    /// Extends an execution claim while the owner is still executing a model
    /// step. A model request may legitimately outlive the initial short lease;
    /// renewal must be conditional on the same ordering/version/token so a
    /// stale worker can never revive a claim that another worker acquired.
    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String>;

    /// Must persist state and all outbox intents in the same database transaction.
    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String>;

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct CloudAgentConsumeInput {
    pub agent_run_id: String,
    pub event_id: String,
    pub trigger: CloudAgentModelTrigger,
    pub expected_status: CloudAgentRunStatus,
    pub expected_phase: CloudAgentRunPhase,
    pub claim_token: String,
    pub claim_until: DateTime<Utc>,
    pub output_routing_key: String,
}

#[derive(Debug, Clone)]
pub struct NewCloudAgentRun {
    pub ordering_lane_key: String,
    pub agent_run_id: String,
    pub owner_service: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub input: Value,
    pub model_config_ref: String,
    pub model_runtime_snapshot_ref: String,
    pub agent_prompt_revision: String,
    pub agent_prompt_checksum: String,
    pub capability_policy_revision: String,
    pub mcp_runtime_session_ref: Option<String>,
    pub current_input_items_ref: String,
    pub max_iterations: u32,
    pub deadline_at: Option<DateTime<Utc>>,
    pub runtime_routing_key: String,
    pub start_causation_id: String,
    pub start_payload: Value,
}

pub async fn create_cloud_agent_run(
    store: &CloudAgentStateStore,
    new_run: NewCloudAgentRun,
) -> Result<CloudAgentRunRecord, String> {
    if new_run.max_iterations == 0 {
        return Err("Cloud Agent max_iterations must be greater than zero".to_string());
    }
    let lane_seq = store
        .allocate_lane_seq(new_run.ordering_lane_key.as_str())
        .await?;
    let ordering = CloudAgentOrdering {
        ordering_lane_key: new_run.ordering_lane_key,
        lane_seq,
        agent_run_id: new_run.agent_run_id,
        generation: 1,
        step_seq: 1,
    };
    let now = Utc::now();
    let record = CloudAgentRunRecord {
        ordering: ordering.clone(),
        owner_service: new_run.owner_service,
        owner_entity_type: new_run.owner_entity_type,
        owner_entity_id: new_run.owner_entity_id,
        owner_user_id: new_run.owner_user_id,
        agent_key: new_run.agent_key,
        input: new_run.input,
        status: CloudAgentRunStatus::ModelReady,
        phase: CloudAgentRunPhase::Ready,
        iteration: 0,
        model_config_ref: new_run.model_config_ref,
        model_runtime_snapshot_ref: new_run.model_runtime_snapshot_ref,
        agent_prompt_revision: new_run.agent_prompt_revision,
        agent_prompt_checksum: new_run.agent_prompt_checksum,
        capability_policy_revision: new_run.capability_policy_revision,
        mcp_runtime_session_ref: new_run.mcp_runtime_session_ref,
        previous_response_id: None,
        continuation_mode: Some("run_started".to_string()),
        pending_batch_id: None,
        pending_tool_calls: Vec::new(),
        pending_tool_results: Vec::new(),
        response_input_items: Vec::new(),
        current_input_items_ref: new_run.current_input_items_ref,
        usage_accumulator: Value::Null,
        max_iterations: new_run.max_iterations,
        retry_count: 0,
        deadline_at: new_run.deadline_at,
        cancel_requested: false,
        terminal_outcome: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let start_event_id = format!(
        "cloud_agent_run_started_{}_{}_{}",
        ordering.agent_run_id, ordering.generation, ordering.step_seq
    );
    let start_outbox = CloudAgentOutboxIntent {
        event_id: start_event_id.clone(),
        topic: "run_started".to_string(),
        routing_key: new_run.runtime_routing_key,
        ordering,
        causation_id: new_run.start_causation_id,
        correlation_id: record.ordering.agent_run_id.clone(),
        available_at: now,
        payload: merge_start_event_identity(new_run.start_payload, start_event_id),
    };
    store
        .insert_run_with_outbox(record.clone(), vec![start_outbox])
        .await?;
    Ok(record)
}

fn merge_start_event_identity(payload: Value, event_id: String) -> Value {
    let mut payload = payload.as_object().cloned().unwrap_or_default();
    payload.insert(
        "event_type".to_string(),
        Value::String("run_started".to_string()),
    );
    payload.insert("event_id".to_string(), Value::String(event_id));
    Value::Object(payload)
}
