// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::services) fn normalize_model_provider_input(
    provider: &str,
) -> Result<String, String> {
    let raw = provider.trim();
    if raw.is_empty() {
        return Err("provider 为必填项".to_string());
    }
    let normalized = normalize_provider(raw);
    let provider = match normalized.as_str() {
        "gpt" => "openai",
        "openai_compatible" => "openai_compatible",
        "deepseek" => "deepseek",
        "kimi" => "kimik2",
        "custom_gateway" => "openai",
        "kiminik2" => "kimik2",
        other => other,
    };
    match provider {
        "openai" | "openai_compatible" | "deepseek" | "kimik2" => Ok(provider.to_string()),
        _ => Err("provider 仅支持 openai / openai_compatible / deepseek / kimik2".to_string()),
    }
}

pub(in crate::services) fn normalize_model_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(level) = level else {
        return Ok(None);
    };
    normalize_thinking_level(provider, Some(level.as_str()))
        .map_err(|_| "思考等级仅支持 none/auto/minimal/low/medium/high/xhigh/max".to_string())
}

pub(in crate::services) fn normalize_model_prompt_vendor_input(
    prompt_vendor: Option<String>,
    provider: &str,
) -> Result<Option<String>, String> {
    if let Some(value) = normalized_optional(prompt_vendor) {
        return AgentPromptVendor::from_str(value.as_str())
            .map(|vendor| Some(vendor.as_str().to_string()))
            .map_err(|_| "prompt_vendor 仅支持 glm / deepseek / gpt / kimi".to_string());
    }
    Ok(normalize_agent_prompt_vendor(None, provider).map(|vendor| vendor.as_str().to_string()))
}

pub(in crate::services) fn normalize_model_base_url_input(
    provider: &str,
    base_url: Option<String>,
) -> String {
    base_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider, "https://api.openai.com/v1"))
        .trim_end_matches('/')
        .to_string()
}

pub(in crate::services) fn normalize_model_config_record(
    mut record: ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    let provider = normalize_model_provider_input(&record.provider)?;
    record.thinking_level =
        normalize_model_thinking_level_input(provider.as_str(), record.thinking_level.clone())?;
    record.prompt_vendor =
        normalize_model_prompt_vendor_input(record.prompt_vendor, provider.as_str())?;
    record.owner_user_id = normalized_optional(record.owner_user_id);
    record.owner_username = normalized_optional(record.owner_username);
    record.owner_display_name = normalized_optional(record.owner_display_name);
    record.base_url = normalize_model_base_url_input(provider.as_str(), Some(record.base_url));
    record.provider = provider;
    record.usage_scenario = normalized_optional(record.usage_scenario);
    record.instructions = normalized_optional(record.instructions);
    record.request_cwd = normalized_optional(record.request_cwd);
    Ok(record)
}
