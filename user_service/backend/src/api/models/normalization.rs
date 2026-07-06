// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use sha2::{Digest, Sha256};

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

pub(super) fn provider_display_name_prefix(name: &str, model: &str) -> String {
    let name = name.trim();
    let model = model.trim();
    if !model.is_empty() {
        let suffix = format!(" / {model}");
        if let Some(prefix) = name.strip_suffix(suffix.as_str()) {
            let prefix = prefix.trim();
            if !prefix.is_empty() {
                return prefix.to_string();
            }
        }
    }
    name.to_string()
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
        "minimax" => Ok("minimax".to_string()),
        "openai_compatible" => Ok("openai_compatible".to_string()),
        _ => Err(bad_request(
            "provider only supports gpt / deepseek / kimi / minimax / openai_compatible",
        )),
    }
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
        "openai_compatible" | "compatible" => "openai_compatible".to_string(),
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
    if provider == "openai_compatible" && normalized == "minimal" {
        return Ok(Some("low".to_string()));
    }
    if !allowed.contains(&normalized) {
        return Err(bad_request(
            "thinking_level is not supported by the selected provider",
        ));
    }
    Ok(Some(normalized.to_string()))
}
