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
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::utils::model_config::is_gpt_provider;

pub(crate) const AGENT_RUNTIME_LOG_PREFIX: &str = "[Agent Runtime]";
const REQUEST_BODY_LIMIT_ENV: &str = "AI_AGENT_REQUEST_BODY_MAX_BYTES";
const CHAT_PROMPT_CACHE_RETENTION: &str = "24h";
const UPSTREAM_CONNECT_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_CONNECT_TIMEOUT_MS";
const UPSTREAM_READ_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_READ_TIMEOUT_MS";
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
            AGENT_RUNTIME_LOG_PREFIX,
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
                AGENT_RUNTIME_LOG_PREFIX,
                purpose, err
            );
            return Err(err);
        }

        let url = match transport {
            AiTransport::Responses => {
                format!("{}/responses", self.base_url.trim_end_matches('/'))
            }
            AiTransport::ChatCompletions => {
                format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
            }
        };
        let token = build_abort_token(session_id.as_deref());

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
        if let Some(cb) = on_before_send_model_request.as_ref() {
            cb(payload.clone());
        }

        let first_attempt = match transport {
            AiTransport::Responses => {
                self.handle_stream_request(
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
                .await
            }
            AiTransport::ChatCompletions => {
                self.handle_chat_completions_stream_request(
                    url.clone(),
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
                )
                .await
            }
        };

        if transport == AiTransport::Responses
            && should_retry_without_prompt_cache_retention(&first_attempt, payload.as_object())
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

fn read_timeout_env_ms(key: &str, default_ms: u64) -> u64 {
    read_timeout_env_ms_with_fallback(key, None, default_ms)
}

fn read_timeout_env_ms_with_fallback(
    key: &str,
    legacy_key: Option<&str>,
    default_ms: u64,
) -> u64 {
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
    let mut messages = input_to_chat_completions_messages(input);
    if let Some(system_prompt) = instructions
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        messages.insert(
            0,
            json!({
                "role": "system",
                "content": system_prompt
            }),
        );
    }

    let mut payload = json!({
        "model": model,
        "messages": messages,
    });
    if let Some(t) = tools {
        if !t.is_empty() {
            payload["tools"] = Value::Array(
                t.into_iter()
                    .map(chat_completion_tool_definition)
                    .collect(),
            );
            payload["tool_choice"] = Value::String("auto".to_string());
        }
    }
    if let Some(t) = temperature {
        payload["temperature"] = json!(t);
    }
    if let Some(max) = max_output_tokens {
        payload["max_tokens"] = json!(max);
    }
    if let Some(level) = normalize_reasoning_effort(provider.as_deref(), thinking_level.as_deref())
    {
        payload["reasoning_effort"] = Value::String(level);
    }
    if stream {
        payload["stream"] = Value::Bool(true);
        payload["stream_options"] = json!({"include_usage": true});
    }
    payload
}

fn input_to_chat_completions_messages(input: Value) -> Vec<Value> {
    match input {
        Value::String(text) => vec![json!({
            "role": "user",
            "content": text,
        })],
        Value::Array(items) => response_items_to_chat_messages(items),
        other => vec![json!({
            "role": "user",
            "content": other.to_string(),
        })],
    }
}

fn response_items_to_chat_messages(items: Vec<Value>) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut index = 0;

    while index < items.len() {
        let item = &items[index];
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

        if item_type == "message" {
            if let Some(mut message) = response_message_item_to_chat_message(item) {
                index += 1;

                let role = message.get("role").and_then(Value::as_str).unwrap_or("");
                if role == "assistant" {
                    let mut tool_calls = Vec::new();
                    while index < items.len()
                        && items[index].get("type").and_then(Value::as_str)
                            == Some("function_call")
                    {
                        tool_calls.push(chat_function_call_item_to_tool_call(&items[index]));
                        index += 1;
                    }
                    if !tool_calls.is_empty() {
                        message["tool_calls"] = Value::Array(tool_calls);
                    }
                }

                messages.push(message);
                continue;
            }
        }

        if item_type == "function_call" {
            let mut tool_calls = Vec::new();
            while index < items.len()
                && items[index].get("type").and_then(Value::as_str) == Some("function_call")
            {
                tool_calls.push(chat_function_call_item_to_tool_call(&items[index]));
                index += 1;
            }
            if !tool_calls.is_empty() {
                messages.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": tool_calls,
                }));
            }
            continue;
        }

        if let Some(message) = response_item_to_chat_message(item.clone()) {
            messages.push(message);
        }
        index += 1;
    }

    drop_incomplete_tool_call_messages(messages)
}

fn response_message_item_to_chat_message(item: &Value) -> Option<Value> {
    let role = item
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = item
        .get("content")
        .map(chat_message_content_to_value)
        .unwrap_or_else(|| Value::String(String::new()));
    let mut message = json!({
        "role": role,
        "content": content,
    });
    if role == "assistant" {
        if let Some(reasoning_content) = chat_message_reasoning_content(item) {
            message["reasoning_content"] = Value::String(reasoning_content);
        }
    }
    Some(message)
}

fn drop_incomplete_tool_call_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut output = Vec::with_capacity(messages.len());
    let mut index = 0;

    while index < messages.len() {
        let message = &messages[index];
        let tool_call_ids: Vec<String> = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str))
                    .filter(|id| !id.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        if tool_call_ids.is_empty() {
            output.push(message.clone());
            index += 1;
            continue;
        }

        let mut scan = index + 1;
        let mut seen_tool_ids = std::collections::HashSet::new();
        while scan < messages.len()
            && messages[scan].get("role").and_then(Value::as_str) == Some("tool")
        {
            if let Some(id) = messages[scan]
                .get("tool_call_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                seen_tool_ids.insert(id.to_string());
            }
            scan += 1;
        }

        if tool_call_ids.iter().all(|id| seen_tool_ids.contains(id)) {
            output.push(message.clone());
            for tool_message in messages.iter().take(scan).skip(index + 1) {
                output.push(tool_message.clone());
            }
        }

        index = scan;
    }

    output
}

fn response_item_to_chat_message(item: Value) -> Option<Value> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    match item_type {
        "message" => response_message_item_to_chat_message(&item),
        "function_call" => Some(json!({
            "role": "assistant",
            "content": Value::Null,
            "tool_calls": [chat_function_call_item_to_tool_call(&item)],
        })),
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let output = item
                .get("output")
                .map(|value| chat_message_content_to_text(value))
                .unwrap_or_default();
            Some(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }))
        }
        _ => None,
    }
}

fn chat_message_reasoning_content(item: &Value) -> Option<String> {
    item.get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            item.get("reasoning")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            let content = item.get("content")?;
            let parts = content.as_array()?;
            let mut chunks = Vec::new();
            for part in parts {
                let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
                if part_type == "reasoning" || part_type == "reasoning_content" {
                    let text = part
                        .get("text")
                        .or_else(|| part.get("content"))
                        .or_else(|| part.get("reasoning"))
                        .map(chat_message_content_to_text)
                        .unwrap_or_else(|| chat_message_content_to_text(part));
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        chunks.push(trimmed.to_string());
                    }
                }
            }
            if chunks.is_empty() {
                None
            } else {
                Some(chunks.join(""))
            }
        })
}

fn chat_function_call_item_to_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| item.get("id").and_then(Value::as_str))
        .unwrap_or("");
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": arguments,
        }
    })
}

fn chat_completion_tool_definition(tool: Value) -> Value {
    if tool
        .get("function")
        .and_then(Value::as_object)
        .is_some()
    {
        return tool;
    }

    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));

    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        }
    })
}

fn chat_message_content_to_value(content: &Value) -> Value {
    match content {
        Value::String(text) => Value::String(text.clone()),
        Value::Array(parts) => {
            let normalized: Vec<Value> = parts
                .iter()
                .filter_map(chat_content_part_to_value)
                .collect();
            if normalized.is_empty() {
                Value::String(chat_message_content_to_text(content))
            } else {
                Value::Array(normalized)
            }
        }
        other => Value::String(chat_message_content_to_text(other)),
    }
}

fn chat_content_part_to_value(part: &Value) -> Option<Value> {
    let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
    match part_type {
        "input_text" | "output_text" | "text" => Some(json!({
            "type": "text",
            "text": part
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| chat_message_content_to_text(part)),
        })),
        "input_image" => {
            let image_url = part
                .get("image_url")
                .and_then(|value| value.as_str().map(|inner| inner.to_string()).or_else(|| value.get("url").and_then(Value::as_str).map(|inner| inner.to_string())))
                .unwrap_or_default();
            if image_url.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image_url,
                        "detail": part.get("detail").cloned().unwrap_or(Value::String("auto".to_string())),
                    }
                }))
            }
        }
        "image_url" => {
            let image_url = part
                .get("image_url")
                .and_then(|value| value.as_str().map(|inner| inner.to_string()).or_else(|| value.get("url").and_then(Value::as_str).map(|inner| inner.to_string())))
                .unwrap_or_default();
            if image_url.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image_url,
                        "detail": part.get("detail").cloned().unwrap_or(Value::String("auto".to_string())),
                    }
                }))
            }
        }
        _ => {
            let text = chat_message_content_to_text(part);
            if text.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "text",
                    "text": text,
                }))
            }
        }
    }
}

fn chat_message_content_to_text(content: &Value) -> String {
    crate::core::messages::join_text_lines_or_json(content, &["text", "value", "content", "delta", "output_text", "output"])
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
