use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::core::messages::owned_non_empty_text;
use crate::services::ai_common::{
    build_abort_token, consume_sse_stream, emit_stream_callbacks, normalize_reasoning_effort,
    parsed_stream_response_is_empty, persist_assistant_response_with_policy,
    read_error_response_text, send_bearer_json_request, validate_request_payload_size,
    AiStreamCallbacks, AssistantResponsePersistenceRequest, EMPTY_STREAM_RESPONSE_PARSE_ERROR,
};
use crate::services::v2::message_manager::MessageManager;

mod parser;

use self::parser::{
    apply_stream_event, collect_tool_calls, StreamState,
};

const REQUEST_BODY_LIMIT_ENV: &str = "AI_V2_REQUEST_BODY_MAX_BYTES";

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
}

pub type StreamCallbacks = AiStreamCallbacks;

#[derive(Clone)]
pub struct AiRequestHandler {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    message_manager: MessageManager,
}

impl AiRequestHandler {
    pub fn new(api_key: String, base_url: String, message_manager: MessageManager) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_key,
            message_manager,
        }
    }

    pub async fn handle_request(
        &self,
        messages: Vec<Value>,
        tools: Option<Vec<Value>>,
        model: String,
        _temperature: Option<f64>,
        max_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        reasoning_enabled: bool,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        turn_id: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        purpose: &str,
    ) -> Result<AiResponse, String> {
        let mut payload = json!({
            "model": model,
            "messages": messages,
        });
        if let Some(t) = tools {
            if !t.is_empty() {
                payload["tools"] = Value::Array(t);
                payload["tool_choice"] = Value::String("auto".to_string());
            }
        }
        // Intentionally omit temperature to match Node behavior (use provider defaults).
        if let Some(mt) = max_tokens {
            payload["max_tokens"] = Value::Number(serde_json::Number::from(mt));
        }

        if let Some(level) =
            normalize_reasoning_effort(provider.as_deref(), thinking_level.as_deref())
        {
            payload["reasoning_effort"] = Value::String(level);
        }

        payload["stream"] = Value::Bool(true);
        payload["stream_options"] = json!({"include_usage": true});

        if let Err(err) = validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV) {
            error!(
                "[AI] request payload rejected before send: purpose={}, detail={}",
                purpose, err
            );
            return Err(err);
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let token = build_abort_token(session_id.as_deref());

        info!(
            "[AI] handleRequest start: purpose={}, model={}, stream={}, baseURL={}, session={}",
            purpose,
            payload["model"].as_str().unwrap_or(""),
            true,
            self.base_url,
            session_id.clone().unwrap_or_else(|| "n/a".to_string())
        );

        let persist_messages = purpose != "agent_builder";
        let force_identity_encoding = purpose == "session_summary_job";

        self.handle_stream_request(
            url,
            payload,
            callbacks,
            reasoning_enabled,
            session_id,
            turn_id,
            token,
            force_identity_encoding,
            persist_messages,
            message_mode,
            message_source,
        )
        .await
    }

    async fn handle_stream_request(
        &self,
        url: String,
        payload: Value,
        callbacks: StreamCallbacks,
        reasoning_enabled: bool,
        session_id: Option<String>,
        turn_id: Option<String>,
        token: Option<CancellationToken>,
        force_identity_encoding: bool,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
    ) -> Result<AiResponse, String> {
        let resp = send_bearer_json_request(
            &self.client,
            &url,
            &self.api_key,
            &payload,
            token.clone(),
            force_identity_encoding,
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let err = read_error_response_text(resp)
                .await
                .unwrap_or_else(|inner| format!("status {}: {}", status, inner));
            error!("[AI] stream request failed: {}", err);
            return Err(err);
        }

        let stream = resp.bytes_stream();
        let mut stream_state = StreamState::default();
        let mut parsed_event_count: usize = 0;

        consume_sse_stream(stream, token.clone(), |v| {
            parsed_event_count += 1;
            let payload = apply_stream_event(&mut stream_state, &v, reasoning_enabled);
            emit_stream_callbacks(&callbacks, payload.chunk, payload.thinking);
        })
        .await?;

        let parsed_empty_response = parsed_stream_response_is_empty(
            parsed_event_count,
            stream_state.full_content.as_str(),
            stream_state.reasoning.as_str(),
            !stream_state.tool_calls_map.is_empty(),
        );
        if parsed_empty_response {
            return Err(EMPTY_STREAM_RESPONSE_PARSE_ERROR.to_string());
        }

        let tool_calls = collect_tool_calls(&stream_state.tool_calls_map);
        let reasoning_opt = owned_non_empty_text(stream_state.reasoning.as_str());

        persist_assistant_response_if_needed(
            self,
            session_id,
            turn_id,
            persist_messages,
            message_mode,
            message_source,
            stream_state.full_content.as_str(),
            reasoning_opt.clone(),
            tool_calls.clone(),
            stream_state.finish_reason.clone(),
        )
        .await;

        Ok(AiResponse {
            content: stream_state.full_content,
            reasoning: reasoning_opt,
            tool_calls,
            finish_reason: stream_state.finish_reason,
            usage: stream_state.usage,
        })
    }
}

async fn persist_assistant_response_if_needed(
    handler: &AiRequestHandler,
    session_id: Option<String>,
    turn_id: Option<String>,
    persist_messages: bool,
    message_mode: Option<String>,
    message_source: Option<String>,
    content: &str,
    reasoning: Option<String>,
    tool_calls: Option<Value>,
    finish_reason: Option<String>,
) {
    let request = AssistantResponsePersistenceRequest {
        session_id,
        turn_id,
        persist_messages,
        message_mode,
        message_source,
        content: content.to_string(),
        reasoning,
        tool_calls,
        response_id: None,
        response_status: finish_reason,
    };

    persist_assistant_response_with_policy(request, true, "[AI]", None, |request| async move {
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
                request.tool_calls,
                None,
                request.turn_id.as_deref(),
                request.response_status.as_deref(),
            )
            .await
            .map(|_| ())
    })
    .await;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::services::ai_common::validate_request_payload_size;

    use super::REQUEST_BODY_LIMIT_ENV;

    #[test]
    fn payload_precheck_accepts_small_payload() {
        let payload = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}]
        });
        assert!(validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV).is_ok());
    }

    #[test]
    fn payload_precheck_rejects_oversized_payload() {
        let payload = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "a".repeat(1_700_000)}]
        });
        let err = validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV)
            .expect_err("should reject");
        assert!(err.contains("request body too large"));
    }
}
