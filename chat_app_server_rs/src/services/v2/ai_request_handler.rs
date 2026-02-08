use std::sync::Arc;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tracing::{info, error};
use futures::StreamExt;

use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

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

        if let Some(level) = normalize_reasoning_effort(provider.as_deref(), thinking_level.as_deref()) {
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

        info!("[AI] handleRequest start: purpose={}, model={}, stream={}, baseURL={}, session={}", purpose, payload["model"].as_str().unwrap_or(""), stream, self.base_url, session_id.clone().unwrap_or_else(|| "n/a".to_string()));

        if stream {
            self.handle_stream_request(url, payload, callbacks, reasoning_enabled, session_id, token).await
        } else {
            self.handle_normal_request(url, payload, reasoning_enabled, session_id, token).await
        }
    }

    async fn handle_normal_request(
        &self,
        url: String,
        payload: Value,
        reasoning_enabled: bool,
        session_id: Option<String>,
        token: Option<CancellationToken>,
    ) -> Result<AiResponse, String> {
        let send = self.client.post(&url)
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
        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let err_text = truncate_log(&val.to_string(), 2000);
            error!("[AI] request failed: status={}, error={}", status, err_text);
            return Err(val.to_string());
        }
        let choice = val.get("choices").and_then(|c| c.get(0)).cloned().unwrap_or(Value::Null);
        let message = choice.get("message").cloned().unwrap_or(Value::Null);
        let content = message.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut reasoning = None;
        if reasoning_enabled {
            let r = normalize_reasoning_value(message.get("reasoning_content").or_else(|| message.get("reasoning")));
            if !r.is_empty() {
                reasoning = Some(r);
            }
        }
        let tool_calls = message.get("tool_calls").cloned();
        let finish_reason = choice.get("finish_reason").and_then(|v| v.as_str()).map(|s| s.to_string());
        let usage = val.get("usage").cloned();

        if let Some(session_id) = session_id {
            let mut metadata = serde_json::Map::new();
            if let Some(tc) = tool_calls.clone() {
                metadata.insert("toolCalls".to_string(), tc);
            }
            let meta_val = if metadata.is_empty() { None } else { Some(Value::Object(metadata)) };
            let _ = self.message_manager.save_assistant_message(&session_id, &content, None, reasoning.clone(), meta_val, tool_calls.clone()).await;
        }

        Ok(AiResponse { content, reasoning, tool_calls, finish_reason, usage })
    }

    async fn handle_stream_request(
        &self,
        url: String,
        payload: Value,
        callbacks: StreamCallbacks,
        reasoning_enabled: bool,
        session_id: Option<String>,
        token: Option<CancellationToken>,
    ) -> Result<AiResponse, String> {
        let send = self.client.post(&url)
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
            error!("[AI] stream request failed: status={}, error={}", status, err_text);
            return Err(text);
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut full_content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls_map: std::collections::BTreeMap<usize, Value> = std::collections::BTreeMap::new();
        let mut finish_reason: Option<String> = None;
        let mut usage: Option<Value> = None;

        while let Some(chunk) = stream.next().await {
            if let Some(token) = token.clone() {
                if token.is_cancelled() { return Err("aborted".to_string()); }
            }
            let bytes = chunk.map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            buffer.push_str(&text);
            while let Some(idx) = buffer.find("\n\n") {
                let packet = buffer[..idx].to_string();
                buffer = buffer[idx+2..].to_string();
                for line in packet.lines() {
                    let line = line.trim();
                    if !line.starts_with("data:") { continue; }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" { break; }
                    let v: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if let Some(u) = v.get("usage") { usage = Some(u.clone()); }
                    let choice = v.get("choices").and_then(|c| c.get(0));
                    if let Some(choice) = choice {
                        if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) { finish_reason = Some(fr.to_string()); }
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                full_content.push_str(content);
                                if let Some(cb) = &callbacks.on_chunk { cb(content.to_string()); }
                            }
                            if reasoning_enabled {
                                let r = normalize_reasoning_value(delta.get("reasoning_content").or_else(|| delta.get("reasoning")));
                                if !r.is_empty() {
                                    reasoning.push_str(&r);
                                    if let Some(cb) = &callbacks.on_thinking { cb(r); }
                                }
                            }
                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                                for tc in tool_calls {
                                    let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                    let entry = tool_calls_map.entry(index).or_insert(json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
                                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) { entry["id"] = Value::String(id.to_string()); }
                                    if let Some(func) = tc.get("function") {
                                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                            let cur = entry["function"]["name"].as_str().unwrap_or("").to_string();
                                            entry["function"]["name"] = Value::String(format!("{}{}", cur, name));
                                        }
                                        if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                            let cur = entry["function"]["arguments"].as_str().unwrap_or("").to_string();
                                            entry["function"]["arguments"] = Value::String(format!("{}{}", cur, args));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let tool_calls = if tool_calls_map.is_empty() {
            None
        } else {
            Some(Value::Array(tool_calls_map.values().cloned().collect()))
        };
        let reasoning_opt = if reasoning.is_empty() { None } else { Some(reasoning.clone()) };

        if let Some(session_id) = session_id {
            let mut metadata = serde_json::Map::new();
            if let Some(tc) = tool_calls.clone() {
                metadata.insert("toolCalls".to_string(), tc);
            }
            let meta_val = if metadata.is_empty() { None } else { Some(Value::Object(metadata)) };
            let _ = self.message_manager.save_assistant_message(&session_id, &full_content, None, reasoning_opt.clone(), meta_val, tool_calls.clone()).await;
        }

        Ok(AiResponse { content: full_content, reasoning: reasoning_opt, tool_calls, finish_reason, usage })
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

fn normalize_reasoning_value(value: Option<&Value>) -> String {
    if let Some(v) = value {
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        if v.is_null() {
            return String::new();
        }
        if let Ok(s) = serde_json::to_string(v) {
            return s;
        }
        return v.to_string();
    }
    String::new()
}

fn truncate_log(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let mut out = value[..max_len].to_string();
    out.push_str("...[truncated]");
    out
}

