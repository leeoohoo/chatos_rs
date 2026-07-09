// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_settings::DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES;
use crate::core::pagination::parse_js_int_value;
use crate::repositories::user_settings as repo;

pub const USER_SETTING_KEYS: &[&str] = &[
    "MAX_ITERATIONS",
    "TASK_FOLLOW_UP_MAX_ROUNDS",
    "LOG_LEVEL",
    "HISTORY_LIMIT",
    "CHAT_MAX_TOKENS",
    "ATTACHMENT_TOTAL_MAX_BYTES",
    "INTERNAL_CONTEXT_LOCALE",
    "UI_LOCALE",
    "TERMINAL_UI_ENABLED",
];

fn coerce(value: &Value, key: &str) -> Value {
    if value.is_null() {
        return Value::Null;
    }
    match key {
        "MAX_ITERATIONS"
        | "TASK_FOLLOW_UP_MAX_ROUNDS"
        | "HISTORY_LIMIT"
        | "CHAT_MAX_TOKENS"
        | "ATTACHMENT_TOTAL_MAX_BYTES" => parse_js_int_value(value)
            .map(|n| Value::Number(serde_json::Number::from(n)))
            .unwrap_or(Value::Null),
        "LOG_LEVEL" => Value::String(value.as_str().unwrap_or(&value.to_string()).to_string()),
        "INTERNAL_CONTEXT_LOCALE" => Value::String(
            value
                .as_str()
                .map(str::trim)
                .filter(|item| matches!(*item, "zh-CN" | "en-US"))
                .unwrap_or("zh-CN")
                .to_string(),
        ),
        "UI_LOCALE" => Value::String(
            value
                .as_str()
                .map(str::trim)
                .filter(|item| matches!(*item, "zh-CN" | "en-US"))
                .unwrap_or("zh-CN")
                .to_string(),
        ),
        "TERMINAL_UI_ENABLED" => Value::Bool(match value {
            Value::Bool(flag) => *flag,
            Value::Number(number) => number.as_i64().unwrap_or(1) != 0,
            Value::String(text) => {
                let normalized = text.trim().to_ascii_lowercase();
                !matches!(normalized.as_str(), "false" | "0" | "off")
            }
            _ => true,
        }),
        _ => value.clone(),
    }
}

pub fn get_default_user_settings() -> Result<Value, String> {
    let cfg = Config::try_get()?;
    let max_iterations = std::env::var("MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(600);
    let task_follow_up_max_rounds = std::env::var("TASK_FOLLOW_UP_MAX_ROUNDS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3);
    let history_limit = std::env::var("HISTORY_LIMIT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(20);
    let chat_max_tokens = std::env::var("CHAT_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .map(serde_json::Number::from)
        .map(Value::Number)
        .unwrap_or(Value::Null);
    let attachment_total_max_bytes = std::env::var("ATTACHMENT_TOTAL_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES);

    Ok(json!({
        "MAX_ITERATIONS": max_iterations,
        "TASK_FOLLOW_UP_MAX_ROUNDS": task_follow_up_max_rounds,
        "LOG_LEVEL": cfg.log_level,
        "HISTORY_LIMIT": history_limit,
        "CHAT_MAX_TOKENS": chat_max_tokens,
        "ATTACHMENT_TOTAL_MAX_BYTES": attachment_total_max_bytes,
        "INTERNAL_CONTEXT_LOCALE": "zh-CN",
        "UI_LOCALE": "zh-CN",
        "TERMINAL_UI_ENABLED": true,
    }))
}

pub async fn get_effective_user_settings(user_id: Option<String>) -> Result<Value, String> {
    let mut base = get_default_user_settings()?;
    let Some(user_id) = user_id else {
        return Ok(base);
    };
    let row = repo::get_user_settings(&user_id).await?;
    let settings = row
        .map(|r| r.settings)
        .unwrap_or(Value::Object(serde_json::Map::new()));
    if let Value::Object(map) = settings {
        if let Value::Object(base_map) = &mut base {
            for k in USER_SETTING_KEYS {
                if let Some(v) = map.get(*k) {
                    base_map.insert((*k).to_string(), coerce(v, k));
                }
            }
        }
    }
    Ok(base)
}

pub async fn save_user_settings(user_id: &str, settings: &Value) -> Result<Value, String> {
    let mut clean = serde_json::Map::new();
    if let Value::Object(map) = settings {
        for k in USER_SETTING_KEYS {
            if let Some(v) = map.get(*k) {
                clean.insert((*k).to_string(), coerce(v, k));
            }
        }
    }
    let val = Value::Object(clean);
    repo::set_user_settings(user_id, &val).await?;
    get_effective_user_settings(Some(user_id.to_string())).await
}

pub async fn patch_user_settings(user_id: &str, patch: &Value) -> Result<Value, String> {
    let mut clean = serde_json::Map::new();
    if let Value::Object(map) = patch {
        for k in USER_SETTING_KEYS {
            if let Some(v) = map.get(*k) {
                clean.insert((*k).to_string(), coerce(v, k));
            }
        }
    }
    let val = Value::Object(clean);
    let _ = repo::update_user_settings(user_id, &val).await?;
    get_effective_user_settings(Some(user_id.to_string())).await
}

pub trait AiClientSettings {
    fn apply_settings(&mut self, effective: &Value);
}

pub fn apply_settings_to_ai_client<T: AiClientSettings>(ai_client: &mut T, effective: &Value) {
    ai_client.apply_settings(effective);
}
