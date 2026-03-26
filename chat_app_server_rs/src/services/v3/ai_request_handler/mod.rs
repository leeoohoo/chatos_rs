mod normal_request;
mod parser;
mod stream_request;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{error, info};

use crate::services::ai_common::{
    build_abort_token, normalize_reasoning_effort, validate_request_payload_size,
};
use crate::services::v3::message_manager::MessageManager;
use crate::utils::model_config::is_gpt_provider;

const REQUEST_BODY_LIMIT_ENV: &str = "AI_V3_REQUEST_BODY_MAX_BYTES";

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

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn handle_request(
        &self,
        input: Value,
        model: String,
        instructions: Option<String>,
        previous_response_id: Option<String>,
        tools: Option<Vec<Value>>,
        request_cwd: Option<String>,
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        turn_id: Option<String>,
        stream: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        purpose: &str,
    ) -> Result<AiResponse, String> {
        let payload = build_request_payload(
            input,
            model,
            instructions,
            previous_response_id,
            tools,
            request_cwd,
            temperature,
            max_output_tokens,
            provider.clone(),
            thinking_level.clone(),
            stream,
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
            stream,
            self.base_url,
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            payload
                .get("tools")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0),
            payload.get("cwd").and_then(|value| value.as_str()).unwrap_or("")
        );

        let persist_messages = purpose != "agent_builder";
        let force_identity_encoding = purpose == "session_summary_job";

        if stream {
            self.handle_stream_request(
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
            )
            .await
        } else {
            self.handle_normal_request(
                url,
                payload,
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
    }
}

pub(super) fn is_non_terminal_response_status(status: Option<&str>) -> bool {
    let normalized = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    matches!(
        normalized.as_deref(),
        Some("in_progress") | Some("queued") | Some("pending") | Some("incomplete")
    )
}

pub(super) fn should_persist_assistant_message(
    content: &str,
    reasoning: Option<&str>,
    tool_calls: Option<&Value>,
    finish_reason: Option<&str>,
) -> bool {
    let has_content = !content.trim().is_empty();
    let has_reasoning = reasoning
        .map(str::trim)
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    let has_tool_calls = tool_calls
        .and_then(|value| value.as_array())
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if has_content || has_reasoning || has_tool_calls {
        return true;
    }
    !is_non_terminal_response_status(finish_reason)
}

fn build_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    previous_response_id: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
) -> Value {
    let mut payload = json!({
        "model": model,
        "input": input
    });
    if let Some(instr) = instructions {
        payload["instructions"] = Value::String(instr);
    }
    if let Some(prev) = previous_response_id {
        payload["previous_response_id"] = Value::String(prev);
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
