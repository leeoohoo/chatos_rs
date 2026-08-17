// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Shared state reducer and persistence boundary used inside each owner
//! service's AI Runtime consumer. This crate is not another consumer or
//! orchestrator: it performs one claimed transition and returns outbox intents.

use async_trait::async_trait;
use chatos_ai_runtime::AiSingleStepOutcome;
use chatos_cloud_agent_protocol::{
    CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunRecord, CloudAgentRunStatus,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

mod mongo_store;
mod rabbitmq_driver;
mod state_store;

pub use mongo_store::{CloudAgentLaneRecord, MongoCloudAgentRunStore};
pub use rabbitmq_driver::{
    publish_cloud_agent_intent, spawn_cloud_agent_consumer, spawn_cloud_agent_outbox_reconciler,
    CloudAgentQueueOwner, CloudAgentRabbitMqTopology, CloudAgentServiceAdapter,
    CloudAgentServiceRuntime,
};
pub use state_store::{
    CloudAgentOutboxPublishFailure, CloudAgentStateStore, InMemoryCloudAgentRunStore,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudAgentClaim {
    pub ordering: CloudAgentOrdering,
    pub expected_status: CloudAgentRunStatus,
    pub expected_phase: CloudAgentRunPhase,
    pub expected_version: u64,
    pub claim_token: String,
    pub claim_until: DateTime<Utc>,
}

impl CloudAgentClaim {
    pub fn validate(&self) -> Result<(), String> {
        self.ordering.validate()?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if self.claim_token.trim().is_empty() {
            return Err("claim_token must not be empty".to_string());
        }
        if self.expected_status.is_terminal() {
            return Err("terminal runs cannot be claimed".to_string());
        }
        Ok(())
    }
}

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

pub fn cloud_agent_trigger_execution_identity(trigger: &CloudAgentModelTrigger) -> (String, usize) {
    match trigger {
        CloudAgentModelTrigger::RunStarted { .. } => ("initial".to_string(), 1),
        CloudAgentModelTrigger::ToolResults { .. } => ("tool_results".to_string(), 1),
        CloudAgentModelTrigger::Continuation { payload, .. } => (
            payload
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("continuation")
                .to_string(),
            1,
        ),
        CloudAgentModelTrigger::Retry {
            model_attempt,
            payload,
            ..
        } => (
            payload
                .get("retry_kind")
                .or_else(|| payload.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("model_retry")
                .to_string(),
            (*model_attempt).max(1),
        ),
    }
}

pub fn cloud_agent_trigger_input_items(
    run: &CloudAgentRunRecord,
    trigger: &CloudAgentModelTrigger,
    initial_input_items: Vec<Value>,
) -> Result<Vec<Value>, String> {
    match trigger {
        CloudAgentModelTrigger::RunStarted { .. } => Ok(initial_input_items),
        CloudAgentModelTrigger::Continuation { .. } | CloudAgentModelTrigger::Retry { .. } => {
            Ok(if run.response_input_items.is_empty() {
                initial_input_items
            } else {
                run.response_input_items.clone()
            })
        }
        CloudAgentModelTrigger::ToolResults { items, .. } => cloud_agent_mcp_result_input_items(
            run.response_input_items.as_slice(),
            run.pending_tool_calls.as_slice(),
            items.as_slice(),
        ),
    }
}

pub fn cloud_agent_mcp_result_input_items(
    response_input_items: &[Value],
    calls: &[Value],
    results: &[Value],
) -> Result<Vec<Value>, String> {
    if calls.len() != results.len() {
        return Err("MCP aggregate result count does not match pending tool calls".to_string());
    }
    let mut items = Vec::with_capacity(response_input_items.len().saturating_add(calls.len()));
    items.extend_from_slice(response_input_items);
    for (index, (call, result)) in calls.iter().zip(results).enumerate() {
        let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
            .ok_or_else(|| format!("pending tool call {index} has no call id"))?;
        let output = if result.get("status").and_then(Value::as_str) == Some("completed") {
            result
                .get("result")
                .cloned()
                .unwrap_or(Value::Null)
                .to_string()
        } else {
            result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("MCP tool call failed")
                .to_string()
        };
        items.push(
            chatos_ai_runtime::tool_call::build_function_call_output_item(call_id, output.as_str()),
        );
    }
    Ok(items)
}

pub fn cloud_agent_mcp_result_callback_payload(
    calls: &[Value],
    results: &[Value],
) -> Result<Value, String> {
    if calls.len() != results.len() {
        return Err("MCP aggregate result count does not match pending tool calls".to_string());
    }
    let tool_results = calls
        .iter()
        .zip(results)
        .enumerate()
        .map(|(index, (call, result))| {
            let tool_call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
                .ok_or_else(|| format!("pending tool call {index} has no call id"))?;
            let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                .ok_or_else(|| format!("pending tool call {index} has no name"))?;
            let completed = result.get("status").and_then(Value::as_str) == Some("completed");
            let content = if completed {
                result
                    .get("result")
                    .cloned()
                    .unwrap_or(Value::Null)
                    .to_string()
            } else {
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP tool call failed")
                    .to_string()
            };
            let mut payload = serde_json::json!({
                "tool_call_id": tool_call_id,
                "name": name,
                "success": completed,
                "is_error": !completed,
                "is_stream": false,
                "content": content,
                "result": result.get("result").cloned().unwrap_or(Value::Null),
                "error": result.get("error").cloned().unwrap_or(Value::Null),
            });
            if let Some(invocation_id) = call.get("invocation_id").and_then(Value::as_str) {
                payload["invocation_id"] = Value::String(invocation_id.to_string());
            }
            if let Some(conversation_turn_id) =
                call.get("conversation_turn_id").and_then(Value::as_str)
            {
                payload["conversation_turn_id"] = Value::String(conversation_turn_id.to_string());
            }
            Ok(payload)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(serde_json::json!({ "tool_results": tool_results }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudAgentConsumeDisposition {
    Committed,
    Duplicate,
    OutOfOrder,
    Conflict,
    Terminal,
}

#[derive(Debug)]
pub struct CloudAgentSingleStepOutput {
    pub outcome: AiSingleStepOutcome,
    pub next_input: Option<Value>,
    pub terminal_outcome_overlay: Option<Value>,
    pub mcp_runtime_session_ref: Option<String>,
    pub mcp_command_queue: Option<String>,
    pub retry_input_items: Option<Vec<Value>>,
}

impl CloudAgentSingleStepOutput {
    pub fn new(outcome: AiSingleStepOutcome) -> Self {
        Self {
            outcome,
            next_input: None,
            terminal_outcome_overlay: None,
            mcp_runtime_session_ref: None,
            mcp_command_queue: None,
            retry_input_items: None,
        }
    }

    pub fn with_mcp_runtime(
        mut self,
        session_ref: impl Into<String>,
        command_queue: impl Into<String>,
    ) -> Self {
        self.mcp_runtime_session_ref = Some(session_ref.into());
        self.mcp_command_queue = Some(command_queue.into());
        self
    }

    pub fn with_retry_input_items(mut self, input_items: Vec<Value>) -> Self {
        self.retry_input_items = Some(input_items);
        self
    }

    pub fn with_next_input(mut self, input: Value) -> Self {
        self.next_input = Some(input);
        self
    }

    pub fn with_terminal_outcome_overlay(mut self, overlay: Option<Value>) -> Self {
        self.terminal_outcome_overlay = overlay;
        self
    }
}

#[derive(Debug)]
pub enum CloudAgentSingleStepExecution {
    Apply(CloudAgentSingleStepOutput),
    AckWithoutTransition,
}

#[async_trait]
pub trait CloudAgentSingleStepExecutor: Send + Sync {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String>;
}

/// Business profile for one or more Agent keys owned by the same cloud
/// service. Profiles only implement one model step and terminal domain work;
/// delivery, ordering, claims, retries and outbox transitions remain in the
/// shared runtime.
#[async_trait]
pub trait CloudAgentProfile: Send + Sync {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String>;

    async fn finalize_terminal(&self, run: &CloudAgentRunRecord) -> Result<(), String>;
}

/// Routes all Agent keys owned by a service through the same durable cloud
/// runtime. A profile may be registered for several keys when those Agents
/// differ only by configuration/locality.
#[derive(Clone)]
pub struct CloudAgentProfileRegistry {
    owner_service: &'static str,
    store: CloudAgentStateStore,
    profiles: HashMap<String, Arc<dyn CloudAgentProfile>>,
}

impl CloudAgentProfileRegistry {
    pub fn new(owner_service: &'static str, store: CloudAgentStateStore) -> Self {
        Self {
            owner_service,
            store,
            profiles: HashMap::new(),
        }
    }

    pub fn register<I, K, P>(mut self, agent_keys: I, profile: P) -> Result<Self, String>
    where
        I: IntoIterator<Item = K>,
        K: Into<String>,
        P: CloudAgentProfile + 'static,
    {
        let profile: Arc<dyn CloudAgentProfile> = Arc::new(profile);
        let mut registered = 0usize;
        for key in agent_keys {
            let key = key.into();
            let key = key.trim();
            if key.is_empty() {
                return Err("Cloud Agent profile key must not be empty".to_string());
            }
            if self
                .profiles
                .insert(key.to_string(), Arc::clone(&profile))
                .is_some()
            {
                return Err(format!(
                    "Cloud Agent profile key is registered twice: {key}"
                ));
            }
            registered = registered.saturating_add(1);
        }
        if registered == 0 {
            return Err("Cloud Agent profile must register at least one key".to_string());
        }
        Ok(self)
    }

    fn profile_for(&self, run: &CloudAgentRunRecord) -> Result<Arc<dyn CloudAgentProfile>, String> {
        if run.owner_service != self.owner_service {
            return Err(format!(
                "Cloud Agent owner mismatch: expected {}, got {}",
                self.owner_service, run.owner_service
            ));
        }
        self.profiles
            .get(run.agent_key.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Cloud Agent profile is not registered for owner {} and key {}",
                    self.owner_service, run.agent_key
                )
            })
    }
}

#[async_trait]
impl CloudAgentSingleStepExecutor for CloudAgentProfileRegistry {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        self.profile_for(run)?
            .execute_single_step(run, trigger)
            .await
    }
}

/// Owns the complete durable transaction around one cloud Agent model step:
/// load, batch identity validation, short CAS claim, one owner execution,
/// reducer, outbox materialization and atomic commit.
pub async fn consume_cloud_agent_single_step<S, E>(
    store: &S,
    executor: &E,
    input: CloudAgentConsumeInput,
) -> Result<CloudAgentConsumeDisposition, String>
where
    S: CloudAgentRunStore,
    E: CloudAgentSingleStepExecutor,
{
    if input.agent_run_id.trim().is_empty()
        || input.event_id.trim().is_empty()
        || input.claim_token.trim().is_empty()
        || input.output_routing_key.trim().is_empty()
    {
        return Err("cloud agent consumer input contains an empty identity".to_string());
    }
    let Some(run) = store.load_run(input.agent_run_id.as_str()).await? else {
        return Ok(CloudAgentConsumeDisposition::Conflict);
    };
    if run.status.is_terminal() {
        return Ok(CloudAgentConsumeDisposition::Terminal);
    }
    if let CloudAgentModelTrigger::ToolResults {
        batch_id,
        source_step_seq,
        items,
        ..
    } = &input.trigger
    {
        if run.pending_batch_id.as_deref() != Some(batch_id.as_str())
            || run.ordering.step_seq != source_step_seq.saturating_add(1)
            || run.pending_tool_calls.len() != items.len()
        {
            return Ok(CloudAgentConsumeDisposition::Conflict);
        }
    }
    // The delivery may have waited in the queue long enough for its original
    // timestamp to pass. Once the claim is acquired, always give this owner a
    // fresh lease; otherwise the first heartbeat would itself race the
    // expiration and permit a duplicate consumer.
    let now = chrono::Utc::now();
    let requested_lease = input.claim_until - now;
    let claim_lease = if requested_lease > chrono::Duration::zero() {
        requested_lease
    } else {
        chrono::Duration::seconds(30)
    };
    let claim = CloudAgentClaim {
        ordering: run.ordering.clone(),
        expected_status: input.expected_status,
        expected_phase: input.expected_phase,
        expected_version: run.version,
        claim_token: input.claim_token,
        claim_until: now + claim_lease,
    };
    match store.acquire_short_claim(&claim).await? {
        CloudAgentClaimResult::Acquired => {}
        CloudAgentClaimResult::Duplicate => return Ok(CloudAgentConsumeDisposition::Duplicate),
        CloudAgentClaimResult::OutOfOrder => return Ok(CloudAgentConsumeDisposition::OutOfOrder),
        CloudAgentClaimResult::Conflict => return Ok(CloudAgentConsumeDisposition::Conflict),
        CloudAgentClaimResult::Terminal => return Ok(CloudAgentConsumeDisposition::Terminal),
    }
    // A model request can take longer than the initial short claim (streaming
    // gateways and MCP setup regularly do). Renew at one third of the lease so
    // another consumer cannot start the same step while this owner is still
    // executing. The original lease duration is derived from the delivery
    // envelope, keeping custom runtimes and tests deterministic.
    let heartbeat_interval = std::time::Duration::from_millis(
        u64::try_from((claim_lease.num_milliseconds() / 3).max(1)).unwrap_or(1),
    );
    let result = async {
        // An owner/profile error means this durable model step could not be
        // prepared or executed. Model-provider retries are represented by the
        // explicit `AiSingleStepOutcome::Retry` variant; blindly returning the
        // owner error to the MQ consumer would release the claim and replay the
        // same delivery forever. Persist it as a terminal failure instead so
        // the run is finalized exactly once and the user can start a fresh run.
        let execution = match executor.execute_single_step(&run, &input.trigger).await {
            Ok(execution) => execution,
            Err(error) => CloudAgentSingleStepExecution::Apply(CloudAgentSingleStepOutput::new(
                AiSingleStepOutcome::Failed { error },
            )),
        };
        let CloudAgentSingleStepExecution::Apply(output) = execution else {
            return Ok(None);
        };
        let mut transition = reduce_single_step(
            &run,
            claim.clone(),
            input.event_id.as_str(),
            input.output_routing_key.as_str(),
            output.outcome,
        )?;
        if let Some(next_input) = output.next_input {
            transition.next_input = next_input;
        }
        if transition.next_status == CloudAgentRunStatus::RetryScheduled {
            if let Some(input_items) = output.retry_input_items.as_ref() {
                transition.response_input_items = input_items.clone();
            }
        }
        if transition.next_status.is_terminal() {
            if let Some(overlay) = output.terminal_outcome_overlay {
                transition.terminal_outcome = Some(merge_terminal_outcome_overlay(
                    transition.terminal_outcome.take(),
                    overlay,
                ));
                if let Some(terminal_outcome) = transition.terminal_outcome.clone() {
                    for intent in &mut transition.outbox {
                        if intent.topic == "owner_lifecycle_terminal" {
                            intent.payload["terminal_outcome"] = terminal_outcome.clone();
                        }
                    }
                }
            }
        }
        if let Some(session_ref) = output.mcp_runtime_session_ref {
            transition.mcp_runtime_session_ref = Some(session_ref);
        }
        for intent in &mut transition.outbox {
            if intent.topic == "ai_runtime_retry" {
                if let Some(input_items) = output.retry_input_items.as_ref() {
                    intent.payload["input_items"] = Value::Array(input_items.clone());
                }
            } else if intent.topic == "mcp_tool_call_command" {
                let command_queue = output
                    .mcp_command_queue
                    .as_deref()
                    .ok_or_else(|| "Cloud Agent MCP command queue was not provided".to_string())?;
                intent.routing_key = command_queue.to_string();
                let session_ref = transition
                    .mcp_runtime_session_ref
                    .as_deref()
                    .ok_or_else(|| "Cloud Agent MCP session was not persisted".to_string())?;
                let transition_run = CloudAgentRunRecord {
                    mcp_runtime_session_ref: Some(session_ref.to_string()),
                    ..run.clone()
                };
                materialize_mcp_command(
                    &transition_run,
                    intent,
                    session_ref,
                    input.output_routing_key.as_str(),
                )?;
            }
        }
        store.commit_transition(transition).await.map(Some)
    };
    tokio::pin!(result);
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + heartbeat_interval,
        heartbeat_interval,
    );
    let result = loop {
        tokio::select! {
            result = &mut result => break result,
            _ = heartbeat.tick() => {
                let mut renewal = claim.clone();
                renewal.claim_until = chrono::Utc::now() + claim_lease;
                match store.renew_short_claim(&renewal).await {
                    Ok(true) => {}
                    Ok(false) => {
                        break Err("Cloud Agent execution claim was lost before the model step completed".to_string());
                    }
                    Err(error) => break Err(error),
                }
            }
        }
    };
    match result {
        Ok(Some(true)) => Ok(CloudAgentConsumeDisposition::Committed),
        Ok(Some(false)) => {
            store.release_short_claim(&claim).await?;
            Ok(CloudAgentConsumeDisposition::Conflict)
        }
        Ok(None) => {
            store.release_short_claim(&claim).await?;
            Ok(CloudAgentConsumeDisposition::Committed)
        }
        Err(error) => {
            store.release_short_claim(&claim).await?;
            Err(error)
        }
    }
}

fn merge_terminal_outcome_overlay(base: Option<Value>, overlay: Value) -> Value {
    match (base, overlay) {
        (Some(Value::Object(mut base)), Value::Object(overlay)) => {
            base.extend(overlay);
            Value::Object(base)
        }
        (_, overlay) => overlay,
    }
}

fn append_response_output_items(
    request_input_items: &[Value],
    response_output_items: &[Value],
    fallback_tool_calls: Option<&Value>,
) -> Vec<Value> {
    let mut items = request_input_items.to_vec();
    if response_output_items.is_empty() {
        if let Some(calls) = fallback_tool_calls.and_then(Value::as_array) {
            items.extend(calls.iter().filter_map(|call| {
                let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)?;
                let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)?;
                let arguments = chatos_ai_runtime::tool_call::tool_call_arguments_text(call);
                Some(chatos_ai_runtime::tool_call::build_function_call_item(
                    call_id,
                    name,
                    arguments.as_str(),
                ))
            }));
        }
    } else {
        items.extend_from_slice(response_output_items);
    }
    items
}

fn append_continuation_items(
    request_input_items: &[Value],
    response_output_items: &[Value],
    continuation_items: &[Value],
) -> Vec<Value> {
    let mut items = append_response_output_items(request_input_items, response_output_items, None);
    items.extend_from_slice(continuation_items);
    items
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CloudAgentModelTrigger {
    RunStarted {
        event_id: String,
        payload: Value,
    },
    ToolResults {
        event_id: String,
        batch_id: String,
        source_step_seq: u64,
        items: Vec<Value>,
    },
    Continuation {
        event_id: String,
        payload: Value,
    },
    Retry {
        event_id: String,
        model_attempt: usize,
        payload: Value,
    },
}

pub fn reduce_single_step(
    run: &CloudAgentRunRecord,
    claim: CloudAgentClaim,
    causation_id: &str,
    result_routing_key: &str,
    outcome: AiSingleStepOutcome,
) -> Result<CloudAgentAtomicTransition, String> {
    claim.validate()?;
    if claim.ordering != run.ordering {
        return Err("claim ordering does not match persisted run".to_string());
    }
    let next_step_seq = claim
        .ordering
        .step_seq
        .checked_add(1)
        .ok_or_else(|| "step_seq overflow".to_string())?;
    let next_iteration = run.iteration.saturating_add(1);
    let transition = match outcome {
        AiSingleStepOutcome::ToolCommand {
            response,
            tool_calls,
        } => {
            let batch_id = stable_batch_id(&claim.ordering);
            CloudAgentAtomicTransition {
                claim: claim.clone(),
                next_input: run.input.clone(),
                next_status: CloudAgentRunStatus::WaitingToolResult,
                next_phase: CloudAgentRunPhase::ToolBatch,
                next_step_seq,
                next_iteration,
                next_retry_count: 0,
                previous_response_id: None,
                continuation_mode: Some("mcp_tool_results".to_string()),
                current_input_items_ref: format!(
                    "cloud_agent:{}:{}:{}:tool_results",
                    claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
                ),
                mcp_runtime_session_ref: run.mcp_runtime_session_ref.clone(),
                pending_batch_id: Some(batch_id.clone()),
                pending_tool_calls: tool_calls.as_array().cloned().unwrap_or_default(),
                pending_tool_results: Vec::new(),
                response_input_items: append_response_output_items(
                    response.request_input_items.as_slice(),
                    response.response_output_items.as_slice(),
                    Some(&tool_calls),
                ),
                terminal_outcome: None,
                outbox: vec![outbox_intent(
                    &claim.ordering,
                    causation_id,
                    "mcp_tool_call_command",
                    result_routing_key,
                    serde_json::json!({
                        "batch_id": batch_id,
                        "source_step_seq": claim.ordering.step_seq,
                        "calls": tool_calls,
                        "response_id": response.response_id,
                    }),
                    Utc::now(),
                )],
            }
        }
        AiSingleStepOutcome::Continue {
            response,
            input_items,
            reason,
        } => CloudAgentAtomicTransition {
            claim: claim.clone(),
            next_input: run.input.clone(),
            next_status: CloudAgentRunStatus::ModelReady,
            next_phase: CloudAgentRunPhase::Ready,
            next_step_seq,
            next_iteration,
            next_retry_count: 0,
            previous_response_id: None,
            continuation_mode: Some(reason.clone()),
            current_input_items_ref: format!(
                "cloud_agent:{}:{}:{}:continuation",
                claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
            ),
            mcp_runtime_session_ref: run.mcp_runtime_session_ref.clone(),
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: append_continuation_items(
                response.request_input_items.as_slice(),
                response.response_output_items.as_slice(),
                input_items.as_slice(),
            ),
            terminal_outcome: None,
            outbox: vec![outbox_intent(
                &claim.ordering,
                causation_id,
                "ai_runtime_continuation",
                result_routing_key,
                serde_json::json!({
                    "reason": reason,
                    "input_items": input_items,
                    "response_id": response.response_id,
                }),
                Utc::now(),
            )],
        },
        AiSingleStepOutcome::Retry {
            error,
            retry_kind,
            next_model_attempt,
            backoff_ms,
            disable_stream,
            downgrade_thinking_to,
        } => CloudAgentAtomicTransition {
            claim: claim.clone(),
            next_input: run.input.clone(),
            next_status: CloudAgentRunStatus::RetryScheduled,
            next_phase: CloudAgentRunPhase::RetryDelay,
            next_step_seq: claim.ordering.step_seq,
            next_iteration: run.iteration,
            next_retry_count: u32::try_from(next_model_attempt.saturating_sub(1))
                .unwrap_or(u32::MAX),
            previous_response_id: run.previous_response_id.clone(),
            continuation_mode: run.continuation_mode.clone(),
            current_input_items_ref: run.current_input_items_ref.clone(),
            mcp_runtime_session_ref: run.mcp_runtime_session_ref.clone(),
            pending_batch_id: run.pending_batch_id.clone(),
            pending_tool_calls: run.pending_tool_calls.clone(),
            pending_tool_results: run.pending_tool_results.clone(),
            response_input_items: run.response_input_items.clone(),
            terminal_outcome: None,
            outbox: vec![retry_outbox_intent(
                &claim.ordering,
                causation_id,
                result_routing_key,
                serde_json::json!({
                    "error": error,
                    "retry_kind": retry_kind,
                    "model_attempt": next_model_attempt,
                    "disable_stream": disable_stream,
                    "downgrade_thinking_to": downgrade_thinking_to,
                }),
                next_model_attempt,
                Utc::now()
                    + chrono::Duration::milliseconds(i64::try_from(backoff_ms).unwrap_or(i64::MAX)),
            )],
        },
        AiSingleStepOutcome::Final(result) => CloudAgentAtomicTransition {
            claim: claim.clone(),
            next_input: run.input.clone(),
            next_status: CloudAgentRunStatus::Succeeded,
            next_phase: CloudAgentRunPhase::Terminal,
            next_step_seq,
            next_iteration,
            next_retry_count: 0,
            previous_response_id: None,
            continuation_mode: None,
            current_input_items_ref: run.current_input_items_ref.clone(),
            mcp_runtime_session_ref: run.mcp_runtime_session_ref.clone(),
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: append_response_output_items(
                result.request_input_items.as_slice(),
                result.response_output_items.as_slice(),
                result.tool_calls.as_ref(),
            ),
            terminal_outcome: Some(serde_json::json!({
                "content": result.content,
                "reasoning": result.reasoning,
                "finish_reason": result.finish_reason,
                "usage": result.usage,
                "response_id": result.response_id,
            })),
            outbox: vec![terminal_outbox_intent(
                &claim.ordering,
                causation_id,
                CloudAgentRunStatus::Succeeded,
                serde_json::json!({
                    "content": result.content,
                    "reasoning": result.reasoning,
                    "finish_reason": result.finish_reason,
                    "usage": result.usage,
                    "response_id": result.response_id,
                }),
            )],
        },
        AiSingleStepOutcome::Failed { error } => terminal_transition(
            claim,
            run.input.clone(),
            run.mcp_runtime_session_ref.clone(),
            CloudAgentRunStatus::Failed,
            next_step_seq,
            next_iteration,
            serde_json::json!({"error": error}),
        ),
        AiSingleStepOutcome::Cancelled => terminal_transition(
            claim,
            run.input.clone(),
            run.mcp_runtime_session_ref.clone(),
            CloudAgentRunStatus::Cancelled,
            next_step_seq,
            next_iteration,
            serde_json::json!({"cancelled": true}),
        ),
    };
    transition.validate()?;
    Ok(transition)
}

pub fn materialize_mcp_command(
    run: &CloudAgentRunRecord,
    intent: &CloudAgentOutboxIntent,
    mcp_runtime_session_ref: &str,
    result_routing_key: &str,
) -> Result<chatos_mcp_service::McpToolCallCommand, String> {
    if intent.topic != "mcp_tool_call_command" {
        return Err("only MCP tool command intents can be materialized".to_string());
    }
    let batch_id = intent
        .payload
        .get("batch_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP command intent is missing batch_id".to_string())?;
    let source_step_seq = intent
        .payload
        .get("source_step_seq")
        .and_then(Value::as_u64)
        .ok_or_else(|| "MCP command intent is missing source_step_seq".to_string())?;
    let calls = intent
        .payload
        .get("calls")
        .and_then(Value::as_array)
        .ok_or_else(|| "MCP command intent is missing calls".to_string())?
        .iter()
        .enumerate()
        .map(|(call_index, call)| {
            let tool_call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
                .ok_or_else(|| format!("MCP command tool call {call_index} is missing id"))?;
            let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                .ok_or_else(|| format!("MCP command tool call {call_index} is missing name"))?;
            let (arguments, preflight_error) =
                match chatos_ai_runtime::tool_call::clone_tool_call_arguments(call) {
                    Value::Object(arguments) => (Value::Object(arguments), None),
                    Value::String(arguments) => match serde_json::from_str::<Value>(&arguments) {
                        Ok(Value::Object(arguments)) => (Value::Object(arguments), None),
                        Ok(_) => (
                            Value::Object(Default::default()),
                            Some("tool arguments must be an object".to_string()),
                        ),
                        Err(error) => (
                            Value::Object(Default::default()),
                            Some(format!("invalid tool arguments: {error}")),
                        ),
                    },
                    _ => (
                        Value::Object(Default::default()),
                        Some("tool arguments must be an object".to_string()),
                    ),
                };
            Ok(chatos_mcp_service::McpToolCallCommandItem {
                invocation_id: format!("{batch_id}:{call_index}"),
                tool_call_id: tool_call_id.to_string(),
                call_index,
                name: name.to_string(),
                arguments,
                preflight_error,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let command = chatos_mcp_service::McpToolCallCommand {
        owner_service: run.owner_service.clone(),
        agent_run_id: run.ordering.agent_run_id.clone(),
        agent_key: run.agent_key.clone(),
        ordering_lane_key: run.ordering.ordering_lane_key.clone(),
        lane_seq: run.ordering.lane_seq,
        generation: run.ordering.generation,
        source_step_seq,
        batch_id: batch_id.to_string(),
        mcp_runtime_session_ref: mcp_runtime_session_ref.to_string(),
        result_routing_key: result_routing_key.to_string(),
        calls,
        delivery_attempt: 1,
    };
    command.validate()?;
    Ok(command)
}

fn terminal_transition(
    claim: CloudAgentClaim,
    next_input: Value,
    mcp_runtime_session_ref: Option<String>,
    status: CloudAgentRunStatus,
    next_step_seq: u64,
    next_iteration: u32,
    terminal_outcome: Value,
) -> CloudAgentAtomicTransition {
    let current_input_items_ref = format!(
        "cloud_agent:{}:{}:{}:terminal",
        claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
    );
    let terminal_event = terminal_outbox_intent(
        &claim.ordering,
        claim.claim_token.as_str(),
        status,
        terminal_outcome.clone(),
    );
    CloudAgentAtomicTransition {
        claim,
        next_input,
        next_status: status,
        next_phase: CloudAgentRunPhase::Terminal,
        next_step_seq,
        next_iteration,
        next_retry_count: 0,
        previous_response_id: None,
        continuation_mode: None,
        current_input_items_ref,
        mcp_runtime_session_ref,
        pending_batch_id: None,
        pending_tool_calls: Vec::new(),
        pending_tool_results: Vec::new(),
        response_input_items: Vec::new(),
        terminal_outcome: Some(terminal_outcome),
        outbox: vec![terminal_event],
    }
}

fn terminal_outbox_intent(
    ordering: &CloudAgentOrdering,
    causation_id: &str,
    status: CloudAgentRunStatus,
    terminal_outcome: Value,
) -> CloudAgentOutboxIntent {
    outbox_intent(
        ordering,
        causation_id,
        "owner_lifecycle_terminal",
        "owner_lifecycle_terminal",
        serde_json::json!({
            "status": status,
            "terminal_outcome": terminal_outcome,
        }),
        Utc::now(),
    )
}

fn stable_batch_id(ordering: &CloudAgentOrdering) -> String {
    format!(
        "mcp_batch_{}_{}_{}",
        ordering.agent_run_id, ordering.generation, ordering.step_seq
    )
}

fn retry_outbox_intent(
    ordering: &CloudAgentOrdering,
    causation_id: &str,
    routing_key: &str,
    payload: Value,
    next_model_attempt: usize,
    available_at: DateTime<Utc>,
) -> CloudAgentOutboxIntent {
    let mut intent = outbox_intent(
        ordering,
        causation_id,
        "ai_runtime_retry",
        routing_key,
        payload,
        available_at,
    );
    intent.event_id = format!("{}_attempt_{next_model_attempt}", intent.event_id);
    intent
}

fn outbox_intent(
    ordering: &CloudAgentOrdering,
    causation_id: &str,
    topic: &str,
    routing_key: &str,
    payload: Value,
    available_at: DateTime<Utc>,
) -> CloudAgentOutboxIntent {
    CloudAgentOutboxIntent {
        event_id: format!(
            "{}_{}_{}_{}",
            topic, ordering.agent_run_id, ordering.generation, ordering.step_seq
        ),
        topic: topic.to_string(),
        routing_key: routing_key.to_string(),
        ordering: ordering.clone(),
        causation_id: causation_id.to_string(),
        correlation_id: ordering.agent_run_id.clone(),
        available_at,
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_ai_runtime::AiRuntimeResult;
    use std::sync::{Arc, Mutex};

    fn ordering() -> CloudAgentOrdering {
        CloudAgentOrdering {
            ordering_lane_key: "task:task-1".to_string(),
            lane_seq: 1,
            agent_run_id: "run-1".to_string(),
            generation: 1,
            step_seq: 2,
        }
    }

    fn run() -> CloudAgentRunRecord {
        let now = Utc::now();
        CloudAgentRunRecord {
            ordering: ordering(),
            owner_service: "task-runner".to_string(),
            owner_entity_type: "task_run".to_string(),
            owner_entity_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::ModelRequesting,
            phase: CloudAgentRunPhase::ModelRequest,
            iteration: 2,
            model_config_ref: "model-1".to_string(),
            model_runtime_snapshot_ref: "snapshot-1".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "prompt-sha".to_string(),
            capability_policy_revision: "1".to_string(),
            mcp_runtime_session_ref: Some("session-1".to_string()),
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
            terminal_outcome: None,
            version: 4,
            created_at: now,
            updated_at: now,
        }
    }

    fn claim() -> CloudAgentClaim {
        CloudAgentClaim {
            ordering: ordering(),
            expected_status: CloudAgentRunStatus::ModelRequesting,
            expected_phase: CloudAgentRunPhase::ModelRequest,
            expected_version: 4,
            claim_token: "claim-1".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        }
    }

    #[test]
    fn tool_batch_uses_a_stable_step_identity_and_one_outbox_command() {
        let transition = reduce_single_step(
            &run(),
            claim(),
            "start-event-1",
            "cloud_agent.task_runner.mcp_results",
            AiSingleStepOutcome::ToolCommand {
                response: AiRuntimeResult {
                    content: String::new(),
                    reasoning: None,
                    tool_calls: None,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                    response_id: Some("response-1".to_string()),
                    response_output_items: Vec::new(),
                    request_input_items: Vec::new(),
                },
                tool_calls: serde_json::json!([{"id": "call-1"}]),
            },
        )
        .unwrap();
        assert_eq!(
            transition.next_status,
            CloudAgentRunStatus::WaitingToolResult
        );
        assert_eq!(transition.outbox.len(), 1);
        assert_eq!(
            transition.pending_batch_id.as_deref(),
            Some("mcp_batch_run-1_1_2")
        );
    }

    #[test]
    fn mcp_results_resume_with_ordered_call_and_output_pairs() {
        let calls = serde_json::json!([
            {
                "id": "call-1",
                "function": {"name": "CodeMaintainer", "arguments": "{\"path\":\"README.md\"}"}
            },
            {
                "id": "call-2",
                "function": {"name": "Terminal", "arguments": "{\"command\":\"python -m unittest\"}"}
            }
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": {"written": true}},
            {"status": "failed", "error": "tests failed"}
        ]);

        let response_output = serde_json::json!([
            {"type":"reasoning","id":"rs-1","summary":[{"type":"summary_text","text":"inspect"}]},
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"CodeMaintainer","arguments":"{\"path\":\"README.md\"}"},
            {"type":"function_call","id":"fc-2","call_id":"call-2","name":"Terminal","arguments":"{\"command\":\"python -m unittest\"}"}
        ]);
        let items = cloud_agent_mcp_result_input_items(
            response_output.as_array().unwrap(),
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(items.len(), 5);
        assert_eq!(items[..3], response_output.as_array().unwrap()[..]);
        assert_eq!(items[3]["type"], "function_call_output");
        assert_eq!(items[3]["call_id"], "call-1");
        assert_eq!(items[3]["output"], "{\"written\":true}");
        assert_eq!(items[4]["type"], "function_call_output");
        assert_eq!(items[4]["call_id"], "call-2");
        assert_eq!(items[4]["output"], "tests failed");
    }

    #[test]
    fn mcp_result_callback_payload_preserves_call_identity() {
        let calls = serde_json::json!([
            {
                "id": "call-1",
                "invocation_id": "invocation-1",
                "conversation_turn_id": "turn-1",
                "function": {"name": "Terminal", "arguments": "{}"}
            }
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": {"background": true, "busy": true}}
        ]);

        let payload = cloud_agent_mcp_result_callback_payload(
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();
        let result = &payload["tool_results"][0];

        assert_eq!(result["tool_call_id"], "call-1");
        assert_eq!(result["invocation_id"], "invocation-1");
        assert_eq!(result["conversation_turn_id"], "turn-1");
        assert_eq!(result["is_stream"], false);
        assert_eq!(result["success"], true);
    }

    #[test]
    fn a_single_mcp_result_uses_the_same_call_and_output_pair() {
        let calls = serde_json::json!([
            {"id": "call-1", "function": {"name": "TaskProcessLog", "arguments": "{}"}}
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": "recorded"}
        ]);

        let response_output = serde_json::json!([
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"TaskProcessLog","arguments":"{}"}
        ]);
        let items = cloud_agent_mcp_result_input_items(
            response_output.as_array().unwrap(),
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "function_call");
        assert_eq!(items[0]["call_id"], "call-1");
        assert_eq!(items[1]["type"], "function_call_output");
        assert_eq!(items[1]["call_id"], "call-1");
    }

    #[test]
    fn multiple_tool_batches_append_without_rewriting_the_previous_prefix() {
        let first_history = serde_json::json!([
            {"role":"user","content":"implement"},
            {"type":"reasoning","id":"rs-1","summary":[]},
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"read","arguments":"{}"}
        ]);
        let first_calls = serde_json::json!([
            {"id":"call-1","function":{"name":"read","arguments":"{}"}}
        ]);
        let first_results = serde_json::json!([
            {"status":"completed","result":"contents"}
        ]);
        let batch_one = cloud_agent_mcp_result_input_items(
            first_history.as_array().unwrap(),
            first_calls.as_array().unwrap(),
            first_results.as_array().unwrap(),
        )
        .unwrap();
        let second_output = serde_json::json!([
            {"type":"reasoning","id":"rs-2","summary":[]},
            {"type":"function_call","id":"fc-2","call_id":"call-2","name":"write","arguments":"{}"}
        ]);
        let second_history = append_response_output_items(
            batch_one.as_slice(),
            second_output.as_array().unwrap(),
            None,
        );
        let second_calls = serde_json::json!([
            {"id":"call-2","function":{"name":"write","arguments":"{}"}}
        ]);
        let second_results = serde_json::json!([
            {"status":"completed","result":{"written":true}}
        ]);
        let batch_two = cloud_agent_mcp_result_input_items(
            second_history.as_slice(),
            second_calls.as_array().unwrap(),
            second_results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(&batch_two[..batch_one.len()], batch_one.as_slice());
        assert_eq!(batch_two.last().unwrap()["call_id"], "call-2");
    }

    #[test]
    fn retry_does_not_advance_the_model_step() {
        let transition = reduce_single_step(
            &run(),
            claim(),
            "start-event-1",
            "cloud_agent.task_runner.retries",
            AiSingleStepOutcome::Retry {
                error: "timeout".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 500,
                disable_stream: false,
                downgrade_thinking_to: None,
            },
        )
        .unwrap();
        assert_eq!(transition.next_step_seq, 2);
        assert_eq!(transition.next_retry_count, 1);
        assert_eq!(transition.outbox.len(), 1);
        assert_eq!(
            transition.outbox[0].event_id,
            "ai_runtime_retry_run-1_1_2_attempt_2"
        );
    }

    #[derive(Clone)]
    struct TestSingleStepExecutor {
        outcome: AiSingleStepOutcome,
        seen_triggers: Arc<Mutex<Vec<CloudAgentModelTrigger>>>,
    }

    #[async_trait]
    impl CloudAgentSingleStepExecutor for TestSingleStepExecutor {
        async fn execute_single_step(
            &self,
            _run: &CloudAgentRunRecord,
            trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            self.seen_triggers.lock().unwrap().push(trigger.clone());
            Ok(CloudAgentSingleStepExecution::Apply(
                CloudAgentSingleStepOutput::new(self.outcome.clone())
                    .with_mcp_runtime("session-1", "mcp.commands")
                    .with_retry_input_items(vec![serde_json::json!({"type": "message"})]),
            ))
        }
    }

    async fn inserted_ready_run() -> InMemoryCloudAgentRunStore {
        let store = InMemoryCloudAgentRunStore::new();
        store.allocate_lane_seq("task:task-1").await.unwrap();
        let mut record = run();
        record.status = CloudAgentRunStatus::ModelReady;
        record.phase = CloudAgentRunPhase::Ready;
        record.ordering.step_seq = 1;
        record.iteration = 0;
        record.version = 1;
        store.insert_run(record).await.unwrap();
        store
    }

    fn consume_input() -> CloudAgentConsumeInput {
        CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: "run-started-1".to_string(),
            trigger: CloudAgentModelTrigger::RunStarted {
                event_id: "run-started-1".to_string(),
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::ModelReady,
            expected_phase: CloudAgentRunPhase::Ready,
            claim_token: "claim-single-step".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        }
    }

    fn tool_outcome(call_count: usize) -> AiSingleStepOutcome {
        AiSingleStepOutcome::ToolCommand {
            response: AiRuntimeResult {
                content: String::new(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("tool_calls".to_string()),
                usage: None,
                response_id: Some("response-1".to_string()),
                response_output_items: Vec::new(),
                request_input_items: Vec::new(),
            },
            tool_calls: Value::Array(
                (0..call_count)
                    .map(|index| {
                        serde_json::json!({
                            "id": format!("call-{index}"),
                            "function": {
                                "name": format!("tool-{index}"),
                                "arguments": "{}",
                            },
                        })
                    })
                    .collect(),
            ),
        }
    }

    #[tokio::test]
    async fn single_and_multiple_tools_use_the_same_single_step_transaction() {
        for call_count in [1, 3] {
            let store = inserted_ready_run().await;
            let executor = TestSingleStepExecutor {
                outcome: tool_outcome(call_count),
                seen_triggers: Arc::new(Mutex::new(Vec::new())),
            };

            assert_eq!(
                consume_cloud_agent_single_step(&store, &executor, consume_input())
                    .await
                    .unwrap(),
                CloudAgentConsumeDisposition::Committed
            );
            let persisted = store.load_run("run-1").await.unwrap().unwrap();
            assert_eq!(persisted.status, CloudAgentRunStatus::WaitingToolResult);
            assert_eq!(persisted.pending_tool_calls.len(), call_count);
            let outbox = store.list_ready_outbox(10).await.unwrap();
            assert_eq!(outbox.len(), 1);
            assert_eq!(outbox[0].topic, "mcp_tool_call_command");
            assert_eq!(outbox[0].routing_key, "mcp.commands");
            assert_eq!(
                outbox[0].payload["calls"].as_array().unwrap().len(),
                call_count
            );
        }
    }

    #[tokio::test]
    async fn retry_keeps_exact_input_items_in_the_durable_event() {
        let store = inserted_ready_run().await;
        let executor = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "timeout".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
                disable_stream: false,
                downgrade_thinking_to: None,
            },
            seen_triggers: Arc::new(Mutex::new(Vec::new())),
        };

        consume_cloud_agent_single_step(&store, &executor, consume_input())
            .await
            .unwrap();
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].topic, "ai_runtime_retry");
        assert_eq!(
            outbox[0].payload["input_items"],
            serde_json::json!([{"type": "message"}])
        );
    }

    #[tokio::test]
    async fn tool_result_transport_retry_keeps_outputs_and_never_republishes_the_tool_batch() {
        let store = InMemoryCloudAgentRunStore::new();
        store.allocate_lane_seq("task:task-1").await.unwrap();
        let mut record = run();
        record.status = CloudAgentRunStatus::WaitingToolResult;
        record.phase = CloudAgentRunPhase::ToolBatch;
        record.ordering.step_seq = 2;
        record.iteration = 1;
        record.version = 1;
        record.pending_batch_id = Some("mcp_batch_run-1_1_1".to_string());
        record.pending_tool_calls = vec![serde_json::json!({
            "id": "call-write-1",
            "function": {
                "name": "code_maintainer_write_stage_edit_batch",
                "arguments": "{\"path\":\"src/App.tsx\"}"
            }
        })];
        record.response_input_items = vec![serde_json::json!({
            "type": "function_call",
            "call_id": "call-write-1",
            "name": "code_maintainer_write_stage_edit_batch",
            "arguments": "{\"path\":\"src/App.tsx\"}"
        })];
        store.insert_run(record).await.unwrap();

        let durable_retry_items = vec![
            serde_json::json!({
                "type": "function_call",
                "call_id": "call-write-1",
                "name": "code_maintainer_write_stage_edit_batch",
                "arguments": "{\"path\":\"src/App.tsx\"}"
            }),
            serde_json::json!({
                "type": "function_call_output",
                "call_id": "call-write-1",
                "output": "{\"written\":true}"
            }),
        ];
        let seen_triggers = Arc::new(Mutex::new(Vec::new()));
        let retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "stream response body failed: unexpected eof".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
                disable_stream: true,
                downgrade_thinking_to: None,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let retry = struct_with_retry_items(retry, durable_retry_items.clone());
        let tool_result_event_id = "mcp-result-batch-1";
        let input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: tool_result_event_id.to_string(),
            trigger: CloudAgentModelTrigger::ToolResults {
                event_id: tool_result_event_id.to_string(),
                batch_id: "mcp_batch_run-1_1_1".to_string(),
                source_step_seq: 1,
                items: vec![serde_json::json!({
                    "status": "completed",
                    "result": {"written": true}
                })],
            },
            expected_status: CloudAgentRunStatus::WaitingToolResult,
            expected_phase: CloudAgentRunPhase::ToolBatch,
            claim_token: "claim-after-tools".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };

        assert_eq!(
            consume_cloud_agent_single_step(&store, &retry, input)
                .await
                .unwrap(),
            CloudAgentConsumeDisposition::Committed
        );
        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.status, CloudAgentRunStatus::RetryScheduled);
        assert_eq!(persisted.response_input_items, durable_retry_items);
        assert_eq!(persisted.pending_tool_calls.len(), 1);
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].topic, "ai_runtime_retry");
        assert_eq!(
            outbox[0].payload["input_items"][1]["type"],
            "function_call_output"
        );
        assert!(!outbox
            .iter()
            .any(|intent| intent.topic == "mcp_tool_call_command"));

        let retry_event_id = outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(retry_event_id.as_str())
            .await
            .unwrap());
        let final_executor = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Final(AiRuntimeResult {
                content: "done without repeating the write".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: Some("response-after-retry".to_string()),
                response_output_items: Vec::new(),
                request_input_items: durable_retry_items,
            }),
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let retry_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: retry_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: retry_event_id,
                model_attempt: 2,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-retry-final".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        assert_eq!(
            consume_cloud_agent_single_step(&store, &final_executor, retry_input)
                .await
                .unwrap(),
            CloudAgentConsumeDisposition::Committed
        );
        assert_eq!(seen_triggers.lock().unwrap().len(), 2);
        assert_eq!(
            store.load_run("run-1").await.unwrap().unwrap().status,
            CloudAgentRunStatus::Succeeded
        );
        assert!(store
            .list_ready_outbox(10)
            .await
            .unwrap()
            .iter()
            .all(|intent| intent.topic != "mcp_tool_call_command"));
    }

    fn struct_with_retry_items(
        executor: TestSingleStepExecutor,
        retry_input_items: Vec<Value>,
    ) -> impl CloudAgentSingleStepExecutor {
        #[derive(Clone)]
        struct RetryInputExecutor {
            executor: TestSingleStepExecutor,
            retry_input_items: Vec<Value>,
        }

        #[async_trait]
        impl CloudAgentSingleStepExecutor for RetryInputExecutor {
            async fn execute_single_step(
                &self,
                run: &CloudAgentRunRecord,
                trigger: &CloudAgentModelTrigger,
            ) -> Result<CloudAgentSingleStepExecution, String> {
                let CloudAgentSingleStepExecution::Apply(output) =
                    self.executor.execute_single_step(run, trigger).await?
                else {
                    unreachable!();
                };
                Ok(CloudAgentSingleStepExecution::Apply(
                    output.with_retry_input_items(self.retry_input_items.clone()),
                ))
            }
        }

        RetryInputExecutor {
            executor,
            retry_input_items,
        }
    }

    #[tokio::test]
    async fn consecutive_retries_publish_distinct_events_and_can_finish_terminally() {
        let store = inserted_ready_run().await;
        let seen_triggers = Arc::new(Mutex::new(Vec::new()));
        let first_retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "connect failed".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
                disable_stream: false,
                downgrade_thinking_to: None,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        consume_cloud_agent_single_step(&store, &first_retry, consume_input())
            .await
            .unwrap();
        let first_outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(first_outbox.len(), 1);
        let first_event_id = first_outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(first_event_id.as_str())
            .await
            .unwrap());

        let second_retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "dns failed".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 3,
                backoff_ms: 0,
                disable_stream: false,
                downgrade_thinking_to: None,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let second_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: first_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: first_event_id,
                model_attempt: 2,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-second-retry".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        consume_cloud_agent_single_step(&store, &second_retry, second_input)
            .await
            .unwrap();
        let second_outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(second_outbox.len(), 1);
        assert_ne!(second_outbox[0].event_id, first_outbox[0].event_id);
        assert_eq!(second_outbox[0].payload["model_attempt"], 3);
        let second_event_id = second_outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(second_event_id.as_str())
            .await
            .unwrap());

        let exhausted = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Failed {
                error: "network retry exhausted".to_string(),
            },
            seen_triggers,
        };
        let exhausted_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: second_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: second_event_id,
                model_attempt: 3,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-exhausted-retry".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        consume_cloud_agent_single_step(&store, &exhausted, exhausted_input)
            .await
            .unwrap();

        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.status, CloudAgentRunStatus::Failed);
        assert_eq!(persisted.phase, CloudAgentRunPhase::Terminal);
        assert_eq!(
            persisted.terminal_outcome.unwrap()["error"],
            "network retry exhausted"
        );
    }

    #[tokio::test]
    async fn owner_step_error_is_committed_as_terminal_failure() {
        #[derive(Clone)]
        struct FailingExecutor;

        #[async_trait]
        impl CloudAgentSingleStepExecutor for FailingExecutor {
            async fn execute_single_step(
                &self,
                _run: &CloudAgentRunRecord,
                _trigger: &CloudAgentModelTrigger,
            ) -> Result<CloudAgentSingleStepExecution, String> {
                Err("required MCPs cannot be materialized".to_string())
            }
        }

        let store = inserted_ready_run().await;
        assert_eq!(
            consume_cloud_agent_single_step(&store, &FailingExecutor, consume_input())
                .await
                .unwrap(),
            CloudAgentConsumeDisposition::Committed
        );

        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.status, CloudAgentRunStatus::Failed);
        assert_eq!(persisted.phase, CloudAgentRunPhase::Terminal);
        assert_eq!(
            persisted
                .terminal_outcome
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(Value::as_str),
            Some("required MCPs cannot be materialized")
        );
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].topic, "owner_lifecycle_terminal");
    }

    #[tokio::test]
    async fn shared_run_factory_allocates_the_lane_and_start_event_atomically() {
        let store = CloudAgentStateStore::memory();
        let record = create_cloud_agent_run(
            &store,
            NewCloudAgentRun {
                ordering_lane_key: "conversation:session-1".to_string(),
                agent_run_id: "turn-1".to_string(),
                owner_service: "chatos".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: "turn-1".to_string(),
                owner_user_id: "user-1".to_string(),
                agent_key: "chatos_conversation_agent".to_string(),
                input: serde_json::json!({"content": "hello"}),
                model_config_ref: "model-1".to_string(),
                model_runtime_snapshot_ref: "turn-1:model".to_string(),
                agent_prompt_revision: "1".to_string(),
                agent_prompt_checksum: "checksum-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                mcp_runtime_session_ref: Some("session-1".to_string()),
                current_input_items_ref: "turn-1:initial".to_string(),
                max_iterations: 10,
                deadline_at: None,
                runtime_routing_key: "cloud_agent.chatos.runtime".to_string(),
                start_causation_id: "message-1".to_string(),
                start_payload: serde_json::json!({"conversation_id": "session-1"}),
            },
        )
        .await
        .unwrap();

        assert_eq!(record.ordering.lane_seq, 1);
        assert_eq!(record.status, CloudAgentRunStatus::ModelReady);
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].ordering, record.ordering);
        assert_eq!(outbox[0].payload["event_type"], "run_started");
        assert_eq!(outbox[0].payload["conversation_id"], "session-1");
    }

    #[tokio::test]
    async fn owner_input_update_commits_with_the_same_model_step() {
        let store = inserted_ready_run().await;
        let executor = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Final(AiRuntimeResult {
                content: "done".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: Some("response-final".to_string()),
                response_output_items: Vec::new(),
                request_input_items: Vec::new(),
            }),
            seen_triggers: Arc::new(Mutex::new(Vec::new())),
        };

        #[derive(Clone)]
        struct InputUpdatingExecutor(TestSingleStepExecutor);

        #[async_trait]
        impl CloudAgentSingleStepExecutor for InputUpdatingExecutor {
            async fn execute_single_step(
                &self,
                run: &CloudAgentRunRecord,
                trigger: &CloudAgentModelTrigger,
            ) -> Result<CloudAgentSingleStepExecution, String> {
                let CloudAgentSingleStepExecution::Apply(output) =
                    self.0.execute_single_step(run, trigger).await?
                else {
                    unreachable!();
                };
                Ok(CloudAgentSingleStepExecution::Apply(
                    output.with_next_input(serde_json::json!({"lifecycle_round": 2})),
                ))
            }
        }

        consume_cloud_agent_single_step(&store, &InputUpdatingExecutor(executor), consume_input())
            .await
            .unwrap();
        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.input, serde_json::json!({"lifecycle_round": 2}));
        assert_eq!(persisted.status, CloudAgentRunStatus::Succeeded);
    }

    #[tokio::test]
    async fn slow_model_step_renews_claim_and_blocks_duplicate_consumer() {
        let store = inserted_ready_run().await;
        let slow = SlowSingleStepExecutor {
            delay: std::time::Duration::from_millis(120),
        };
        let mut input = consume_input();
        input.claim_until = Utc::now() + chrono::Duration::milliseconds(45);

        let competing_store = store.clone();
        let competing = async move {
            tokio::time::sleep(std::time::Duration::from_millis(70)).await;
            competing_store
                .acquire_short_claim(&CloudAgentClaim {
                    ordering: ordering(),
                    expected_status: CloudAgentRunStatus::ModelReady,
                    expected_phase: CloudAgentRunPhase::Ready,
                    expected_version: 1,
                    claim_token: "duplicate-claim".to_string(),
                    claim_until: Utc::now() + chrono::Duration::seconds(30),
                })
                .await
                .unwrap()
        };

        let (consumed, competing_result) = tokio::join!(
            consume_cloud_agent_single_step(&store, &slow, input),
            competing,
        );

        assert_eq!(consumed.unwrap(), CloudAgentConsumeDisposition::Committed);
        assert_eq!(competing_result, CloudAgentClaimResult::Conflict);
        assert_eq!(
            store.load_run("run-1").await.unwrap().unwrap().status,
            CloudAgentRunStatus::Succeeded
        );
    }

    #[derive(Clone)]
    struct TestProfile {
        executions: Arc<Mutex<Vec<String>>>,
        finalizations: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Clone)]
    struct SlowSingleStepExecutor {
        delay: std::time::Duration,
    }

    #[async_trait]
    impl CloudAgentSingleStepExecutor for SlowSingleStepExecutor {
        async fn execute_single_step(
            &self,
            _run: &CloudAgentRunRecord,
            _trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            tokio::time::sleep(self.delay).await;
            Ok(CloudAgentSingleStepExecution::Apply(
                CloudAgentSingleStepOutput::new(AiSingleStepOutcome::Final(AiRuntimeResult {
                    content: "done".to_string(),
                    reasoning: None,
                    tool_calls: None,
                    finish_reason: Some("stop".to_string()),
                    usage: None,
                    response_id: Some("response-slow".to_string()),
                    response_output_items: Vec::new(),
                    request_input_items: Vec::new(),
                })),
            ))
        }
    }

    #[async_trait]
    impl CloudAgentProfile for TestProfile {
        async fn execute_single_step(
            &self,
            run: &CloudAgentRunRecord,
            _trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            self.executions.lock().unwrap().push(run.agent_key.clone());
            Ok(CloudAgentSingleStepExecution::Apply(
                CloudAgentSingleStepOutput::new(AiSingleStepOutcome::Final(AiRuntimeResult {
                    content: "done".to_string(),
                    reasoning: None,
                    tool_calls: None,
                    finish_reason: Some("stop".to_string()),
                    usage: None,
                    response_id: Some("response-final".to_string()),
                    response_output_items: Vec::new(),
                    request_input_items: Vec::new(),
                })),
            ))
        }

        async fn finalize_terminal(&self, run: &CloudAgentRunRecord) -> Result<(), String> {
            self.finalizations
                .lock()
                .unwrap()
                .push(run.agent_key.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn one_profile_registration_serves_multiple_agent_keys() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let finalizations = Arc::new(Mutex::new(Vec::new()));
        let registry =
            CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
                .register(
                    ["task_runner_plan_phase", "task_runner_run_phase"],
                    TestProfile {
                        executions: Arc::clone(&executions),
                        finalizations: Arc::clone(&finalizations),
                    },
                )
                .unwrap();
        let trigger = CloudAgentModelTrigger::RunStarted {
            event_id: "start-1".to_string(),
            payload: Value::Null,
        };

        for key in ["task_runner_plan_phase", "task_runner_run_phase"] {
            let mut record = run();
            record.agent_key = key.to_string();
            registry
                .execute_single_step(&record, &trigger)
                .await
                .unwrap();
        }

        assert_eq!(
            executions.lock().unwrap().as_slice(),
            ["task_runner_plan_phase", "task_runner_run_phase"]
        );
        assert!(finalizations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn registry_rejects_unknown_agent_keys_and_wrong_owners() {
        let registry =
            CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
                .register(
                    ["task_runner_run_phase"],
                    TestProfile {
                        executions: Arc::new(Mutex::new(Vec::new())),
                        finalizations: Arc::new(Mutex::new(Vec::new())),
                    },
                )
                .unwrap();
        let trigger = CloudAgentModelTrigger::RunStarted {
            event_id: "start-1".to_string(),
            payload: Value::Null,
        };

        let mut unknown = run();
        unknown.agent_key = "unknown_agent".to_string();
        assert!(registry
            .execute_single_step(&unknown, &trigger)
            .await
            .unwrap_err()
            .contains("not registered"));

        let mut wrong_owner = run();
        wrong_owner.owner_service = "chatos".to_string();
        assert!(registry
            .execute_single_step(&wrong_owner, &trigger)
            .await
            .unwrap_err()
            .contains("owner mismatch"));
    }
}
