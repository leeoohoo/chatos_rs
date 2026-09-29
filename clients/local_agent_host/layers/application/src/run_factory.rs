// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{CreateRunCommand, LocalAgentRunRecord, LocalAgentRunStatus};

pub(super) fn create_run_record(command: CreateRunCommand, now: i64) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: command.run_id,
        owner_user_id: command.owner_user_id,
        owner_entity_type: command.owner_entity_type,
        owner_entity_id: command.owner_entity_id,
        profile_key: command.profile_key,
        model_config_ref: command.model_config_ref,
        model_config_revision: command.model_config_revision,
        capability_policy_revision: command.capability_policy_revision,
        input: command.input,
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: command.max_iterations,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: serde_json::Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    }
}
