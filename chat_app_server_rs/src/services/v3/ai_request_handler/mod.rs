use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::services::ai_common::{
    await_with_optional_abort, build_assistant_message_metadata, consume_sse_stream,
    normalize_reasoning_effort, truncate_log,
};
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;
use crate::utils::model_config::is_gpt_provider;

mod parser;

use self::parser::{
    apply_stream_event, extract_output_text, extract_reasoning_from_response, extract_tool_calls,
    StreamState,
};

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
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
        temperature: Option<f64>,
        max_output_tokens: Option<i64>,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        stream: bool,
        purpose: &str,
    ) -> Result<AiResponse, String> {
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
        if let Some(t) = temperature {
            payload["temperature"] = json!(t);
        }
        if let Some(max) = max_output_tokens {
            payload["max_output_tokens"] = json!(max);
        }
        if let Some(level) =
            normalize_reasoning_effort(provider.as_deref(), thinking_level.as_deref())
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

        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let token = if let Some(session_id) = session_id.as_ref() {
            let token = CancellationToken::new();
            abort_registry::set_controller(session_id, token.clone());
            Some(token)
        } else {
            None
        };

        info!(
            "[AI_V3] handleRequest start: purpose={}, model={}, stream={}, baseURL={}, session={}",
            purpose,
            payload.get("model").and_then(|v| v.as_str()).unwrap_or(""),
            stream,
            self.base_url,
            session_id.clone().unwrap_or_else(|| "n/a".to_string())
        );

        let persist_messages = purpose != "sub_agent_router";

        if stream {
            self.handle_stream_request(url, payload, callbacks, session_id, token, persist_messages)
                .await
        } else {
            self.handle_normal_request(url, payload, session_id, token, persist_messages)
                .await
        }
    }

    async fn handle_normal_request(
        &self,
        url: String,
        payload: Value,
        session_id: Option<String>,
        token: Option<CancellationToken>,
        persist_messages: bool,
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
            error!(
                "[AI_V3] request failed: status={}, error={}",
                status, err_text
            );
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

        let tool_calls = extract_tool_calls(&val);
        let content = extract_output_text(&val);
        let usage = val.get("usage").cloned();
        let finish_reason = val
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let response_id = val
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if persist_messages {
            if let Some(session_id) = session_id.clone() {
                let meta_val =
                    build_assistant_message_metadata(tool_calls.as_ref(), response_id.as_deref());
                let reasoning = None;
                let _ = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &content,
                        None,
                        reasoning,
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await;
            }
        }

        Ok(AiResponse {
            content,
            reasoning: None,
            tool_calls,
            finish_reason,
            usage,
            response_id,
        })
    }

    async fn handle_stream_request(
        &self,
        url: String,
        payload: Value,
        callbacks: StreamCallbacks,
        session_id: Option<String>,
        token: Option<CancellationToken>,
        persist_messages: bool,
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
                "[AI_V3] stream request failed: status={}, error={}",
                status, err_text
            );
            return Err(text);
        }

        let stream = resp.bytes_stream();
        let mut stream_state = StreamState::default();

        consume_sse_stream(stream, token.clone(), |v| {
            let payload = apply_stream_event(&mut stream_state, &v);
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

        let response_val = stream_state
            .response_obj
            .clone()
            .unwrap_or_else(|| json!({ "output_text": stream_state.full_content }));
        let tool_calls = extract_tool_calls(&response_val);
        let content = if !stream_state.full_content.is_empty() {
            stream_state.full_content.clone()
        } else {
            extract_output_text(&response_val)
        };
        if !stream_state.sent_any_chunk {
            if let Some(cb) = &callbacks.on_chunk {
                if !content.is_empty() {
                    cb(content.clone());
                }
            }
        }
        let reasoning_opt = if stream_state.reasoning.is_empty() {
            let fallback = extract_reasoning_from_response(&response_val);
            if fallback.is_empty() {
                None
            } else {
                Some(fallback)
            }
        } else {
            Some(stream_state.reasoning.clone())
        };
        if stream_state.finish_reason.is_none() {
            stream_state.finish_reason = response_val
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if stream_state.response_id.is_none() {
            stream_state.response_id = response_val
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if stream_state.usage.is_none() {
            stream_state.usage = response_val.get("usage").cloned();
        }

        if persist_messages {
            if let Some(session_id) = session_id.clone() {
                let meta_val = build_assistant_message_metadata(
                    tool_calls.as_ref(),
                    stream_state.response_id.as_deref(),
                );
                let _ = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &content,
                        None,
                        reasoning_opt.clone(),
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await;
            }
        }

        Ok(AiResponse {
            content,
            reasoning: reasoning_opt,
            tool_calls,
            finish_reason: stream_state.finish_reason,
            usage: stream_state.usage,
            response_id: stream_state.response_id,
        })
    }
}
