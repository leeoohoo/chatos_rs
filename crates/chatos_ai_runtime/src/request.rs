// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{Duration, Instant};

use serde_json::Value;
use tracing::{info, warn};

use crate::request_payload::build_responses_request_payload;
use http::{
    log_preview, read_error_response_text_limited, retry_after_delay_ms, send_json_request,
    serialize_request_payload, validate_request_payload_size,
};
use streaming::parse_stream_response;

const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 15;
// This is an inactivity timeout: reqwest resets it whenever another response
// chunk arrives. Long-running streamed generations remain valid, while a
// provider that returns no headers or no further bytes is released promptly.
const DEFAULT_READ_TIMEOUT_SECS: u64 = 180;
const MAX_CONFIGURED_TIMEOUT_SECS: u64 = 7_200;
const AI_CONNECT_TIMEOUT_SECS_ENV: &str = "CHATOS_AI_CONNECT_TIMEOUT_SECS";
const AI_READ_TIMEOUT_SECS_ENV: &str = "CHATOS_AI_READ_TIMEOUT_SECS";

mod http;
mod streaming;
#[cfg(test)]
mod tests;
mod types;

pub use types::{AiRequestOptions, AiResponse, StreamCallbacks};

#[cfg(test)]
use streaming::emit_finalized_stream_callbacks;

#[derive(Clone)]
pub struct AiRequestHandler {
    client: reqwest::Client,
    read_timeout: Option<Duration>,
}

impl AiRequestHandler {
    pub fn new() -> Self {
        let read_timeout = configured_timeout(AI_READ_TIMEOUT_SECS_ENV, DEFAULT_READ_TIMEOUT_SECS);
        Self::new_with_read_timeout(read_timeout)
    }

    pub fn new_with_read_timeout(read_timeout: Duration) -> Self {
        let connect_timeout =
            configured_timeout(AI_CONNECT_TIMEOUT_SECS_ENV, DEFAULT_CONNECT_TIMEOUT_SECS);
        let client = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .read_timeout(read_timeout)
            .build()
            .unwrap_or_else(|err| {
                warn!("failed to build AI http client with timeouts: {err}");
                reqwest::Client::new()
            });
        Self {
            client,
            read_timeout: Some(read_timeout),
        }
    }

    pub fn from_client(client: reqwest::Client) -> Self {
        Self {
            client,
            read_timeout: None,
        }
    }

    pub fn read_timeout_seconds(&self) -> Option<u64> {
        self.read_timeout.map(|value| value.as_secs())
    }

    pub async fn handle_request(
        &self,
        base_url: &str,
        api_key: &str,
        input: Value,
        model: String,
        instructions: Option<String>,
        tools: Option<Vec<Value>>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
    ) -> Result<AiResponse, String> {
        self.handle_request_with_options(
            base_url,
            api_key,
            input,
            model,
            instructions,
            tools,
            temperature,
            max_output_tokens,
            provider,
            thinking_level,
            AiRequestOptions {
                callbacks,
                ..Default::default()
            },
        )
        .await
    }

    pub async fn handle_request_with_options(
        &self,
        base_url: &str,
        api_key: &str,
        input: Value,
        model: String,
        instructions: Option<String>,
        tools: Option<Vec<Value>>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        provider: Option<String>,
        thinking_level: Option<String>,
        mut options: AiRequestOptions,
    ) -> Result<AiResponse, String> {
        let provider = effective_provider_for_request(base_url, provider);
        options.previous_response_id = None;
        let first_payload = build_request_payload(
            input.clone(),
            model.clone(),
            instructions.clone(),
            tools.clone(),
            temperature,
            max_output_tokens,
            provider.clone(),
            thinking_level.clone(),
            &options,
        );
        self.send_payload(base_url, api_key, first_payload.clone(), options)
            .await
    }

    async fn send_payload(
        &self,
        base_url: &str,
        api_key: &str,
        payload: Value,
        options: AiRequestOptions,
    ) -> Result<AiResponse, String> {
        let callbacks = options.callbacks.clone();
        let payload_body = serialize_request_payload(&payload)?;
        validate_request_payload_size(payload_body.len(), options.request_body_limit_bytes)?;
        if let Some(cb) = options.on_before_send_model_request {
            cb(payload.clone());
        }
        let url = format!("{}/responses", base_url.trim_end_matches('/'));
        info!(
            transport = "responses",
            url = url.as_str(),
            payload_bytes = payload_body.len(),
            stream = options.stream,
            "dispatching ai provider request"
        );
        let request_started_at = Instant::now();
        let response = send_json_request(
            &self.client,
            url.as_str(),
            api_key,
            payload_body,
            options.abort_token.clone(),
            options.force_identity_encoding,
        )
        .await?;
        let response_headers_ms = request_started_at.elapsed().as_millis();
        info!(
            transport = "responses",
            url = url.as_str(),
            response_headers_ms,
            "ai provider response headers received"
        );
        if !response.status().is_success() {
            let status = response.status();
            let retry_after_ms = retry_after_delay_ms(response.headers());
            let provider_request_id = response
                .headers()
                .get("x-request-id")
                .or_else(|| response.headers().get("openai-request-id"))
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unavailable")
                .to_string();
            let body = read_error_response_text_limited(response).await;
            let body_preview = log_preview(body.as_str());
            warn!(
                transport = "responses",
                url = url.as_str(),
                status = status.as_u16(),
                retry_after_ms,
                provider_request_id = provider_request_id.as_str(),
                response_body = body_preview.as_str(),
                "ai provider request failed"
            );
            let retry_hint = retry_after_ms
                .map(|value| format!(" [retry_after_ms={value}]"))
                .unwrap_or_default();
            return Err(format!(
                "status {status}{retry_hint} [provider_request_id={provider_request_id}]: {body}"
            ));
        }

        let parsed =
            parse_stream_response(response, callbacks, options.abort_token, self.read_timeout)
                .await;
        match &parsed {
            Ok(ai_response) => {
                info!(
                    transport = "responses",
                    url = url.as_str(),
                    response_id = ai_response.response_id.as_deref().unwrap_or(""),
                    provider_request_id = ai_response.provider_request_id.as_deref().unwrap_or(""),
                    finish_reason = ai_response.finish_reason.as_deref().unwrap_or(""),
                    response_status = ai_response.response_status.as_deref().unwrap_or(""),
                    terminal_event_type = ai_response.terminal_event_type.as_deref().unwrap_or(""),
                    terminal_event_seen = ai_response.terminal_event_seen,
                    parsed_sse_event_count = ai_response.parsed_stream_event_count,
                    malformed_sse_event_count = ai_response.malformed_stream_event_count,
                    content_bytes = ai_response.content.len(),
                    reasoning_bytes = ai_response.reasoning.as_deref().map(str::len).unwrap_or(0),
                    tool_call_count = ai_response_tool_call_count(ai_response),
                    has_provider_error = ai_response.provider_error.is_some(),
                    has_usage = ai_response.usage.is_some(),
                    ai_provider_request_ms = request_started_at.elapsed().as_millis(),
                    stream = options.stream,
                    "received ai provider response"
                );
            }
            Err(err) => {
                warn!(
                    transport = "responses",
                    url = url.as_str(),
                    error = err.as_str(),
                    stream = options.stream,
                    "failed to parse ai provider response"
                );
            }
        }
        parsed
    }
}

impl Default for AiRequestHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn build_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    tools: Option<Vec<Value>>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    options: &AiRequestOptions,
) -> Value {
    let mut payload = build_responses_request_payload(
        input,
        model,
        instructions,
        tools,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        options,
    );
    if let Some(threshold) = options.responses_compaction_threshold {
        payload["context_management"] = serde_json::json!([{
            "type": "compaction",
            "compact_threshold": threshold,
        }]);
    }
    payload
}

fn configured_timeout(env_key: &str, default_seconds: u64) -> Duration {
    let configured = std::env::var(env_key).ok();
    let seconds = parse_timeout_seconds(configured.as_deref(), default_seconds);
    if configured
        .as_deref()
        .is_some_and(|value| value.trim().parse::<u64>().ok() != Some(seconds))
    {
        warn!(
            env_key,
            configured_value = configured.as_deref().unwrap_or_default(),
            default_seconds,
            "invalid AI timeout configuration; using default"
        );
    }
    Duration::from_secs(seconds)
}

fn parse_timeout_seconds(value: Option<&str>, default_seconds: u64) -> u64 {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=MAX_CONFIGURED_TIMEOUT_SECS).contains(value))
        .unwrap_or(default_seconds)
}

fn effective_provider_for_request(base_url: &str, provider: Option<String>) -> Option<String> {
    let _ = base_url;
    provider
}

fn ai_response_tool_call_count(response: &AiResponse) -> usize {
    response
        .tool_calls
        .as_ref()
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}
