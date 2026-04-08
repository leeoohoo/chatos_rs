use serde_json::{json, Value};

use crate::services::llm_prompt_runner::run_text_prompt_with_model_config;

pub async fn run_text_prompt(
    ai_model_config: Option<Value>,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    default_model: &str,
    purpose: &str,
) -> Result<String, String> {
    let model_cfg = ai_model_config.unwrap_or_else(|| json!({}));
    run_text_prompt_with_model_config(
        Some(model_cfg),
        system_prompt,
        user_prompt,
        max_tokens,
        default_model,
        purpose,
    )
    .await
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
