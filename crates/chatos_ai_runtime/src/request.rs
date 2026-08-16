// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::model_config::normalize_provider;
#[cfg(test)]
use crate::request_payload::response_items_to_chat_messages;
use crate::request_payload::{
    build_chat_completions_request_payload, build_responses_request_payload,
};
use crate::request_retry::should_retry_without_prompt_cache_options;
use http::{
    log_preview, read_error_response_text_limited, retry_after_delay_ms, send_json_request,
    serialize_request_payload, validate_request_payload_size,
};
use streaming::parse_stream_response;

const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 15;
const DEFAULT_READ_TIMEOUT_SECS: u64 = 300;
const MAX_CONFIGURED_TIMEOUT_SECS: u64 = 3_600;
const AI_CONNECT_TIMEOUT_SECS_ENV: &str = "CHATOS_AI_CONNECT_TIMEOUT_SECS";
const AI_READ_TIMEOUT_SECS_ENV: &str = "CHATOS_AI_READ_TIMEOUT_SECS";

mod http;
mod streaming;
#[cfg(test)]
mod tests;
mod types;

pub use types::{AiRequestOptions, AiResponse, AiTransport, StreamCallbacks};

#[cfg(test)]
use streaming::emit_finalized_stream_callbacks;

#[derive(Clone)]
pub struct AiRequestHandler {
    client: reqwest::Client,
    read_timeout: Option<Duration>,
}

impl AiRequestHandler {
    pub fn new() -> Self {
        let connect_timeout =
            configured_timeout(AI_CONNECT_TIMEOUT_SECS_ENV, DEFAULT_CONNECT_TIMEOUT_SECS);
        let read_timeout = configured_timeout(AI_READ_TIMEOUT_SECS_ENV, DEFAULT_READ_TIMEOUT_SECS);
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

    #[allow(clippy::too_many_arguments)]
    pub async fn handle_request(
        &self,
        base_url: &str,
        api_key: &str,
        input: Value,
        supports_responses: bool,
        model: String,
        instructions: Option<String>,
        tools: Option<Vec<Value>>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    ) -> Result<AiResponse, String> {
        self.handle_request_with_options(
            base_url,
            api_key,
            input,
            supports_responses,
            model,
            instructions,
            tools,
            temperature,
            max_output_tokens,
            callbacks,
            provider,
            thinking_level,
            on_before_send_model_request,
            AiRequestOptions::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn handle_request_with_options(
        &self,
        base_url: &str,
        api_key: &str,
        input: Value,
        supports_responses: bool,
        model: String,
        instructions: Option<String>,
        tools: Option<Vec<Value>>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
        options: AiRequestOptions,
    ) -> Result<AiResponse, String> {
        let provider = effective_provider_for_request(base_url, provider);
        let transport = if supports_responses {
            AiTransport::Responses
        } else {
            AiTransport::ChatCompletions
        };

        let first_payload = build_request_payload(
            transport,
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
        let first_attempt = self
            .send_payload(
                base_url,
                api_key,
                transport,
                first_payload.clone(),
                callbacks.clone(),
                provider.clone(),
                thinking_level.clone(),
                on_before_send_model_request.clone(),
                options.request_body_limit_bytes,
                options.abort_token.clone(),
                options.force_identity_encoding,
                options.stream,
            )
            .await;

        if transport == AiTransport::Responses
            && should_retry_without_prompt_cache_options(&first_attempt, &first_payload)
        {
            let mut retry_options = options.clone();
            retry_options.prompt_cache_key = None;
            retry_options.include_prompt_cache_retention = false;
            let retry_payload = build_request_payload(
                transport,
                input,
                model,
                instructions,
                tools,
                temperature,
                max_output_tokens,
                provider.clone(),
                thinking_level.clone(),
                &retry_options,
            );
            return self
                .send_payload(
                    base_url,
                    api_key,
                    transport,
                    retry_payload,
                    callbacks,
                    provider,
                    thinking_level,
                    on_before_send_model_request,
                    options.request_body_limit_bytes,
                    options.abort_token,
                    options.force_identity_encoding,
                    retry_options.stream,
                )
                .await;
        }

        first_attempt
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_prebuilt_payload_with_options(
        &self,
        base_url: &str,
        api_key: &str,
        transport: AiTransport,
        payload: Value,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
        options: AiRequestOptions,
    ) -> Result<AiResponse, String> {
        self.send_payload(
            base_url,
            api_key,
            transport,
            payload,
            callbacks,
            provider,
            thinking_level,
            on_before_send_model_request,
            options.request_body_limit_bytes,
            options.abort_token,
            options.force_identity_encoding,
            options.stream,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_payload(
        &self,
        base_url: &str,
        api_key: &str,
        transport: AiTransport,
        payload: Value,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
        request_body_limit_bytes: Option<usize>,
        abort_token: Option<CancellationToken>,
        force_identity_encoding: bool,
        stream: bool,
    ) -> Result<AiResponse, String> {
        let payload_body = serialize_request_payload(&payload)?;
        validate_request_payload_size(payload_body.len(), request_body_limit_bytes)?;
        if let Some(cb) = on_before_send_model_request {
            cb(payload.clone());
        }
        let url = match transport {
            AiTransport::Responses => format!("{}/responses", base_url.trim_end_matches('/')),
            AiTransport::ChatCompletions => {
                format!("{}/chat/completions", base_url.trim_end_matches('/'))
            }
        };
        info!(
            transport = transport_label(transport),
            url = url.as_str(),
            payload_bytes = payload_body.len(),
            stream,
            "dispatching ai provider request"
        );
        let request_started_at = Instant::now();
        let response = send_json_request(
            &self.client,
            url.as_str(),
            api_key,
            payload_body,
            abort_token.clone(),
            force_identity_encoding,
        )
        .await?;
        let response_headers_ms = request_started_at.elapsed().as_millis();
        info!(
            transport = transport_label(transport),
            url = url.as_str(),
            response_headers_ms,
            "ai provider response headers received"
        );
        if !response.status().is_success() {
            let status = response.status();
            let retry_after_ms = retry_after_delay_ms(response.headers());
            let body = read_error_response_text_limited(response).await;
            let body_preview = log_preview(body.as_str());
            warn!(
                transport = transport_label(transport),
                url = url.as_str(),
                status = status.as_u16(),
                retry_after_ms,
                response_body = body_preview.as_str(),
                "ai provider request failed"
            );
            let retry_hint = retry_after_ms
                .map(|value| format!(" [retry_after_ms={value}]"))
                .unwrap_or_default();
            return Err(format!("status {status}{retry_hint}: {body}"));
        }

        let parsed = parse_stream_response(
            response,
            transport,
            callbacks,
            provider.as_deref(),
            thinking_level.as_deref(),
            abort_token,
        )
        .await;
        match &parsed {
            Ok(ai_response) => {
                info!(
                    transport = transport_label(transport),
                    url = url.as_str(),
                    response_id = ai_response.response_id.as_deref().unwrap_or(""),
                    finish_reason = ai_response.finish_reason.as_deref().unwrap_or(""),
                    content_bytes = ai_response.content.len(),
                    reasoning_bytes = ai_response.reasoning.as_deref().map(str::len).unwrap_or(0),
                    tool_call_count = ai_response_tool_call_count(ai_response),
                    has_provider_error = ai_response.provider_error.is_some(),
                    has_usage = ai_response.usage.is_some(),
                    ai_provider_request_ms = request_started_at.elapsed().as_millis(),
                    stream,
                    "received ai provider response"
                );
            }
            Err(err) => {
                warn!(
                    transport = transport_label(transport),
                    url = url.as_str(),
                    error = err.as_str(),
                    stream,
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
    transport: AiTransport,
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
    match transport {
        AiTransport::Responses => build_responses_request_payload(
            input,
            model,
            instructions,
            options.prompt_cache_key.clone(),
            options.previous_response_id.clone(),
            tools,
            options.request_cwd.clone(),
            temperature,
            max_output_tokens,
            provider,
            thinking_level,
            options.stream,
            options.include_prompt_cache_retention,
            options.output_format.clone(),
        ),
        AiTransport::ChatCompletions => build_chat_completions_request_payload(
            input,
            model,
            instructions,
            tools,
            temperature,
            max_output_tokens,
            provider,
            thinking_level,
            options.stream,
            options.output_format.clone(),
        ),
    }
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
    let provider = provider?;
    let normalized = normalize_provider(provider.as_str());
    if normalized == "gpt" && !is_openai_api_base_url(base_url) {
        return Some("openai_compatible".to_string());
    }
    Some(provider)
}

fn is_openai_api_base_url(base_url: &str) -> bool {
    let value = base_url.trim().to_ascii_lowercase();
    value.is_empty() || value.contains("api.openai.com")
}

fn transport_label(transport: AiTransport) -> &'static str {
    match transport {
        AiTransport::Responses => "responses",
        AiTransport::ChatCompletions => "chat_completions",
    }
}

fn ai_response_tool_call_count(response: &AiResponse) -> usize {
    response
        .tool_calls
        .as_ref()
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}
