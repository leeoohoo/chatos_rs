// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::input_history::merge_terminal_outcome_overlay;
use crate::reducer::{materialize_mcp_command, reduce_single_step, CloudAgentModelTrigger};
use crate::run_contract::{CloudAgentClaimResult, CloudAgentConsumeInput, CloudAgentRunStore};
use crate::{CloudAgentClaim, CloudAgentStateStore};
use async_trait::async_trait;
use chatos_ai_runtime::AiSingleStepOutcome;
use chatos_cloud_agent_protocol::{CloudAgentRunRecord, CloudAgentRunStatus};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

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
#[allow(clippy::large_enum_variant)]
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
    pub(crate) owner_service: &'static str,
    pub(crate) store: CloudAgentStateStore,
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

    pub(crate) fn profile_for(
        &self,
        run: &CloudAgentRunRecord,
    ) -> Result<Arc<dyn CloudAgentProfile>, String> {
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
        let execution_result = match run.deadline_at {
            Some(deadline_at) => match deadline_at.signed_duration_since(Utc::now()).to_std() {
                Ok(remaining) if !remaining.is_zero() => {
                    match tokio::time::timeout(
                        remaining,
                        executor.execute_single_step(&run, &input.trigger),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err("Cloud Agent execution deadline exceeded".to_string()),
                    }
                }
                _ => Err("Cloud Agent execution deadline exceeded".to_string()),
            },
            None => executor.execute_single_step(&run, &input.trigger).await,
        };
        let execution = match execution_result {
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
