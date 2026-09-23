// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Owner-local state reducer: performs one claimed transition and returns outbox intents.

use chatos_cloud_agent_protocol::{CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod execution;
mod input_history;
mod input_projection;
mod rabbitmq_driver;
mod reducer;
mod run_contract;
mod state_repository;
mod state_store;

pub use execution::{
    consume_cloud_agent_single_step, CloudAgentConsumeDisposition, CloudAgentProfile,
    CloudAgentProfileRegistry, CloudAgentSingleStepExecution, CloudAgentSingleStepExecutor,
    CloudAgentSingleStepOutput,
};
pub use input_projection::{
    cloud_agent_mcp_result_callback_payload, cloud_agent_mcp_result_input_items,
    cloud_agent_trigger_execution_identity, cloud_agent_trigger_input_items,
};
pub use rabbitmq_driver::{
    publish_cloud_agent_intent, spawn_cloud_agent_consumer, spawn_cloud_agent_outbox_reconciler,
    CloudAgentQueueOwner, CloudAgentRabbitMqTopology, CloudAgentServiceAdapter,
    CloudAgentServiceRuntime,
};
pub use reducer::{materialize_mcp_command, reduce_single_step, CloudAgentModelTrigger};
pub use run_contract::{
    create_cloud_agent_run, CloudAgentAtomicTransition, CloudAgentClaimResult,
    CloudAgentConsumeInput, CloudAgentOutboxIntent, CloudAgentRunStore, NewCloudAgentRun,
};
pub use state_repository::{
    apply_cloud_agent_transition, bounded_cloud_agent_outbox_error, classify_cloud_agent_claim,
    cloud_agent_outbox_failure, validate_initial_cloud_agent_state, CloudAgentOutboxPublishFailure,
    CloudAgentPendingOutboxIntent, CloudAgentStateRepository,
};
pub use state_store::{CloudAgentStateStore, InMemoryCloudAgentRunStore};

#[cfg(test)]
use input_history::append_response_output_items;

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

#[cfg(test)]
include!("lib_inline_tests.rs");
