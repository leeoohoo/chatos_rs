// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use chatos_plugin_management_sdk::{normalize_agent_prompt_vendor, AgentPromptVendor};
use sha2::{Digest, Sha256};
use std::str::FromStr;

use crate::models::{UserModelConfigRecord, UserModelProviderRecord};
use crate::secrets::is_secret_encrypted;

use super::super::bad_request;

pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn normalize_api_key_input(
    value: Option<String>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let value = normalize_optional_string(value);
    if value
        .as_deref()
        .is_some_and(|item| is_secret_encrypted(item.trim()))
    {
        return Err(bad_request(
            "api_key must be a plain provider token, not an encrypted secret",
        ));
    }
    Ok(value)
}

pub(super) fn model_config_id_for(
    owner_user_id: &str,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(owner_user_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(provider.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(base_url.unwrap_or_default().trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(model.trim().as_bytes());
    let digest = hasher.finalize();
    format!("model_{}", hex_prefix(&digest, 32))
}

fn hex_prefix(bytes: &[u8], max_chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
        if out.len() >= max_chars {
            out.truncate(max_chars);
            break;
        }
    }
    out
}

pub(super) fn normalized_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn model_config_belongs_to_provider(
    source_provider_id: Option<&str>,
    model_provider: &str,
    model_base_url: Option<&str>,
    provider_id: &str,
    provider: &str,
    provider_base_url: Option<&str>,
) -> bool {
    if let Some(source_provider_id) = source_provider_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return source_provider_id == provider_id.trim();
    }

    model_provider == provider
        && normalized_base_url(model_base_url) == normalized_base_url(provider_base_url)
}

pub(super) fn model_config_has_backing_provider(
    model: &UserModelConfigRecord,
    providers: &[UserModelProviderRecord],
) -> bool {
    providers.iter().any(|provider| {
        provider.owner_user_id == model.owner_user_id
            && model_config_belongs_to_provider(
                model.source_provider_id.as_deref(),
                model.provider.as_str(),
                model.base_url.as_deref(),
                provider.id.as_str(),
                provider.provider.as_str(),
                provider.base_url.as_deref(),
            )
    })
}

pub(super) fn normalize_provider_input(
    provider: Option<String>,
) -> Result<String, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let provider = provider
        .unwrap_or_else(|| "gpt".to_string())
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");
    match provider.as_str() {
        "openai" | "gpt" => Ok("gpt".to_string()),
        "deepseek" => Ok("deepseek".to_string()),
        "kimi" | "kimik2" | "moonshot" => Ok("kimi".to_string()),
        "glm" | "zhipu" | "zhipuai" | "zai" | "chatglm" => Ok("glm".to_string()),
        _ => Err(bad_request(
            "provider only supports gpt / deepseek / kimi / glm",
        )),
    }
}

pub(in crate::api) fn is_supported_provider(provider: &str) -> bool {
    matches!(
        provider
            .trim()
            .to_ascii_lowercase()
            .replace('-', "_")
            .as_str(),
        "openai"
            | "gpt"
            | "deepseek"
            | "kimi"
            | "kimik2"
            | "moonshot"
            | "glm"
            | "zhipu"
            | "zhipuai"
            | "zai"
            | "chatglm"
    )
}

pub(super) fn normalize_prompt_vendor_input(
    prompt_vendor: Option<String>,
    provider: &str,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    if let Some(value) = normalize_optional_string(prompt_vendor) {
        return AgentPromptVendor::from_str(value.as_str())
            .map(|vendor| Some(vendor.as_str().to_string()))
            .map_err(|_| bad_request("prompt_vendor only supports glm/deepseek/gpt/kimi"));
    }
    Ok(normalize_agent_prompt_vendor(None, provider).map(|vendor| vendor.as_str().to_string()))
}

pub(super) fn normalize_thinking_level_input(
    provider: &str,
    value: Option<&str>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let provider = match provider
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".to_string(),
        "kimik2" | "kimi" | "moonshot" => "kimi".to_string(),
        other => other.to_string(),
    };
    let Some(level) = value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
    else {
        return Ok(None);
    };
    let normalized = match level.to_ascii_lowercase().as_str() {
        "none" | "off" | "disabled" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" => {
            if provider == "deepseek" {
                "max"
            } else {
                "xhigh"
            }
        }
        _ => {
            return Err(bad_request(
                "thinking_level only supports none/auto/minimal/low/medium/high/xhigh/max",
            ))
        }
    };
    let allowed = match provider.as_str() {
        "gpt" => ["none", "minimal", "low", "medium", "high", "xhigh"].as_slice(),
        "deepseek" => ["none", "low", "medium", "high", "max"].as_slice(),
        "kimi" => ["none", "auto", "low", "medium", "high", "xhigh"].as_slice(),
        _ => ["none", "low", "medium", "high", "xhigh"].as_slice(),
    };
    if !allowed.contains(&normalized) {
        return Err(bad_request(
            "thinking_level is not supported by the selected provider",
        ));
    }
    Ok(Some(normalized.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(source_provider_id: Option<&str>) -> UserModelConfigRecord {
        UserModelConfigRecord {
            id: "model-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_provider_id: source_provider_id.map(ToOwned::to_owned),
            name: "Model".to_string(),
            provider: "gpt".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            model: "gpt-test".to_string(),
            thinking_level: None,
            task_usage_scenario: None,
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            api_key: None,
            has_api_key: false,
            base_url: Some("https://gateway.example/v1".to_string()),
            enabled: true,
            supports_images: false,
            supports_reasoning: false,
            supports_responses: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn provider(owner_user_id: &str) -> UserModelProviderRecord {
        UserModelProviderRecord {
            id: "provider-1".to_string(),
            owner_user_id: owner_user_id.to_string(),
            name: "Provider".to_string(),
            provider: "gpt".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            api_key: None,
            has_api_key: false,
            base_url: Some("https://gateway.example/v1".to_string()),
            enabled: true,
            supports_images: false,
            supports_reasoning: false,
            supports_responses: false,
            last_sync_status: None,
            last_sync_error: None,
            last_synced_at: None,
            imported_model_count: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn normalizes_glm_provider_aliases_and_prompt_vendor() {
        let provider = normalize_provider_input(Some("zhipu".to_string())).expect("provider");
        let prompt_vendor =
            normalize_prompt_vendor_input(None, provider.as_str()).expect("prompt vendor");

        assert_eq!(provider, "glm");
        assert_eq!(prompt_vendor.as_deref(), Some("glm"));
    }

    #[test]
    fn rejects_removed_provider_values() {
        assert!(normalize_provider_input(Some("openai_compatible".to_string())).is_err());
        assert!(normalize_provider_input(Some("minimax".to_string())).is_err());
    }

    #[test]
    fn explicit_source_provider_id_is_authoritative() {
        assert!(model_config_belongs_to_provider(
            Some("provider-1"),
            "glm",
            Some("https://old.example/v1"),
            "provider-1",
            "gpt",
            Some("https://new.example/v1"),
        ));
        assert!(!model_config_belongs_to_provider(
            Some("provider-2"),
            "glm",
            Some("https://same.example/v1"),
            "provider-1",
            "glm",
            Some("https://same.example/v1"),
        ));
    }

    #[test]
    fn legacy_models_fall_back_to_provider_and_base_url() {
        assert!(model_config_belongs_to_provider(
            None,
            "glm",
            Some("https://same.example/v1/"),
            "provider-1",
            "glm",
            Some("https://same.example/v1"),
        ));
    }

    #[test]
    fn orphan_models_are_not_backed_by_an_unrelated_or_missing_provider() {
        assert!(!model_config_has_backing_provider(&model(None), &[]));
        assert!(!model_config_has_backing_provider(
            &model(None),
            &[provider("user-2")]
        ));
        assert!(model_config_has_backing_provider(
            &model(None),
            &[provider("user-1")]
        ));
        assert!(model_config_has_backing_provider(
            &model(Some("provider-1")),
            &[provider("user-1")]
        ));
    }
}
