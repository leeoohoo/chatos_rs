// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{LocalAgentArtifact, LocalAgentRunStatus, LocalAgentToolBatch};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct IdempotentCommand {
    pub command_id: String,
    pub request_fingerprint: String,
    /// Durable IPC commands keep a replay receipt; trusted in-process schedulers can opt out.
    pub persist_receipt: bool,
}

#[derive(Debug, Clone)]
pub struct LocalAgentArtifactWrite {
    pub artifact: LocalAgentArtifact,
    pub idempotency_key: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunTransition {
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub next_status: LocalAgentRunStatus,
    pub next_model_attempt: u32,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub tool_batch: Option<LocalAgentToolBatch>,
    pub checkpoint: Option<Value>,
    pub clear_continuation_input: bool,
    pub terminal_outcome: Option<Value>,
    pub event_id: String,
    pub event_type: String,
    pub event_payload: Value,
    pub occurred_at_unix_ms: i64,
}
