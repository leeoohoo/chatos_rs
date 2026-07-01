// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::info;

use super::{AiTransport, AGENT_RUNTIME_LOG_PREFIX};

pub(super) fn log_request_fingerprint(
    purpose: &str,
    session_id: Option<&str>,
    base_url: &str,
    payload: &Value,
    transport: AiTransport,
) {
    let input = payload
        .get("input")
        .cloned()
        .or_else(|| payload.get("messages").cloned())
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let tools = payload
        .get("tools")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let input_item_count = input.as_array().map(|items| items.len()).unwrap_or(0);
    let tools_count = tools.as_array().map(|items| items.len()).unwrap_or(0);

    let input_hash = sha256_json_hex(&input);
    let tools_hash = sha256_json_hex(&tools);
    let prefix_hash = compute_prefix_hash(&input, 8);

    info!(
        "{} request fingerprint: purpose={}, transport={}, session={}, baseURL={}, input_items={}, tools={}, prompt_cache_key={}, prompt_cache_retention={}, input_hash={}, input_prefix_hash={}, tools_hash={}",
        AGENT_RUNTIME_LOG_PREFIX,
        purpose,
        transport.log_label(),
        session_id.unwrap_or("n/a"),
        base_url,
        input_item_count,
        tools_count,
        payload
            .get("prompt_cache_key")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        payload
            .get("prompt_cache_retention")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        input_hash,
        prefix_hash,
        tools_hash,
    );
}

fn compute_prefix_hash(input: &Value, max_items: usize) -> String {
    let prefix = match input {
        Value::Array(items) => Value::Array(items.iter().take(max_items).cloned().collect()),
        other => other.clone(),
    };
    sha256_json_hex(&prefix)
}

fn sha256_json_hex(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.to_string().as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}
