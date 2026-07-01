// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Instant;

use tracing::{info, warn};

use super::AiClient;

pub(super) fn build_user_prompt(title: Option<&str>, input: &str) -> String {
    match title.map(str::trim).filter(|value| !value.is_empty()) {
        Some(title) => format!("Thread title: {title}\n\nConversation increment:\n{input}"),
        None => format!("Conversation increment:\n{input}"),
    }
}

pub(super) fn request_kind(supports_responses: bool) -> &'static str {
    if supports_responses {
        "responses"
    } else {
        "chat_completions"
    }
}

pub(super) fn validate_summary_text(
    client: &AiClient,
    request_kind: &str,
    started_at: Instant,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() {
        warn!(
            "[MEMORY-ENGINE-AI] response-empty-content model={} base_url={} request_kind={} elapsed_ms={}",
            client.model,
            client.base_url,
            request_kind,
            started_at.elapsed().as_millis()
        );
        return Err("ai empty content".to_string());
    }
    info!(
        "[MEMORY-ENGINE-AI] request-done model={} base_url={} request_kind={} elapsed_ms={} output_chars={}",
        client.model,
        client.base_url,
        request_kind,
        started_at.elapsed().as_millis(),
        text.chars().count()
    );
    Ok(text)
}
