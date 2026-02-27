use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::services::ai_common::{
    await_with_optional_abort, build_assistant_message_metadata, consume_sse_stream,
    normalize_reasoning_effort, truncate_log,
};
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

mod parser;

use self::parser::{
    apply_stream_event, collect_tool_calls, normalize_reasoning_value, StreamState,
};

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
}

#[derive(Clone)]
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
        stream: bool,
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

        if stream {
            payload["stream"] = Value::Bool(true);
            payload["stream_options"] = json!({"include_usage": true});
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let token = if let Some(session_id) = session_id.as_ref() {
            let token = CancellationToken::new();
            abort_registry::set_controller(session_id, token.clone());
            Some(token)
        } else {
            None
        };

        info!(
            "[AI] handleRequest start: purpose={}, model={}, stream={}, baseURL={}, session={}",
            purpose,
            payload["model"].as_str().unwrap_or(""),
            stream,
            self.base_url,
            session_id.clone().unwrap_or_else(|| "n/a".to_string())
        );

        let persist_messages = purpose != "sub_agent_router";

        if stream {
            self.handle_stream_request(
                url,
                payload,
                callbacks,
                reasoning_enabled,
                session_id,
                token,
                persist_messages,
                message_mode,
                message_source,
            )
            .await
        } else {
            self.handle_normal_request(
                url,
                payload,
                reasoning_enabled,
                session_id,
                token,
                persist_messages,
                message_mode,
                message_source,
            )
            .await
        }
    }

    async fn handle_normal_request(
        &self,
        url: String,
        payload: Value,
        reasoning_enabled: bool,
        session_id: Option<String>,
        token: Option<CancellationToken>,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
    ) -> Result<AiResponse, String> {
        let resp = await_with_optional_abort(
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&payload)
                .send(),
            token,
        )
        .await?;

        let status = resp.status();
        let raw = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let err_text = truncate_log(&raw, 2000);
            error!("[AI] request failed: status={}, error={}", status, err_text);
            return Err(format!("status {}: {}", status, err_text));
        }

        let val: Value = serde_json::from_str(raw.as_str()).map_err(|err| {
            format!(
                "invalid JSON response (status {}): {}; body_preview={}",
                status,
                err,
                truncate_log(raw.as_str(), 1200)
            )
        })?;

        let choice = val
            .get("choices")
            .and_then(|c| c.get(0))
            .cloned()
            .unwrap_or(Value::Null);
        let message = choice.get("message").cloned().unwrap_or(Value::Null);
        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut reasoning = None;
        if reasoning_enabled {
            let r = normalize_reasoning_value(
                message
                    .get("reasoning_content")
                    .or_else(|| message.get("reasoning")),
            );
            if !r.is_empty() {
                reasoning = Some(r);
            }
        }
        let tool_calls = message.get("tool_calls").cloned();
        let finish_reason = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let usage = val.get("usage").cloned();

        if persist_messages {
            if let Some(session_id) = session_id {
                let meta_val = build_assistant_message_metadata(tool_calls.as_ref(), None);
                let _ = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &content,
                        None,
                        reasoning.clone(),
                        message_mode,
                        message_source,
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await;
            }
        }

        Ok(AiResponse {
            content,
            reasoning,
            tool_calls,
            finish_reason,
            usage,
        })
    }

    async fn handle_stream_request(
        &self,
        url: String,
        payload: Value,
        callbacks: StreamCallbacks,
        reasoning_enabled: bool,
        session_id: Option<String>,
        token: Option<CancellationToken>,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
    ) -> Result<AiResponse, String> {
        let resp = await_with_optional_abort(
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&payload)
                .send(),
            token.clone(),
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let err_text = truncate_log(&text, 2000);
            error!(
                "[AI] stream request failed: status={}, error={}",
                status, err_text
            );
            return Err(text);
        }

        let stream = resp.bytes_stream();
        let mut stream_state = StreamState::default();

        consume_sse_stream(stream, token.clone(), |v| {
            let payload = apply_stream_event(&mut stream_state, &v, reasoning_enabled);
            if let Some(chunk) = payload.chunk {
                if let Some(cb) = &callbacks.on_chunk {
                    cb(chunk);
                }
            }
            if let Some(thinking) = payload.thinking {
                if let Some(cb) = &callbacks.on_thinking {
                    cb(thinking);
                }
            }
        })
        .await?;

        let tool_calls = collect_tool_calls(&stream_state.tool_calls_map);
        let reasoning_opt = if stream_state.reasoning.is_empty() {
            None
        } else {
            Some(stream_state.reasoning.clone())
        };

        if persist_messages {
            if let Some(session_id) = session_id {
                let meta_val = build_assistant_message_metadata(tool_calls.as_ref(), None);
                let _ = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &stream_state.full_content,
                        None,
                        reasoning_opt.clone(),
                        message_mode,
                        message_source,
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await;
            }
        }

        Ok(AiResponse {
            content: stream_state.full_content,
            reasoning: reasoning_opt,
            tool_calls,
            finish_reason: stream_state.finish_reason,
            usage: stream_state.usage,
        })
    }
}
