// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Shared state reducer and persistence boundary used inside each owner
//! service's AI Runtime consumer. This crate is not another consumer or
//! orchestrator: it performs one claimed transition and returns outbox intents.

use async_trait::async_trait;
use chatos_ai_runtime::{AiSingleStepOutcome, AiSingleStepRequest};
use chatos_cloud_agent_protocol::{
    CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunRecord, CloudAgentRunStatus,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

mod mongo_store;
mod state_store;

pub use mongo_store::{CloudAgentLaneRecord, MongoCloudAgentRunStore};
pub use state_store::{CloudAgentStateStore, InMemoryCloudAgentRunStore};

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
    pub pending_batch_id: Option<String>,
    #[serde(default)]
    pub pending_tool_calls: Vec<Value>,
    #[serde(default)]
    pub pending_tool_results: Vec<Value>,
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

    /// Must persist state and all outbox intents in the same database transaction.
    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String>;

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String>;
}

#[async_trait]
pub trait CloudAgentModelResolver: Send + Sync {
    async fn resolve_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: CloudAgentModelTrigger,
    ) -> Result<AiSingleStepRequest, String>;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudAgentConsumeDisposition {
    Committed,
    Duplicate,
    OutOfOrder,
    Conflict,
    Terminal,
}

/// Handles one AI Runtime delivery. The supplied executor must itself perform
/// exactly one model request; `AiRuntime::execute_once` satisfies that contract.
pub async fn consume_once<S, R, E, Fut>(
    store: &S,
    resolver: &R,
    input: CloudAgentConsumeInput,
    execute: E,
) -> Result<CloudAgentConsumeDisposition, String>
where
    S: CloudAgentRunStore,
    R: CloudAgentModelResolver,
    E: FnOnce(AiSingleStepRequest) -> Fut,
    Fut: Future<Output = Result<AiSingleStepOutcome, String>>,
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
    let claim = CloudAgentClaim {
        ordering: run.ordering.clone(),
        expected_status: input.expected_status,
        expected_phase: input.expected_phase,
        expected_version: run.version,
        claim_token: input.claim_token,
        claim_until: input.claim_until,
    };
    match store.acquire_short_claim(&claim).await? {
        CloudAgentClaimResult::Acquired => {}
        CloudAgentClaimResult::Duplicate => return Ok(CloudAgentConsumeDisposition::Duplicate),
        CloudAgentClaimResult::OutOfOrder => return Ok(CloudAgentConsumeDisposition::OutOfOrder),
        CloudAgentClaimResult::Conflict => return Ok(CloudAgentConsumeDisposition::Conflict),
        CloudAgentClaimResult::Terminal => return Ok(CloudAgentConsumeDisposition::Terminal),
    }
    let result = async {
        let request = resolver.resolve_single_step(&run, input.trigger).await?;
        let outcome = execute(request).await?;
        let transition = reduce_single_step(
            &run,
            claim.clone(),
            input.event_id.as_str(),
            input.output_routing_key.as_str(),
            outcome,
        )?;
        store.commit_transition(transition).await
    }
    .await;
    match result {
        Ok(true) => Ok(CloudAgentConsumeDisposition::Committed),
        Ok(false) => {
            store.release_short_claim(&claim).await?;
            Ok(CloudAgentConsumeDisposition::Conflict)
        }
        Err(error) => {
            store.release_short_claim(&claim).await?;
            Err(error)
        }
    }
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
        items: Vec<Value>,
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
                next_status: CloudAgentRunStatus::WaitingToolResult,
                next_phase: CloudAgentRunPhase::ToolBatch,
                next_step_seq,
                next_iteration,
                next_retry_count: 0,
                previous_response_id: response.response_id.clone(),
                continuation_mode: Some("mcp_tool_results".to_string()),
                current_input_items_ref: format!(
                    "cloud_agent:{}:{}:{}:tool_results",
                    claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
                ),
                pending_batch_id: Some(batch_id.clone()),
                pending_tool_calls: tool_calls.as_array().cloned().unwrap_or_default(),
                pending_tool_results: Vec::new(),
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
            next_status: CloudAgentRunStatus::ModelReady,
            next_phase: CloudAgentRunPhase::Ready,
            next_step_seq,
            next_iteration,
            next_retry_count: 0,
            previous_response_id: response.response_id.clone(),
            continuation_mode: Some(reason.clone()),
            current_input_items_ref: format!(
                "cloud_agent:{}:{}:{}:continuation",
                claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
            ),
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
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
            next_status: CloudAgentRunStatus::RetryScheduled,
            next_phase: CloudAgentRunPhase::RetryDelay,
            next_step_seq: claim.ordering.step_seq,
            next_iteration: run.iteration,
            next_retry_count: u32::try_from(next_model_attempt.saturating_sub(1))
                .unwrap_or(u32::MAX),
            previous_response_id: run.previous_response_id.clone(),
            continuation_mode: run.continuation_mode.clone(),
            current_input_items_ref: run.current_input_items_ref.clone(),
            pending_batch_id: run.pending_batch_id.clone(),
            pending_tool_calls: run.pending_tool_calls.clone(),
            pending_tool_results: run.pending_tool_results.clone(),
            terminal_outcome: None,
            outbox: vec![outbox_intent(
                &claim.ordering,
                causation_id,
                "ai_runtime_retry",
                result_routing_key,
                serde_json::json!({
                    "error": error,
                    "retry_kind": retry_kind,
                    "model_attempt": next_model_attempt,
                    "disable_stream": disable_stream,
                    "downgrade_thinking_to": downgrade_thinking_to,
                }),
                Utc::now()
                    + chrono::Duration::milliseconds(i64::try_from(backoff_ms).unwrap_or(i64::MAX)),
            )],
        },
        AiSingleStepOutcome::Final(result) => CloudAgentAtomicTransition {
            claim,
            next_status: CloudAgentRunStatus::Succeeded,
            next_phase: CloudAgentRunPhase::Terminal,
            next_step_seq,
            next_iteration,
            next_retry_count: 0,
            previous_response_id: result.response_id.clone(),
            continuation_mode: None,
            current_input_items_ref: run.current_input_items_ref.clone(),
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            terminal_outcome: Some(serde_json::json!({
                "content": result.content,
                "reasoning": result.reasoning,
                "finish_reason": result.finish_reason,
                "usage": result.usage,
                "response_id": result.response_id,
            })),
            outbox: Vec::new(),
        },
        AiSingleStepOutcome::Failed { error } => terminal_transition(
            claim,
            CloudAgentRunStatus::Failed,
            next_step_seq,
            next_iteration,
            serde_json::json!({"error": error}),
        ),
        AiSingleStepOutcome::Cancelled => terminal_transition(
            claim,
            CloudAgentRunStatus::Cancelled,
            next_step_seq,
            next_iteration,
            serde_json::json!({"cancelled": true}),
        ),
    };
    transition.validate()?;
    Ok(transition)
}

fn terminal_transition(
    claim: CloudAgentClaim,
    status: CloudAgentRunStatus,
    next_step_seq: u64,
    next_iteration: u32,
    terminal_outcome: Value,
) -> CloudAgentAtomicTransition {
    let current_input_items_ref = format!(
        "cloud_agent:{}:{}:{}:terminal",
        claim.ordering.agent_run_id, claim.ordering.generation, claim.ordering.step_seq
    );
    CloudAgentAtomicTransition {
        claim,
        next_status: status,
        next_phase: CloudAgentRunPhase::Terminal,
        next_step_seq,
        next_iteration,
        next_retry_count: 0,
        previous_response_id: None,
        continuation_mode: None,
        current_input_items_ref,
        pending_batch_id: None,
        pending_tool_calls: Vec::new(),
        pending_tool_results: Vec::new(),
        terminal_outcome: Some(terminal_outcome),
        outbox: Vec::new(),
    }
}

fn stable_batch_id(ordering: &CloudAgentOrdering) -> String {
    format!(
        "mcp_batch_{}_{}_{}",
        ordering.agent_run_id, ordering.generation, ordering.step_seq
    )
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
    }
}
