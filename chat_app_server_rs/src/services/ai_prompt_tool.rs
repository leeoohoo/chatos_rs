use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::message_manager::MessageManager;

pub async fn run_text_prompt(
    ai_model_config: Option<Value>,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    default_model: &str,
    purpose: &str,
) -> Result<String, String> {
    let cfg = Config::get();
    let model_cfg = ai_model_config.unwrap_or_else(|| json!({}));
    let resolved = resolve_chat_model_config(
        &model_cfg,
        default_model,
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        Some(false),
        true,
    );

    if resolved.api_key.trim().is_empty() {
        return Err("未配置可用的 API Key".to_string());
    }
    if resolved.base_url.trim().is_empty() {
        return Err("未配置可用的 Base URL".to_string());
    }

    let handler = AiRequestHandler::new(
        resolved.api_key.clone(),
        resolved.base_url.clone(),
        MessageManager::new(),
    );

    let messages = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": user_prompt}),
    ];

    let response = handler
        .handle_request(
            messages,
            None,
            resolved.model.clone(),
            Some(resolved.temperature),
            max_tokens,
            StreamCallbacks {
                on_chunk: None,
                on_thinking: None,
            },
            false,
            Some(resolved.provider.clone()),
            resolved.thinking_level.clone(),
            None,
            false,
            purpose,
        )
        .await?;

    let content = response.content.trim().to_string();
    if content.is_empty() {
        return Err("AI 未返回文本内容".to_string());
    }

    Ok(content)
}

pub fn parse_json_loose(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }

    if let Some(stripped) = strip_markdown_fence(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(stripped.as_str()) {
            return Some(value);
        }
    }

    extract_bracket_json(trimmed, '{', '}').or_else(|| extract_bracket_json(trimmed, '[', ']'))
}

fn strip_markdown_fence(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return None;
    }
    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or_default();
    if !first_line.trim_start().starts_with("```") {
        return None;
    }

    let mut payload_lines = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("```") {
            break;
        }
        payload_lines.push(line);
    }

    let joined = payload_lines.join("\n");
    let candidate = joined.trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn extract_bracket_json(raw: &str, open: char, close: char) -> Option<Value> {
    let start = raw.find(open)?;
    let end = raw.rfind(close)?;
    if end <= start {
        return None;
    }
    let candidate = raw[start..=end].trim();
    serde_json::from_str::<Value>(candidate).ok()
}
