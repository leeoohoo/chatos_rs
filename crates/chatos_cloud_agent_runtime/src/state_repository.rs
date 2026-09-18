// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;

use crate::{
    CloudAgentAtomicTransition, CloudAgentClaim, CloudAgentClaimResult, CloudAgentOutboxIntent,
    CloudAgentRunStore,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentOutboxPublishFailure {
    pub publish_attempts: u32,
    pub dead_lettered: bool,
    pub available_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CloudAgentPendingOutboxIntent {
    pub intent: CloudAgentOutboxIntent,
    pub publish_attempts: u32,
}

pub fn bounded_cloud_agent_outbox_error(error: &str) -> String {
    error.chars().take(2_000).collect()
}

pub fn cloud_agent_outbox_failure(
    row: Option<(i32, String)>,
    available_at: chrono::DateTime<chrono::Utc>,
) -> Option<CloudAgentOutboxPublishFailure> {
    row.map(|(attempts, status)| CloudAgentOutboxPublishFailure {
        publish_attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
        dead_lettered: status == "dead_lettered",
        available_at,
    })
}

#[async_trait]
pub trait CloudAgentStateRepository: CloudAgentRunStore {
    async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String>;
    async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String>;
    async fn advance_lane_after_terminal(
        &self,
        ordering_lane_key: &str,
        completed_lane_seq: u64,
    ) -> Result<Option<u64>, String>;
    async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String>;
    async fn mark_claimed_outbox_published(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool, String>;
    async fn mark_claimed_outbox_publish_failed(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String>;
}

pub fn validate_initial_cloud_agent_state(
    record: &CloudAgentRunRecord,
    outbox: &[CloudAgentOutboxIntent],
) -> Result<(), String> {
    record.validate()?;
    for intent in outbox {
        intent.validate()?;
        if intent.ordering != record.ordering {
            return Err("initial outbox ordering does not match Cloud Agent run".to_string());
        }
    }
    Ok(())
}

pub fn classify_cloud_agent_claim(
    run: &CloudAgentRunRecord,
    claim: &CloudAgentClaim,
    current_token: Option<&str>,
    current_until: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> CloudAgentClaimResult {
    if run.status.is_terminal() {
        CloudAgentClaimResult::Terminal
    } else if run.ordering.generation > claim.ordering.generation
        || run.ordering.step_seq > claim.ordering.step_seq
        || run.version > claim.expected_version
    {
        CloudAgentClaimResult::Duplicate
    } else if run.ordering != claim.ordering
        || run.status != claim.expected_status
        || run.phase != claim.expected_phase
        || run.version != claim.expected_version
        || (current_token.is_some_and(|value| value != claim.claim_token)
            && current_until.is_some_and(|value| value > now))
    {
        CloudAgentClaimResult::Conflict
    } else {
        CloudAgentClaimResult::Acquired
    }
}

pub fn apply_cloud_agent_transition(
    run: &mut CloudAgentRunRecord,
    transition: CloudAgentAtomicTransition,
    current_token: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<Vec<CloudAgentOutboxIntent>>, String> {
    let claim = &transition.claim;
    if current_token != Some(claim.claim_token.as_str())
        || run.ordering != claim.ordering
        || run.status != claim.expected_status
        || run.phase != claim.expected_phase
        || run.version != claim.expected_version
    {
        return Ok(None);
    }
    run.input = transition.next_input;
    run.status = transition.next_status;
    run.phase = transition.next_phase;
    run.ordering.step_seq = transition.next_step_seq;
    run.iteration = transition.next_iteration;
    run.retry_count = transition.next_retry_count;
    run.previous_response_id = transition.previous_response_id;
    run.continuation_mode = transition.continuation_mode;
    run.current_input_items_ref = transition.current_input_items_ref;
    run.mcp_runtime_session_ref = transition.mcp_runtime_session_ref;
    run.pending_batch_id = transition.pending_batch_id;
    run.pending_tool_calls = transition.pending_tool_calls;
    run.pending_tool_results = transition.pending_tool_results;
    run.response_input_items = transition.response_input_items;
    run.usage_accumulator = transition.usage_accumulator;
    run.terminal_outcome = transition.terminal_outcome;
    run.version = run
        .version
        .checked_add(1)
        .ok_or_else(|| "Cloud Agent version overflow".to_string())?;
    run.updated_at = now;
    Ok(Some(transition.outbox))
}
