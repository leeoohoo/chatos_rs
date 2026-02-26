use std::sync::Arc;

use futures::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;
use crate::utils::model_config::is_gpt_provider;

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
        let send = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send();

        let resp = if let Some(token) = token {
            tokio::select! {
                _ = token.cancelled() => {
                    return Err("aborted".to_string());
                }
                res = send => res.map_err(|e| e.to_string())?
            }
        } else {
            send.await.map_err(|e| e.to_string())?
        };

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
                let mut metadata = serde_json::Map::new();
                if let Some(id) = response_id.clone() {
                    metadata.insert("response_id".to_string(), Value::String(id));
                }
                if let Some(tc) = tool_calls.clone() {
                    metadata.insert("toolCalls".to_string(), tc);
                }
                let meta_val = if metadata.is_empty() {
                    None
                } else {
                    Some(Value::Object(metadata))
                };
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
        let send = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send();

        let resp = if let Some(token) = token.clone() {
            tokio::select! {
                _ = token.cancelled() => {
                    return Err("aborted".to_string());
                }
                res = send => res.map_err(|e| e.to_string())?
            }
        } else {
            send.await.map_err(|e| e.to_string())?
        };

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

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut full_content = String::new();
        let mut reasoning = String::new();
        let mut usage: Option<Value> = None;
        let mut response_obj: Option<Value> = None;
        let mut response_id: Option<String> = None;
        let mut finish_reason: Option<String> = None;
        let mut sent_any_chunk = false;

        while let Some(chunk) = stream.next().await {
            if let Some(token) = token.clone() {
                if token.is_cancelled() {
                    return Err("aborted".to_string());
                }
            }
            let bytes = chunk.map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            buffer.push_str(&text);
            while let Some(idx) = buffer.find("\n\n") {
                let packet = buffer[..idx].to_string();
                buffer = buffer[idx + 2..].to_string();
                for line in packet.lines() {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        break;
                    }
                    let v: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if let Some(t) = v.get("type").and_then(|v| v.as_str()) {
                        if t == "response.output_text.delta" {
                            if let Some(delta_val) = v.get("delta") {
                                if let Some(delta) = extract_text_delta(delta_val) {
                                    if !delta.is_empty() {
                                        full_content.push_str(&delta);
                                        if let Some(cb) = &callbacks.on_chunk {
                                            cb(delta.clone());
                                        }
                                        sent_any_chunk = true;
                                    }
                                }
                            }
                        } else if t == "response.output_text.done"
                            || t == "response.output_text"
                            || t == "response.output_text.completed"
                        {
                            if full_content.is_empty() {
                                if let Some(text) =
                                    extract_text_from_fields(&v, &["text", "output_text", "delta"])
                                {
                                    if !text.is_empty() {
                                        full_content.push_str(&text);
                                        if let Some(cb) = &callbacks.on_chunk {
                                            cb(text);
                                        }
                                        sent_any_chunk = true;
                                    }
                                }
                            }
                        } else if let Some(reasoning_delta) = extract_reasoning_event_text(t, &v) {
                            if !reasoning_delta.is_empty() {
                                reasoning.push_str(&reasoning_delta);
                                if let Some(cb) = &callbacks.on_thinking {
                                    cb(reasoning_delta);
                                }
                            }
                        } else if t == "response.completed" {
                            if let Some(resp) = v.get("response") {
                                response_obj = Some(resp.clone());
                                if full_content.is_empty() {
                                    let extracted = extract_output_text(resp);
                                    if !extracted.is_empty() {
                                        full_content.push_str(&extracted);
                                        if let Some(cb) = &callbacks.on_chunk {
                                            cb(extracted);
                                        }
                                        sent_any_chunk = true;
                                    }
                                }
                            } else {
                                response_obj = Some(v.clone());
                                if full_content.is_empty() {
                                    let extracted = extract_output_text(&v);
                                    if !extracted.is_empty() {
                                        full_content.push_str(&extracted);
                                        if let Some(cb) = &callbacks.on_chunk {
                                            cb(extracted);
                                        }
                                        sent_any_chunk = true;
                                    }
                                }
                            }
                        } else if t == "response.failed" {
                            if let Some(resp) = v.get("response") {
                                response_obj = Some(resp.clone());
                            }
                        } else if response_obj.is_none() {
                            if let Some(resp) = v.get("response") {
                                if resp.get("output").is_some()
                                    || resp.get("output_text").is_some()
                                    || resp.get("status").is_some()
                                {
                                    response_obj = Some(resp.clone());
                                }
                            } else if v.get("output").is_some() || v.get("output_text").is_some() {
                                response_obj = Some(v.clone());
                            }
                        }
                    }
                    if let Some(id) = v
                        .get("response")
                        .and_then(|r| r.get("id"))
                        .and_then(|v| v.as_str())
                    {
                        response_id = Some(id.to_string());
                    } else if response_id.is_none() {
                        if let Some(id) = v.get("id").and_then(|v| v.as_str()) {
                            if looks_like_response_id(id) {
                                response_id = Some(id.to_string());
                            }
                        }
                    }
                    if let Some(u) = v.get("response").and_then(|r| r.get("usage")) {
                        usage = Some(u.clone());
                    }
                }
            }
        }

        let response_val = response_obj.unwrap_or_else(|| json!({ "output_text": full_content }));
        let tool_calls = extract_tool_calls(&response_val);
        let content = if !full_content.is_empty() {
            full_content.clone()
        } else {
            extract_output_text(&response_val)
        };
        if !sent_any_chunk {
            if let Some(cb) = &callbacks.on_chunk {
                if !content.is_empty() {
                    cb(content.clone());
                }
            }
        }
        let reasoning_opt = if reasoning.is_empty() {
            let fallback = extract_reasoning_from_response(&response_val);
            if fallback.is_empty() {
                None
            } else {
                Some(fallback)
            }
        } else {
            Some(reasoning.clone())
        };
        if finish_reason.is_none() {
            finish_reason = response_val
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if response_id.is_none() {
            response_id = response_val
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if usage.is_none() {
            usage = response_val.get("usage").cloned();
        }

        if persist_messages {
            if let Some(session_id) = session_id.clone() {
                let mut metadata = serde_json::Map::new();
                if let Some(id) = response_id.clone() {
                    metadata.insert("response_id".to_string(), Value::String(id));
                }
                if let Some(tc) = tool_calls.clone() {
                    metadata.insert("toolCalls".to_string(), tc);
                }
                let meta_val = if metadata.is_empty() {
                    None
                } else {
                    Some(Value::Object(metadata))
                };
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
            finish_reason,
            usage,
            response_id,
        })
    }
}

fn normalize_reasoning_effort(provider: Option<&str>, level: Option<&str>) -> Option<String> {
    let provider = provider.unwrap_or("gpt");
    let lvl = match crate::utils::model_config::normalize_thinking_level(provider, level) {
        Ok(v) => v,
        Err(_) => None,
    };
    lvl
}

fn extract_tool_calls(response: &Value) -> Option<Value> {
    let mut tool_calls: Vec<Value> = Vec::new();
    if let Some(arr) = response.get("output").and_then(|v| v.as_array()) {
        for item in arr {
            if item.get("type").and_then(|v| v.as_str()) != Some("function_call") {
                continue;
            }
            let call_id = item
                .get("call_id")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("id").and_then(|v| v.as_str()))
                .unwrap_or("");
            if call_id.is_empty() {
                continue;
            }
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = item
                .get("arguments")
                .cloned()
                .unwrap_or(Value::String("{}".to_string()));
            let args_str = if let Some(s) = args.as_str() {
                s.to_string()
            } else {
                args.to_string()
            };
            tool_calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": args_str
                }
            }));
        }
    }
    if tool_calls.is_empty() {
        None
    } else {
        Some(Value::Array(tool_calls))
    }
}

fn extract_output_text(response: &Value) -> String {
    if let Some(s) = response.get("output_text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = response.get("text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(arr) = response.get("output").and_then(|v| v.as_array()) {
        let mut text = String::new();
        for item in arr {
            if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            if let Some(content) = item.get("content").and_then(|v| v.as_str()) {
                text.push_str(content);
                continue;
            }
            if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                for part in parts {
                    let ptype = part.get("type").and_then(|v| v.as_str());
                    if ptype == Some("output_text") || ptype == Some("text") {
                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
        return text;
    }
    String::new()
}

fn extract_text_delta(delta: &Value) -> Option<String> {
    if let Some(s) = delta.as_str() {
        return Some(s.to_string());
    }
    if let Some(s) = delta.get("text").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = delta.get("content").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn extract_text_from_fields(value: &Value, fields: &[&str]) -> Option<String> {
    for key in fields {
        if let Some(v) = value.get(*key) {
            if let Some(text) = extract_text_delta(v) {
                return Some(text);
            }
        }
    }
    None
}

fn extract_reasoning_event_text(event_type: &str, event: &Value) -> Option<String> {
    let is_reasoning_event = event_type.starts_with("response.reasoning")
        || event_type.starts_with("response.reasoning_text")
        || event_type.starts_with("response.reasoning_summary");

    if is_reasoning_event {
        for key in [
            "delta",
            "summary_text",
            "summary",
            "text",
            "part",
            "item",
            "content",
        ] {
            if let Some(value) = event.get(key) {
                let text = normalize_reasoning_delta(Some(value));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }

        if let Some(response) = event.get("response") {
            let text = extract_reasoning_from_response(response);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    if event_type == "response.output_item.added"
        || event_type == "response.output_item.delta"
        || event_type == "response.output_item.done"
    {
        let item = event.get("item").or_else(|| event.get("output_item"));
        if let Some(item) = item {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if item_type == "reasoning" || item_type == "reasoning_summary" {
                let text = extract_reasoning_from_response(&json!({ "output": [item.clone()] }));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

fn extract_reasoning_from_response(response: &Value) -> String {
    if let Some(reasoning) = response
        .get("reasoning")
        .or_else(|| response.get("reasoning_summary"))
    {
        let text = normalize_reasoning_delta(Some(reasoning));
        if !text.is_empty() {
            return text;
        }
    }

    let mut parts = Vec::new();
    if let Some(output_items) = response.get("output").and_then(|v| v.as_array()) {
        for item in output_items {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if item_type != "reasoning" && item_type != "reasoning_summary" {
                continue;
            }

            let mut item_has_text = false;
            for key in ["summary", "content", "text", "delta", "reasoning"] {
                if let Some(value) = item.get(key) {
                    let text = normalize_reasoning_delta(Some(value));
                    if !text.is_empty() {
                        parts.push(text);
                        item_has_text = true;
                    }
                }
            }

            if !item_has_text {
                let text = normalize_reasoning_delta(Some(item));
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }
    }

    parts.join("")
}

fn extract_reasoning_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }

    if let Some(arr) = value.as_array() {
        let mut out = String::new();
        for item in arr {
            let text = extract_reasoning_text(item);
            if !text.is_empty() {
                out.push_str(&text);
            }
        }
        return out;
    }

    let Some(obj) = value.as_object() else {
        return String::new();
    };

    for key in [
        "text",
        "summary_text",
        "delta",
        "content",
        "summary",
        "reasoning",
        "reasoning_text",
        "value",
        "part",
        "item",
    ] {
        if let Some(inner) = obj.get(key) {
            let text = extract_reasoning_text(inner);
            if !text.is_empty() {
                return text;
            }
        }
    }

    if let Some(parts) = obj.get("content").and_then(|v| v.as_array()) {
        let mut out = String::new();
        for part in parts {
            let text = extract_reasoning_text(part);
            if !text.is_empty() {
                out.push_str(&text);
            }
        }
        return out;
    }

    String::new()
}

fn normalize_reasoning_delta(delta: Option<&Value>) -> String {
    delta.map(extract_reasoning_text).unwrap_or_default()
}

fn looks_like_response_id(id: &str) -> bool {
    let normalized = id.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if normalized.starts_with("event_") || normalized.starts_with("call_") {
        return false;
    }
    if normalized.starts_with("resp_")
        || normalized.starts_with("response_")
        || normalized.starts_with("chatcmpl-")
        || normalized.starts_with("cmpl-")
    {
        return true;
    }
    normalized.len() >= 16
}

fn truncate_log(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let mut out = value[..max_len].to_string();
    out.push_str("...[truncated]");
    out
}
