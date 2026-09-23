// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::CloudAgentClaim;
use crate::input_history::{
    accumulate_usage, append_continuation_items, append_response_output_items,
};
use crate::run_contract::{CloudAgentAtomicTransition, CloudAgentOutboxIntent};
use chatos_ai_runtime::AiSingleStepOutcome;
use chatos_cloud_agent_protocol::{
    CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunRecord, CloudAgentRunStatus,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
                usage_accumulator: accumulate_usage(
                    &run.usage_accumulator,
                    response.usage.as_ref(),
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
            usage_accumulator: accumulate_usage(&run.usage_accumulator, response.usage.as_ref()),
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
        } => CloudAgentAtomicTransition {
            claim: claim.clone(),
            next_input: run.input.clone(),
            next_status: CloudAgentRunStatus::RetryScheduled,
            next_phase: CloudAgentRunPhase::RetryDelay,
            next_step_seq: claim.ordering.step_seq,
            next_iteration: run.iteration,
            next_retry_count: u32::try_from(next_model_attempt.saturating_sub(1))
                .unwrap_or(u32::MAX),
            previous_response_id: None,
            continuation_mode: run.continuation_mode.clone(),
            current_input_items_ref: run.current_input_items_ref.clone(),
            mcp_runtime_session_ref: run.mcp_runtime_session_ref.clone(),
            pending_batch_id: run.pending_batch_id.clone(),
            pending_tool_calls: run.pending_tool_calls.clone(),
            pending_tool_results: run.pending_tool_results.clone(),
            response_input_items: run.response_input_items.clone(),
            usage_accumulator: run.usage_accumulator.clone(),
            terminal_outcome: None,
            outbox: vec![retry_outbox_intent(
                &claim.ordering,
                causation_id,
                result_routing_key,
                serde_json::json!({
                    "error": error,
                    "retry_kind": retry_kind,
                    "model_attempt": next_model_attempt,
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
            usage_accumulator: accumulate_usage(&run.usage_accumulator, result.usage.as_ref()),
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
            run.usage_accumulator.clone(),
            CloudAgentRunStatus::Failed,
            next_step_seq,
            next_iteration,
            serde_json::json!({"error": error}),
        ),
        AiSingleStepOutcome::Cancelled => terminal_transition(
            claim,
            run.input.clone(),
            run.mcp_runtime_session_ref.clone(),
            run.usage_accumulator.clone(),
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
    usage_accumulator: Value,
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
        usage_accumulator,
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
