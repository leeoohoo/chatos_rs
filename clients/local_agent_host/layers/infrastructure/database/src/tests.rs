// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use sqlx::Connection;
use std::time::Duration;
use uuid::Uuid;

fn run(now: i64) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: "run-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "conversation-1".to_string(),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: serde_json::json!({"message": "hello"}),
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: 8,
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

fn owned_run(run_id: &str, owner_user_id: &str, now: i64) -> LocalAgentRunRecord {
    let mut record = run(now);
    record.run_id = run_id.to_string();
    record.owner_user_id = owner_user_id.to_string();
    record.owner_entity_id = format!("conversation-{owner_user_id}");
    record
}

fn command(id: &str, fingerprint: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: fingerprint.to_string(),
        persist_receipt: true,
    }
}

mod lease_tests;
mod query_tests;
mod run_lifecycle_tests;
