use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::models::ai_model_config::AiModelConfig;

pub(super) fn build_browser_vision_prompt(question: &str) -> String {
    format!(
        "你现在收到了一张当前网页截图。请仅基于截图内容回答用户问题，先给结论，再给1-3条关键依据。用户问题：{}",
        question
    )
}

pub(super) fn build_browser_vision_unavailable_message(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "browser_vision has no available vision-capable model configuration.".to_string()
    } else {
        format!(
            "browser_vision has no available vision-capable model configuration. {}",
            warnings
                .iter()
                .map(|item| normalize_inline_text(item.as_str(), 180))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    }
}

pub(super) fn json_value_is_empty_object(value: &Value) -> bool {
    value
        .as_object()
        .map(|items| items.is_empty())
        .unwrap_or(false)
}

pub(super) fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

pub(super) fn normalize_inline_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    normalized.chars().take(max_chars).collect::<String>() + "..."
}

pub(super) fn ai_model_config_to_runtime_value(model_cfg: &AiModelConfig) -> Value {
    json!({
        "id": model_cfg.id,
        "name": model_cfg.name,
        "provider": model_cfg.provider,
        "model_name": model_cfg.model,
        "thinking_level": model_cfg.thinking_level,
        "api_key": model_cfg.api_key,
        "base_url": model_cfg.base_url,
        "enabled": model_cfg.enabled,
        "supports_images": model_cfg.supports_images,
        "supports_reasoning": model_cfg.supports_reasoning,
        "supports_responses": model_cfg.supports_responses,
    })
}

pub(super) async fn build_browser_vision_image_data_url(
    screenshot_path: &str,
) -> Result<String, String> {
    let image_bytes = tokio::fs::read(screenshot_path)
        .await
        .map_err(|err| format!("read screenshot failed: {}", err))?;
    let mime = mime_guess::from_path(screenshot_path).first_or_octet_stream();
    Ok(format!(
        "data:{};base64,{}",
        mime.essence_str(),
        BASE64_STD.encode(image_bytes)
    ))
}
