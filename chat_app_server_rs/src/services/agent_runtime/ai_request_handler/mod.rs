#[cfg(test)]
mod parser;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chatos_ai_runtime::request_payload::{
    build_chat_completions_request_payload as build_shared_chat_completions_request_payload,
    build_responses_request_payload as build_shared_responses_request_payload,
    CHAT_PROMPT_CACHE_RETENTION,
};
#[cfg(test)]
use chatos_ai_runtime::request_retry::is_prompt_cache_retention_unsupported_error;
use chatos_ai_runtime::request_retry::{
    base_url_supports_prompt_cache_retention, should_retry_without_prompt_cache_retention,
};
pub use chatos_ai_runtime::AiResponse;
use chatos_ai_runtime::{
    AiRequestHandler as SharedAiRequestHandler, AiRequestOptions as SharedAiRequestOptions,
    AiTransport as SharedAiTransport, StreamCallbacks as SharedStreamCallbacks,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_common::{
    build_abort_token, persist_assistant_response_with_policy, should_persist_assistant_message,
    validate_request_payload_size, AiStreamCallbacks, AssistantResponsePersistenceRequest,
};

pub(crate) const AGENT_RUNTIME_LOG_PREFIX: &str = "[Agent Runtime]";
const REQUEST_BODY_LIMIT_ENV: &str = "AI_AGENT_REQUEST_BODY_MAX_BYTES";
const UPSTREAM_CONNECT_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_CONNECT_TIMEOUT_MS";
const UPSTREAM_READ_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_READ_TIMEOUT_MS";
const DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_UPSTREAM_READ_TIMEOUT_MS: u64 = 120_000;
const MIN_UPSTREAM_TIMEOUT_MS: u64 = 1_000;
const MAX_UPSTREAM_TIMEOUT_MS: u64 = 600_000;

pub type StreamCallbacks = AiStreamCallbacks;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTransport {
    Responses,
    ChatCompletions,
}

impl AiTransport {
    fn from_supports_responses(supports_responses: bool) -> Self {
        if supports_responses {
            Self::Responses
        } else {
            Self::ChatCompletions
        }
    }

    fn log_label(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }

    fn as_shared(self) -> SharedAiTransport {
        match self {
            Self::Responses => SharedAiTransport::Responses,
            Self::ChatCompletions => SharedAiTransport::ChatCompletions,
        }
    }

    fn persist_skip_log_label(self) -> &'static str {
        match self {
            Self::Responses => "non-terminal empty stream response",
            Self::ChatCompletions => "chat-completions empty stream response",
        }
    }
}

#[derive(Clone)]
pub struct AiRequestHandler {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    message_manager: MessageManager,
    prompt_cache_retention_enabled: Arc<AtomicBool>,
}

impl AiRequestHandler {
    pub fn new(api_key: String, base_url: String, message_manager: MessageManager) -> Self {
        let prompt_cache_retention_enabled =
            base_url_supports_prompt_cache_retention(base_url.as_str());
        let (client, connect_timeout_ms, read_timeout_ms) = build_http_client();
        info!(
            "{} prompt_cache_retention init: baseURL={}, enabled={}, value={}",
            AGENT_RUNTIME_LOG_PREFIX,
            base_url,
            prompt_cache_retention_enabled,
            CHAT_PROMPT_CACHE_RETENTION
        );
        info!(
            "{} http client timeout config: connect_timeout_ms={}, read_timeout_ms={}",
            AGENT_RUNTIME_LOG_PREFIX, connect_timeout_ms, read_timeout_ms
        );

        Self {
            client,
            base_url,
            api_key,
            message_manager,
            prompt_cache_retention_enabled: Arc::new(AtomicBool::new(
                prompt_cache_retention_enabled,
            )),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn handle_request(
        &self,
        input: Value,
        supports_responses: bool,
        model: String,
        instructions: Option<String>,
        prompt_cache_key: Option<String>,
        tools: Option<Vec<Value>>,
        request_cwd: Option<String>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        turn_id: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
        on_before_send_model_request: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
        purpose: &str,
    ) -> Result<AiResponse, String> {
        let transport = AiTransport::from_supports_responses(supports_responses);
        let mut payload = match transport {
            AiTransport::Responses => build_responses_request_payload(
                input,
                model,
                instructions,
                prompt_cache_key,
                tools,
                request_cwd,
                temperature,
                max_output_tokens,
                provider.clone(),
                thinking_level.clone(),
                true,
                self.prompt_cache_retention_enabled.load(Ordering::Relaxed),
            ),
            AiTransport::ChatCompletions => build_chat_completions_request_payload(
                input,
                model,
                instructions,
                tools,
                temperature,
                max_output_tokens,
                provider.clone(),
                thinking_level.clone(),
                true,
            ),
        };

        if let Err(err) = validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV) {
            error!(
                "{} request payload rejected before send: purpose={}, detail={}",
                AGENT_RUNTIME_LOG_PREFIX, purpose, err
            );
            return Err(err);
        }

        let token = build_abort_token(session_id.as_deref(), turn_id.as_deref());

        info!(
            "{} request start: purpose={}, transport={}, model={}, stream={}, baseURL={}, session={}, tools={}, cwd={}",
            AGENT_RUNTIME_LOG_PREFIX,
            purpose,
            transport.log_label(),
            payload.get("model").and_then(|v| v.as_str()).unwrap_or(""),
            true,
            self.base_url,
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            payload
                .get("tools")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0),
            payload.get("cwd").and_then(|value| value.as_str()).unwrap_or("")
        );
        log_request_fingerprint(
            purpose,
            session_id.as_deref(),
            self.base_url.as_str(),
            &payload,
            transport,
        );

        let persist_messages = purpose != "agent_builder";
        let force_identity_encoding = purpose == "session_summary_job";
        let first_attempt = self
            .send_prebuilt_payload_and_persist(
                transport,
                payload.clone(),
                callbacks.clone(),
                provider.clone(),
                thinking_level.clone(),
                session_id.clone(),
                turn_id.clone(),
                token.clone(),
                force_identity_encoding,
                persist_messages,
                message_mode.clone(),
                message_source.clone(),
                metadata.clone(),
                on_before_send_model_request.clone(),
            )
            .await;

        if transport == AiTransport::Responses
            && should_retry_without_prompt_cache_retention(&first_attempt, &payload)
        {
            warn!(
                "{} upstream rejected prompt_cache_retention; disable and retry once: purpose={}, session={}",
                AGENT_RUNTIME_LOG_PREFIX,
                purpose,
                session_id.as_deref().unwrap_or("n/a")
            );
            self.prompt_cache_retention_enabled
                .store(false, Ordering::Relaxed);
            if let Some(object) = payload.as_object_mut() {
                object.remove("prompt_cache_retention");
            }
            if let Some(cb) = on_before_send_model_request.as_ref() {
                cb(payload.clone());
            }
            log_request_fingerprint(
                purpose,
                session_id.as_deref(),
                self.base_url.as_str(),
                &payload,
                transport,
            );
            return self
                .send_prebuilt_payload_and_persist(
                    transport,
                    payload,
                    callbacks,
                    provider,
                    thinking_level,
                    session_id,
                    turn_id,
                    token,
                    force_identity_encoding,
                    persist_messages,
                    message_mode,
                    message_source,
                    metadata,
                    on_before_send_model_request,
                )
                .await;
        }

        first_attempt
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_prebuilt_payload_and_persist(
        &self,
        transport: AiTransport,
        payload: Value,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        turn_id: Option<String>,
        token: Option<tokio_util::sync::CancellationToken>,
        force_identity_encoding: bool,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
        on_before_send_model_request: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    ) -> Result<AiResponse, String> {
        let response = SharedAiRequestHandler::from_client(self.client.clone())
            .send_prebuilt_payload_with_options(
                self.base_url.as_str(),
                self.api_key.as_str(),
                transport.as_shared(),
                payload,
                to_shared_stream_callbacks(&callbacks),
                provider,
                thinking_level,
                on_before_send_model_request,
                SharedAiRequestOptions {
                    abort_token: token,
                    force_identity_encoding,
                    ..Default::default()
                },
            )
            .await?;

        info!(
            "{} stream response parsed: transport={}, session_id={}, turn_id={}, response_id={}, tool_call_count={}",
            AGENT_RUNTIME_LOG_PREFIX,
            transport.log_label(),
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
            response.response_id.as_deref().unwrap_or("none"),
            response
                .tool_calls
                .as_ref()
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        );

        persist_assistant_response_if_needed(
            self,
            session_id,
            turn_id,
            persist_messages,
            message_mode,
            message_source,
            metadata,
            response.content.as_str(),
            response.reasoning.clone(),
            response.tool_calls.clone(),
            response.response_id.clone(),
            response.finish_reason.clone(),
            transport.persist_skip_log_label(),
        )
        .await;

        Ok(response)
    }
}

fn build_http_client() -> (reqwest::Client, u64, u64) {
    let connect_timeout_ms = read_timeout_env_ms_with_fallback(
        UPSTREAM_CONNECT_TIMEOUT_MS_ENV,
        None,
        DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS,
    );
    let read_timeout_ms = read_timeout_env_ms_with_fallback(
        UPSTREAM_READ_TIMEOUT_MS_ENV,
        None,
        DEFAULT_UPSTREAM_READ_TIMEOUT_MS,
    );

    match reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(connect_timeout_ms))
        .read_timeout(Duration::from_millis(read_timeout_ms))
        .build()
    {
        Ok(client) => (client, connect_timeout_ms, read_timeout_ms),
        Err(err) => {
            warn!(
                "{} failed to build reqwest client with timeout config; fallback default client: {}",
                AGENT_RUNTIME_LOG_PREFIX,
                err
            );
            (reqwest::Client::new(), connect_timeout_ms, read_timeout_ms)
        }
    }
}

fn to_shared_stream_callbacks(callbacks: &StreamCallbacks) -> SharedStreamCallbacks {
    SharedStreamCallbacks {
        on_chunk: callbacks.on_chunk.clone(),
        on_thinking: callbacks.on_thinking.clone(),
    }
}

fn read_timeout_env_ms(key: &str, default_ms: u64) -> u64 {
    read_timeout_env_ms_with_fallback(key, None, default_ms)
}

fn read_timeout_env_ms_with_fallback(key: &str, legacy_key: Option<&str>, default_ms: u64) -> u64 {
    let parsed = std::env::var(key)
        .ok()
        .or_else(|| legacy_key.and_then(|legacy| std::env::var(legacy).ok()))
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(default_ms);
    parsed.clamp(MIN_UPSTREAM_TIMEOUT_MS, MAX_UPSTREAM_TIMEOUT_MS)
}

pub(super) async fn persist_assistant_response_if_needed(
    handler: &AiRequestHandler,
    session_id: Option<String>,
    turn_id: Option<String>,
    persist_messages: bool,
    message_mode: Option<String>,
    message_source: Option<String>,
    metadata: Option<Value>,
    content: &str,
    reasoning: Option<String>,
    tool_calls: Option<Value>,
    response_id: Option<String>,
    finish_reason: Option<String>,
    skip_log_label: &str,
) {
    let request = AssistantResponsePersistenceRequest {
        session_id,
        turn_id,
        persist_messages,
        message_mode,
        message_source,
        metadata,
        content: content.to_string(),
        reasoning,
        tool_calls,
        response_id,
        response_status: finish_reason,
    };
    let should_persist = should_persist_assistant_message(
        request.content.as_str(),
        request.reasoning.as_deref(),
        request.tool_calls.as_ref(),
        request.response_status.as_deref(),
    );

    persist_assistant_response_with_policy(
        request,
        should_persist,
        AGENT_RUNTIME_LOG_PREFIX,
        Some(skip_log_label),
        |request| async move {
            let Some(session_id) = request.session_id.as_deref() else {
                return Ok(());
            };

            handler
                .message_manager
                .save_assistant_response_message(
                    session_id,
                    request.content.as_str(),
                    request.reasoning,
                    request.message_mode,
                    request.message_source,
                    request.metadata,
                    request.tool_calls,
                    request.response_id.as_deref(),
                    request.turn_id.as_deref(),
                    request.response_status.as_deref(),
                )
                .await
                .map(|_| ())
        },
    )
    .await;
}

fn build_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    prompt_cache_key: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
    include_prompt_cache_retention: bool,
) -> Value {
    build_responses_request_payload(
        input,
        model,
        instructions,
        prompt_cache_key,
        tools,
        request_cwd,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
        include_prompt_cache_retention,
    )
}

fn build_responses_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    prompt_cache_key: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
    include_prompt_cache_retention: bool,
) -> Value {
    build_shared_responses_request_payload(
        input,
        model,
        instructions,
        prompt_cache_key,
        tools,
        request_cwd,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
        include_prompt_cache_retention,
    )
}

fn build_chat_completions_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    tools: Option<Vec<Value>>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
) -> Value {
    build_shared_chat_completions_request_payload(
        input,
        model,
        instructions,
        tools,
        temperature,
        max_output_tokens,
        provider,
        thinking_level,
        stream,
    )
}

fn chat_message_content_to_text(content: &Value) -> String {
    crate::core::messages::join_text_lines_or_json(
        content,
        &["text", "value", "content", "delta", "output_text", "output"],
    )
}

fn log_request_fingerprint(
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
