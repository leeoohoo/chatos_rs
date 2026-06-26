use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::model_config::{
    normalize_provider, reasoning_effort_for_provider, thinking_mode_for_provider,
};
#[cfg(test)]
use crate::request_payload::response_items_to_chat_messages;
use crate::request_payload::{
    build_chat_completions_request_payload, build_responses_request_payload,
};
use crate::request_retry::should_retry_without_prompt_cache_retention;
use crate::stream::consume_sse_stream;
use crate::stream_parse::{
    apply_chat_completions_stream_event, apply_responses_stream_event,
    finalize_chat_completions_stream_state, finalize_responses_stream_state, FinalizedStreamState,
    StreamState,
};
use crate::tool_call::collect_ordered_tool_calls;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(120);
const EMPTY_STREAM_RESPONSE_PARSE_ERROR: &str =
    "stream response parse failed: no valid SSE events parsed from provider";

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

#[derive(Clone, Default)]
pub struct StreamCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTransport {
    Responses,
    ChatCompletions,
}

#[derive(Clone, Debug, Default)]
pub struct AiRequestOptions {
    pub prompt_cache_key: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub abort_token: Option<CancellationToken>,
    pub force_identity_encoding: bool,
}

#[derive(Clone)]
pub struct AiRequestHandler {
    client: reqwest::Client,
}

impl AiRequestHandler {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .read_timeout(DEFAULT_READ_TIMEOUT)
            .build()
            .unwrap_or_else(|err| {
                warn!("failed to build AI http client with timeouts: {err}");
                reqwest::Client::new()
            });
        Self { client }
    }

    pub fn from_client(client: reqwest::Client) -> Self {
        Self { client }
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
            )
            .await;

        if transport == AiTransport::Responses
            && should_retry_without_prompt_cache_retention(&first_attempt, &first_payload)
        {
            let mut retry_options = options.clone();
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
            let body = response.text().await.unwrap_or_default();
            let body_preview = log_preview(body.as_str());
            warn!(
                transport = transport_label(transport),
                url = url.as_str(),
                status = status.as_u16(),
                response_body = body_preview.as_str(),
                "ai provider request failed"
            );
            return Err(format!("status {status}: {body}"));
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
                    "received ai provider response"
                );
            }
            Err(err) => {
                warn!(
                    transport = transport_label(transport),
                    url = url.as_str(),
                    error = err.as_str(),
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

async fn send_json_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    payload_body: Vec<u8>,
    abort_token: Option<CancellationToken>,
    force_identity_encoding: bool,
) -> Result<reqwest::Response, String> {
    let mut request = client
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload_body);
    if force_identity_encoding {
        request = request
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }

    let future = request.send();
    if let Some(token) = abort_token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            response = future => response.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
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
            tools,
            options.request_cwd.clone(),
            temperature,
            max_output_tokens,
            provider,
            thinking_level,
            true,
            options.include_prompt_cache_retention,
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
            true,
        ),
    }
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

async fn parse_stream_response(
    response: reqwest::Response,
    transport: AiTransport,
    callbacks: StreamCallbacks,
    provider: Option<&str>,
    thinking_level: Option<&str>,
    abort_token: Option<CancellationToken>,
) -> Result<AiResponse, String> {
    let mut state = StreamState::default();
    let mut parsed_event_count = 0usize;
    let mut sent_any_thinking = false;
    let reasoning_enabled = reasoning_effort_for_provider(provider, thinking_level).is_some()
        || thinking_mode_for_provider(provider, thinking_level) == Some("enabled");

    consume_sse_stream(response.bytes_stream(), abort_token, |event| {
        parsed_event_count += 1;
        let payload = match transport {
            AiTransport::Responses => apply_responses_stream_event(&mut state, &event),
            AiTransport::ChatCompletions => {
                apply_chat_completions_stream_event(&mut state, &event, reasoning_enabled)
            }
        };
        if let Some(chunk) = payload.chunk {
            if let Some(cb) = &callbacks.on_chunk {
                cb(chunk);
            }
        }
        if let Some(thinking) = payload.thinking {
            sent_any_thinking = true;
            if let Some(cb) = &callbacks.on_thinking {
                cb(thinking);
            }
        }
    })
    .await?;

    if parsed_stream_response_is_empty(parsed_event_count, &state) {
        return Err(EMPTY_STREAM_RESPONSE_PARSE_ERROR.to_string());
    }

    let finalized = match transport {
        AiTransport::Responses => finalize_responses_stream_state(&mut state),
        AiTransport::ChatCompletions => finalize_chat_completions_stream_state(&mut state),
    };

    emit_finalized_stream_callbacks(
        &finalized,
        state.sent_any_chunk,
        sent_any_thinking,
        &callbacks,
    );

    Ok(AiResponse {
        content: finalized.content,
        reasoning: finalized.reasoning,
        tool_calls: match transport {
            AiTransport::Responses => finalized.tool_calls,
            AiTransport::ChatCompletions => {
                collect_tool_calls(&state.tool_calls_map).or(finalized.tool_calls)
            }
        },
        finish_reason: finalized.finish_reason,
        provider_error: finalized.provider_error,
        usage: finalized.usage,
        response_id: finalized.response_id,
    })
}

fn emit_finalized_stream_callbacks(
    finalized: &FinalizedStreamState,
    sent_any_chunk: bool,
    sent_any_thinking: bool,
    callbacks: &StreamCallbacks,
) {
    if !sent_any_chunk && !finalized.content.is_empty() {
        if let Some(cb) = &callbacks.on_chunk {
            cb(finalized.content.clone());
        }
    }

    if sent_any_thinking {
        return;
    }
    if let Some(reasoning) = finalized.reasoning.as_deref().map(str::trim) {
        if !reasoning.is_empty() {
            if let Some(cb) = &callbacks.on_thinking {
                cb(reasoning.to_string());
            }
        }
    }
}

fn collect_tool_calls(tool_calls: &BTreeMap<usize, Value>) -> Option<Value> {
    collect_ordered_tool_calls(tool_calls).and_then(|value| {
        let calls = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| {
                item.get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
            })
            .collect::<Vec<_>>();
        if calls.is_empty() {
            None
        } else {
            Some(Value::Array(calls))
        }
    })
}

fn parsed_stream_response_is_empty(parsed_event_count: usize, state: &StreamState) -> bool {
    parsed_event_count == 0
        && state.full_content.trim().is_empty()
        && state.reasoning.trim().is_empty()
        && state.tool_calls_map.is_empty()
        && state.response_obj.is_none()
        && state.provider_error.is_none()
}

fn serialize_request_payload(payload: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(payload)
        .map_err(|err| format!("failed to serialize AI request payload: {err}"))
}

fn validate_request_payload_size(
    size: usize,
    request_body_limit_bytes: Option<usize>,
) -> Result<(), String> {
    let Some(limit) = request_body_limit_bytes.filter(|value| *value > 0) else {
        return Ok(());
    };
    if size > limit {
        Err(format!(
            "AI request payload too large: {size} bytes exceeds {limit} bytes"
        ))
    } else {
        Ok(())
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

fn log_preview(value: &str) -> String {
    const MAX_LOG_PREVIEW_CHARS: usize = 2_000;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_LOG_PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let preview = trimmed
        .chars()
        .take(MAX_LOG_PREVIEW_CHARS)
        .collect::<String>();
    format!("{preview}... [truncated]")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};

    use super::{
        build_chat_completions_request_payload, build_responses_request_payload,
        effective_provider_for_request, emit_finalized_stream_callbacks,
        response_items_to_chat_messages, AiRequestOptions, StreamCallbacks,
    };
    use crate::stream_parse::FinalizedStreamState;

    #[test]
    fn response_items_to_chat_messages_keeps_complete_tool_exchange() {
        let messages = response_items_to_chat_messages(vec![
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "checking"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call_1",
                "name": "memory_search",
                "arguments": "{\"q\":\"rust\"}"
            }),
            json!({
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "done"
            }),
        ]);

        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0]
                .get("tool_calls")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            messages[1].get("role").and_then(Value::as_str),
            Some("tool")
        );
    }

    #[test]
    fn response_items_to_chat_messages_drops_incomplete_tool_exchange() {
        let messages = response_items_to_chat_messages(vec![
            json!({
                "type": "function_call",
                "call_id": "call_1",
                "name": "memory_search",
                "arguments": "{}"
            }),
            json!({
                "type": "message",
                "role": "user",
                "content": "next"
            }),
        ]);

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].get("role").and_then(Value::as_str),
            Some("user")
        );
    }

    #[test]
    fn deepseek_thinking_chat_payload_skips_temperature() {
        let payload = build_chat_completions_request_payload(
            json!("hello"),
            "deepseek-reasoner".to_string(),
            None,
            None,
            Some(0.7),
            None,
            Some("deepseek".to_string()),
            Some("high".to_string()),
            true,
        );

        assert!(payload.get("temperature").is_none());
        assert_eq!(
            payload.get("thinking").and_then(|value| value.get("type")),
            Some(&Value::String("enabled".to_string()))
        );
        assert_eq!(
            payload.get("reasoning_effort"),
            Some(&Value::String("high".to_string()))
        );
    }

    #[test]
    fn responses_payload_supports_prompt_cache_and_cwd() {
        let options = AiRequestOptions {
            prompt_cache_key: Some("session_1".to_string()),
            request_cwd: Some("/workspace".to_string()),
            include_prompt_cache_retention: true,
            request_body_limit_bytes: None,
            abort_token: None,
            force_identity_encoding: false,
        };
        let payload = build_responses_request_payload(
            json!([]),
            "gpt-4.1".to_string(),
            Some("system".to_string()),
            options.prompt_cache_key,
            None,
            options.request_cwd,
            None,
            None,
            Some("gpt".to_string()),
            Some("medium".to_string()),
            true,
            options.include_prompt_cache_retention,
        );

        assert_eq!(
            payload.get("prompt_cache_key"),
            Some(&Value::String("session_1".to_string()))
        );
        assert_eq!(
            payload.get("prompt_cache_retention"),
            Some(&Value::String("24h".to_string()))
        );
        assert_eq!(
            payload.get("cwd"),
            Some(&Value::String("/workspace".to_string()))
        );
    }

    #[test]
    fn responses_payload_requests_summary_for_gpt_model_on_compatible_provider() {
        let payload = build_responses_request_payload(
            json!([]),
            "gpt-5.4".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("openai_compatible".to_string()),
            Some("xhigh".to_string()),
            true,
            false,
        );

        assert_eq!(
            payload.pointer("/reasoning/effort"),
            Some(&Value::String("xhigh".to_string()))
        );
        assert_eq!(
            payload.pointer("/reasoning/summary"),
            Some(&Value::String("auto".to_string()))
        );
    }

    #[test]
    fn responses_payload_omits_summary_for_generic_compatible_model() {
        let payload = build_responses_request_payload(
            json!([]),
            "generic-compatible-model".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("openai_compatible".to_string()),
            Some("high".to_string()),
            true,
            false,
        );

        assert_eq!(
            payload.pointer("/reasoning/effort"),
            Some(&Value::String("high".to_string()))
        );
        assert!(payload.pointer("/reasoning/summary").is_none());
    }

    #[test]
    fn custom_openai_base_url_uses_compatible_provider() {
        assert_eq!(
            effective_provider_for_request(
                "https://gateway.example.test/v1",
                Some("openai".to_string()),
            )
            .as_deref(),
            Some("openai_compatible")
        );
        assert_eq!(
            effective_provider_for_request(
                "https://api.openai.com/v1",
                Some("openai".to_string()),
            )
            .as_deref(),
            Some("openai")
        );
    }

    #[test]
    fn finalized_stream_callbacks_emit_final_reasoning_when_no_stream_thinking() {
        let thinkings = Arc::new(Mutex::new(Vec::<String>::new()));
        let callbacks = StreamCallbacks {
            on_chunk: None,
            on_thinking: Some(Arc::new({
                let thinkings = thinkings.clone();
                move |value| {
                    thinkings.lock().expect("lock poisoned").push(value);
                }
            })),
        };
        let finalized = FinalizedStreamState {
            content: "done".to_string(),
            reasoning: Some("final reasoning".to_string()),
            ..FinalizedStreamState::default()
        };

        emit_finalized_stream_callbacks(&finalized, true, false, &callbacks);

        assert_eq!(
            thinkings.lock().expect("lock poisoned").as_slice(),
            ["final reasoning"]
        );
    }

    #[test]
    fn finalized_stream_callbacks_do_not_duplicate_streamed_thinking() {
        let thinkings = Arc::new(Mutex::new(Vec::<String>::new()));
        let callbacks = StreamCallbacks {
            on_chunk: None,
            on_thinking: Some(Arc::new({
                let thinkings = thinkings.clone();
                move |value| {
                    thinkings.lock().expect("lock poisoned").push(value);
                }
            })),
        };
        let finalized = FinalizedStreamState {
            content: "done".to_string(),
            reasoning: Some("final reasoning".to_string()),
            ..FinalizedStreamState::default()
        };

        emit_finalized_stream_callbacks(&finalized, true, true, &callbacks);

        assert!(thinkings.lock().expect("lock poisoned").is_empty());
    }
}
