mod parser;
mod stream_request;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

use crate::services::ai_common::{
    build_abort_token, normalize_reasoning_effort, persist_assistant_response_with_policy,
    should_persist_assistant_message, validate_request_payload_size, AiStreamCallbacks,
    AssistantResponsePersistenceRequest,
};
use crate::services::v3::message_manager::MessageManager;
use crate::utils::model_config::is_gpt_provider;

const REQUEST_BODY_LIMIT_ENV: &str = "AI_V3_REQUEST_BODY_MAX_BYTES";
const CHAT_PROMPT_CACHE_RETENTION: &str = "24h";
const UPSTREAM_CONNECT_TIMEOUT_MS_ENV: &str = "AI_V3_UPSTREAM_CONNECT_TIMEOUT_MS";
const UPSTREAM_READ_TIMEOUT_MS_ENV: &str = "AI_V3_UPSTREAM_READ_TIMEOUT_MS";
const DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_UPSTREAM_READ_TIMEOUT_MS: u64 = 120_000;
const MIN_UPSTREAM_TIMEOUT_MS: u64 = 1_000;
const MAX_UPSTREAM_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub provider_error: Option<Value>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
}

pub type StreamCallbacks = AiStreamCallbacks;

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
            "[AI_V3] prompt_cache_retention init: baseURL={}, enabled={}, value={}",
            base_url,
            prompt_cache_retention_enabled,
            CHAT_PROMPT_CACHE_RETENTION
        );
        info!(
            "[AI_V3] http client timeout config: connect_timeout_ms={}, read_timeout_ms={}",
            connect_timeout_ms,
            read_timeout_ms
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
        purpose: &str,
    ) -> Result<AiResponse, String> {
        let mut payload = build_request_payload(
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
        );

        if let Err(err) = validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV) {
            error!(
                "[AI_V3] request payload rejected before send: purpose={}, detail={}",
                purpose, err
            );
            return Err(err);
        }

        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let token = build_abort_token(session_id.as_deref());

        info!(
            "[AI_V3] handleRequest start: purpose={}, model={}, stream={}, baseURL={}, session={}, tools={}, cwd={}",
            purpose,
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
        );

        let persist_messages = purpose != "agent_builder";
        let force_identity_encoding = purpose == "session_summary_job";

        let first_attempt = self
            .handle_stream_request(
                url.clone(),
                payload.clone(),
                callbacks.clone(),
                session_id.clone(),
                turn_id.clone(),
                token.clone(),
                force_identity_encoding,
                persist_messages,
                message_mode.clone(),
                message_source.clone(),
                metadata.clone(),
            )
            .await;

        if should_retry_without_prompt_cache_retention(&first_attempt, payload.as_object()) {
            warn!(
                "[AI_V3] upstream rejected prompt_cache_retention; disable and retry once: purpose={}, session={}",
                purpose,
                session_id.as_deref().unwrap_or("n/a")
            );
            self.prompt_cache_retention_enabled
                .store(false, Ordering::Relaxed);
            if let Some(object) = payload.as_object_mut() {
                object.remove("prompt_cache_retention");
            }
            log_request_fingerprint(
                purpose,
                session_id.as_deref(),
                self.base_url.as_str(),
                &payload,
            );
            return self
                .handle_stream_request(
                    url,
                    payload,
                    callbacks,
                    session_id,
                    turn_id,
                    token,
                    force_identity_encoding,
                    persist_messages,
                    message_mode,
                    message_source,
                    metadata,
                )
                .await;
        }

        first_attempt
    }
}

fn should_retry_without_prompt_cache_retention(
    first_attempt: &Result<AiResponse, String>,
    payload_object: Option<&serde_json::Map<String, Value>>,
) -> bool {
    if payload_object
        .and_then(|object| object.get("prompt_cache_retention"))
        .is_none()
    {
        return false;
    }
    match first_attempt {
        Ok(_) => false,
        Err(err) => is_prompt_cache_retention_unsupported_error(err.as_str()),
    }
}

fn is_prompt_cache_retention_unsupported_error(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    if !normalized.contains("prompt_cache_retention") {
        return false;
    }
    normalized.contains("unsupported parameter")
        || normalized.contains("unknown parameter")
        || normalized.contains("not supported")
}

fn base_url_supports_prompt_cache_retention(base_url: &str) -> bool {
    let normalized = base_url.trim().to_ascii_lowercase();
    normalized.contains("api.openai.com")
}

fn build_http_client() -> (reqwest::Client, u64, u64) {
    let connect_timeout_ms = read_timeout_env_ms(
        UPSTREAM_CONNECT_TIMEOUT_MS_ENV,
        DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS,
    );
    let read_timeout_ms =
        read_timeout_env_ms(UPSTREAM_READ_TIMEOUT_MS_ENV, DEFAULT_UPSTREAM_READ_TIMEOUT_MS);

    match reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(connect_timeout_ms))
        .read_timeout(Duration::from_millis(read_timeout_ms))
        .build()
    {
        Ok(client) => (client, connect_timeout_ms, read_timeout_ms),
        Err(err) => {
            warn!(
                "[AI_V3] failed to build reqwest client with timeout config; fallback default client: {}",
                err
            );
            (reqwest::Client::new(), connect_timeout_ms, read_timeout_ms)
        }
    }
}

fn read_timeout_env_ms(key: &str, default_ms: u64) -> u64 {
    let parsed = std::env::var(key)
        .ok()
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
        "[AI_V3]",
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
    let mut payload = json!({
        "model": model,
        "input": input
    });
    let mut has_prompt_cache_key = false;
    if let Some(instr) = instructions {
        payload["instructions"] = Value::String(instr);
    }
    if let Some(cache_key) = prompt_cache_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload["prompt_cache_key"] = Value::String(cache_key.to_string());
        has_prompt_cache_key = true;
    }
    if has_prompt_cache_key && include_prompt_cache_retention {
        payload["prompt_cache_retention"] =
            Value::String(CHAT_PROMPT_CACHE_RETENTION.to_string());
    }
    if let Some(t) = tools {
        if !t.is_empty() {
            payload["tools"] = Value::Array(t);
            payload["tool_choice"] = Value::String("auto".to_string());
        }
    }
    if let Some(cwd) = request_cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload["cwd"] = Value::String(cwd.to_string());
    }
    if let Some(t) = temperature {
        payload["temperature"] = json!(t);
    }
    if let Some(max) = max_output_tokens {
        payload["max_output_tokens"] = json!(max);
    }
    if let Some(level) = normalize_reasoning_effort(provider.as_deref(), thinking_level.as_deref())
    {
        let mut reasoning_payload = json!({ "effort": level });
        if is_gpt_provider(provider.as_deref().unwrap_or("gpt")) {
            reasoning_payload["summary"] = Value::String("auto".to_string());
        }
        payload["reasoning"] = reasoning_payload;
    }
    if stream {
        payload["stream"] = Value::Bool(true);
    }
    payload
}

fn log_request_fingerprint(
    purpose: &str,
    session_id: Option<&str>,
    base_url: &str,
    payload: &Value,
) {
    let input = payload
        .get("input")
        .cloned()
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
        "[AI_V3] request fingerprint: purpose={}, session={}, baseURL={}, input_items={}, tools={}, prompt_cache_key={}, prompt_cache_retention={}, input_hash={}, input_prefix_hash={}, tools_hash={}",
        purpose,
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
