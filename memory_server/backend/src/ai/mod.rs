use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::models::{AiModelConfig, DEFAULT_SUMMARY_PROMPT_TEMPLATE};

#[derive(Clone)]
pub struct AiClient {
    http: Client,
    default_api_key: Option<String>,
    default_base_url: String,
    default_model: String,
    default_temperature: f64,
    request_timeout_secs: u64,
    allow_local_fallback: bool,
}

impl AiClient {
    pub fn new(timeout_secs: u64, config: &AppConfig) -> Result<Self, String> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            default_api_key: config.openai_api_key.clone(),
            default_base_url: normalize_base_url(config.openai_base_url.as_str()),
            default_model: normalize_model_name(config.openai_model.as_str()),
            default_temperature: config.openai_temperature.clamp(0.0, 2.0),
            request_timeout_secs: timeout_secs,
            allow_local_fallback: config.allow_local_summary_fallback,
        })
    }

    pub async fn summarize(
        &self,
        model_cfg: Option<&AiModelConfig>,
        target_tokens: i64,
        prompt_title: &str,
        chunks: &[String],
        summary_prompt: Option<&str>,
    ) -> Result<String, String> {
        if chunks.is_empty() {
            return Err("empty summarize input".to_string());
        }

        let max_tokens = target_tokens.max(128).min(4000);

        let context = chunks
            .iter()
            .enumerate()
            .map(|(idx, c)| format!("[{}]\n{}", idx + 1, c))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system_prompt = build_summary_system_prompt(summary_prompt, max_tokens, prompt_title);
        let user_prompt = format!(
            "任务：{}\n\n请基于以下内容生成高质量总结：\n\n{}",
            prompt_title, context
        );

        if let Some(cfg) = model_cfg.filter(|cfg| cfg.enabled == 1) {
            let api_key = cfg
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .or(self.default_api_key.as_deref());

            let model_name = normalize_model_name(cfg.model.as_str());
            let base_url = cfg
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(normalize_base_url)
                .unwrap_or_else(|| self.default_base_url.clone());
            let temperature = cfg
                .temperature
                .unwrap_or(self.default_temperature)
                .clamp(0.0, 2.0);
            let supports_responses = cfg.supports_responses == 1;

            if let Some(api_key) = api_key {
                return self
                    .call_openai_compatible(
                        base_url.as_str(),
                        model_name.as_str(),
                        temperature,
                        supports_responses,
                        api_key,
                        &system_prompt,
                        &user_prompt,
                        max_tokens,
                    )
                    .await;
            }
        }

        if let Some(api_key) = self.default_api_key.as_deref() {
            return self
                .call_openai_compatible(
                    self.default_base_url.as_str(),
                    self.default_model.as_str(),
                    self.default_temperature,
                    false,
                    api_key,
                    &system_prompt,
                    &user_prompt,
                    max_tokens,
                )
                .await;
        }

        if self.allow_local_fallback {
            return Ok(local_fallback_summary(chunks, max_tokens as usize));
        }

        Err(
            "no available AI credentials: set model api_key in model config or MEMORY_SERVER_OPENAI_API_KEY"
                .to_string(),
        )
    }

    async fn call_openai_compatible(
        &self,
        base_url: &str,
        model: &str,
        temperature: f64,
        supports_responses: bool,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: i64,
    ) -> Result<String, String> {
        if supports_responses {
            return self
                .call_responses_api(
                    base_url,
                    model,
                    temperature,
                    api_key,
                    system_prompt,
                    user_prompt,
                    max_tokens,
                )
                .await;
        }

        self.call_chat_completions_api(
            base_url,
            model,
            temperature,
            api_key,
            system_prompt,
            user_prompt,
            max_tokens,
        )
        .await
    }

    async fn call_chat_completions_api(
        &self,
        base_url: &str,
        model: &str,
        temperature: f64,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: i64,
    ) -> Result<String, String> {
        let endpoint = build_chat_completion_endpoint(base_url);

        let body = json!({
            "model": model,
            "temperature": temperature.clamp(0.0, 2.0),
            "max_tokens": max_tokens,
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": [
                {"role":"system","content": system_prompt},
                {"role":"user","content": user_prompt}
            ]
        });

        let mut req = self
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(timeout) = request_timeout_for_base_url(base_url, self.request_timeout_secs) {
            req = req.timeout(timeout);
        }
        let resp = req.send().await
            .map_err(|e| format!("ai request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("ai request status={} body={}", status, text));
        }

        let events = read_sse_json_events(resp).await?;
        let text = extract_chat_text_from_stream(events.as_slice())
            .ok_or_else(|| "ai empty content".to_string())?;

        Ok(text)
    }

    async fn call_responses_api(
        &self,
        base_url: &str,
        model: &str,
        temperature: f64,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: i64,
    ) -> Result<String, String> {
        let endpoint = build_responses_endpoint(base_url);

        let body = json!({
            "model": model,
            "temperature": temperature.clamp(0.0, 2.0),
            "max_output_tokens": max_tokens,
            "stream": true,
            "input": [
                {
                    "type": "message",
                    "role": "system",
                    "content": [{"type": "input_text", "text": system_prompt}]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": user_prompt}]
                }
            ]
        });

        let mut req = self
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(timeout) = request_timeout_for_base_url(base_url, self.request_timeout_secs) {
            req = req.timeout(timeout);
        }
        let resp = req.send().await
            .map_err(|e| format!("ai request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("ai request status={} body={}", status, text));
        }

        let events = read_sse_json_events(resp).await?;
        let value = aggregate_responses_stream(events.as_slice())?;

        extract_responses_output_text(&value).ok_or_else(|| "ai empty content".to_string())
    }
}

async fn read_sse_json_events(mut response: reqwest::Response) -> Result<Vec<Value>, String> {
    let mut buffer = String::new();
    let mut events: Vec<Value> = Vec::new();

    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|err| format!("ai stream read failed: {err}"))?
    {
        let text = String::from_utf8_lossy(&bytes).to_string();
        buffer.push_str(text.as_str());
        events.extend(drain_sse_json_events(&mut buffer));
    }

    flush_sse_tail_events(&mut buffer, &mut events);

    if events.is_empty() {
        return Err("ai stream parse failed: no JSON events found".to_string());
    }

    Ok(events)
}

fn drain_sse_json_events(buffer: &mut String) -> Vec<Value> {
    let mut events = Vec::new();

    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();

        for line in packet.lines() {
            let normalized = line.trim();
            if !normalized.starts_with("data:") {
                continue;
            }

            let data = normalized.trim_start_matches("data:").trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                events.push(value);
            }
        }
    }

    events
}

fn flush_sse_tail_events(buffer: &mut String, events: &mut Vec<Value>) {
    if buffer.trim().is_empty() {
        return;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        events.extend(drain_sse_json_events(buffer));
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return;
    }
    if let Ok(value) = serde_json::from_str::<Value>(tail) {
        emit_tail_json_value(value, events);
    }
    buffer.clear();
}

fn emit_tail_json_value(value: Value, events: &mut Vec<Value>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if item.is_object() {
                events.push(item.clone());
            }
        }
        return;
    }
    if value.is_object() {
        events.push(value);
    }
}

fn extract_chat_text_from_stream(events: &[Value]) -> Option<String> {
    let mut text = String::new();

    for event in events {
        if let Some(choices) = event.get("choices").and_then(Value::as_array) {
            for choice in choices {
                if let Some(delta) = choice.get("delta") {
                    if let Some(content) = delta.get("content").and_then(Value::as_str) {
                        text.push_str(content);
                    } else if let Some(parts) = delta.get("content").and_then(Value::as_array) {
                        for part in parts {
                            if let Some(piece) = part.get("text").and_then(Value::as_str) {
                                text.push_str(piece);
                            }
                        }
                    }
                }

                if let Some(message) = choice.get("message") {
                    if let Some(content) = message.get("content").and_then(Value::as_str) {
                        if text.trim().is_empty() {
                            text = content.to_string();
                        }
                    }
                }
            }
        }
    }

    let normalized = text.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn aggregate_responses_stream(events: &[Value]) -> Result<Value, String> {
    let mut completed_response: Option<Value> = None;
    let mut response_template: Option<Value> = None;
    let mut output_items: Vec<Value> = Vec::new();
    let mut output_text = String::new();

    for event in events {
        if event.get("object").and_then(Value::as_str) == Some("response") {
            completed_response = Some(event.clone());
        }
        if let Some(response) = event.get("response") {
            response_template = Some(response.clone());
            let event_type = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if event_type == "response.completed" || event_type == "response.failed" {
                completed_response = Some(response.clone());
            }
        }

        let event_type = event.get("type").and_then(Value::as_str).unwrap_or_default();
        if event_type == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                output_text.push_str(delta);
            }
        } else if event_type == "response.output_item.done" {
            if let Some(item) = event.get("item") {
                output_items.push(item.clone());
            }
        }
    }

    if let Some(value) = completed_response {
        return Ok(value);
    }

    let mut response = response_template
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if output_items.is_empty() && !output_text.trim().is_empty() {
        output_items.push(json!({
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{"type": "output_text", "text": output_text.clone()}]
        }));
    }

    if !output_items.is_empty() {
        response.insert("output".to_string(), Value::Array(output_items));
    }
    if !output_text.trim().is_empty() {
        response.insert("output_text".to_string(), Value::String(output_text));
    }
    if !response.contains_key("status") {
        response.insert("status".to_string(), Value::String("completed".to_string()));
    }
    if !response.contains_key("object") {
        response.insert("object".to_string(), Value::String("response".to_string()));
    }

    if response.is_empty() {
        return Err("ai stream parse failed: no response payload assembled".to_string());
    }

    Ok(Value::Object(response))
}

fn build_summary_system_prompt(
    summary_prompt: Option<&str>,
    target_tokens: i64,
    prompt_title: &str,
) -> String {
    let template = summary_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SUMMARY_PROMPT_TEMPLATE);

    let target_tokens_str = target_tokens.to_string();
    template
        .replace("{{target_tokens}}", target_tokens_str.as_str())
        .replace("{{prompt_title}}", prompt_title)
}

fn normalize_base_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        normalized.to_string()
    }
}

fn normalize_model_name(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        "gpt-4o-mini".to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else {
        format!("{}/chat/completions", normalized)
    }
}

fn build_responses_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/responses") {
        normalized
    } else {
        format!("{}/responses", normalized)
    }
}

fn is_local_gateway_base_url(base_url: &str) -> bool {
    let normalized = normalize_base_url(base_url);
    let Ok(parsed) = url::Url::parse(normalized.as_str()) else {
        return false;
    };

    let host = parsed
        .host_str()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        return false;
    }

    parsed.port_or_known_default() == Some(8089)
}

fn request_timeout_for_base_url(base_url: &str, timeout_secs: u64) -> Option<Duration> {
    if is_local_gateway_base_url(base_url) {
        None
    } else {
        Some(Duration::from_secs(timeout_secs))
    }
}

fn extract_responses_output_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(|v| v.as_str()) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let mut parts = Vec::new();
    let Some(items) = value.get("output").and_then(|v| v.as_array()) else {
        return None;
    };

    for item in items {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if item_type == "message" {
            if let Some(contents) = item.get("content").and_then(|v| v.as_array()) {
                for content in contents {
                    let content_type = content.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if content_type == "output_text"
                        || content_type == "input_text"
                        || content_type == "text"
                    {
                        if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                parts.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            continue;
        }

        if (item_type == "output_text" || item_type == "input_text" || item_type == "text")
            && item.get("text").and_then(|v| v.as_str()).is_some()
        {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn local_fallback_summary(chunks: &[String], max_tokens: usize) -> String {
    let mut lines = vec![
        "[fallback-summary] 未配置可用模型，使用本地降级摘要。".to_string(),
        "关键要点：".to_string(),
    ];

    for chunk in chunks.iter().take(8) {
        let short = chunk
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(260)
            .collect::<String>();
        lines.push(format!("- {}", short));
    }

    let mut out = lines.join("\n");
    let approx_chars = max_tokens.saturating_mul(4);
    if out.len() > approx_chars {
        out = out.chars().take(approx_chars).collect();
    }
    out
}
