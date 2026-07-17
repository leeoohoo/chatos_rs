// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;

use chatos_agent::{agent_max_iterations_from_env, AGENT_MAX_ITERATIONS_CONFIG_KEY};
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_settings::DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES;
use crate::core::pagination::parse_js_int_value;
use crate::repositories::user_settings as repo;

pub const USER_PREFERENCE_KEYS: &[&str] = &["INTERNAL_CONTEXT_LOCALE", "UI_LOCALE"];

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
    let max_iterations = agent_max_iterations_from_env() as i64;
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
    if let Some(client) = managed_config_client() {
        if let Ok(snapshot) = client.load().await {
            if let Value::Object(base_map) = &mut base {
                for (config_key, legacy_key) in [
                    (AGENT_MAX_ITERATIONS_CONFIG_KEY, "MAX_ITERATIONS"),
                    (
                        "chatos.task.follow_up_max_rounds",
                        "TASK_FOLLOW_UP_MAX_ROUNDS",
                    ),
                    ("chatos.conversation.history_limit", "HISTORY_LIMIT"),
                    ("chatos.ai.max_output_tokens", "CHAT_MAX_TOKENS"),
                    (
                        "chatos.attachment.total_max_bytes",
                        "ATTACHMENT_TOTAL_MAX_BYTES",
                    ),
                    ("chatos.ui.terminal_enabled", "TERMINAL_UI_ENABLED"),
                    ("shared.logging.level", "LOG_LEVEL"),
                ] {
                    if let Some(value) = snapshot.value(config_key) {
                        base_map.insert(legacy_key.to_string(), coerce(value, legacy_key));
                    }
                }
            }
        }
    }

    if let Some(user_id) = user_id {
        let settings = repo::get_user_settings(user_id.as_str())
            .await?
            .map(|row| row.settings)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        if let (Value::Object(settings), Value::Object(base)) = (settings, &mut base) {
            for key in USER_PREFERENCE_KEYS {
                if let Some(value) = settings.get(*key) {
                    base.insert((*key).to_string(), coerce(value, key));
                }
            }
        }
    }
    Ok(base)
}

fn managed_config_client() -> Option<&'static chatos_config_sdk::ConfigClient> {
    static CLIENT: OnceLock<Option<chatos_config_sdk::ConfigClient>> = OnceLock::new();
    CLIENT
        .get_or_init(|| chatos_config_sdk::ConfigClient::from_env("chatos-backend").ok())
        .as_ref()
}

pub async fn save_user_settings(user_id: &str, settings: &Value) -> Result<Value, String> {
    let clean = preference_values(settings);
    repo::set_user_settings(user_id, &clean).await?;
    get_effective_user_settings(Some(user_id.to_string())).await
}

pub async fn patch_user_settings(user_id: &str, patch: &Value) -> Result<Value, String> {
    let mut merged = repo::get_user_settings(user_id)
        .await?
        .map(|row| preference_values(&row.settings))
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if let (Value::Object(target), Value::Object(clean)) = (&mut merged, preference_values(patch)) {
        target.extend(clean);
    }
    repo::set_user_settings(user_id, &merged).await?;
    get_effective_user_settings(Some(user_id.to_string())).await
}

fn preference_values(value: &Value) -> Value {
    let mut clean = serde_json::Map::new();
    if let Value::Object(values) = value {
        for key in USER_PREFERENCE_KEYS {
            if let Some(value) = values.get(*key) {
                clean.insert((*key).to_string(), coerce(value, key));
            }
        }
    }
    Value::Object(clean)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::preference_values;

    #[test]
    fn keeps_language_preferences_and_drops_managed_runtime_fields() {
        assert_eq!(
            preference_values(&json!({
                "UI_LOCALE": "en-US",
                "INTERNAL_CONTEXT_LOCALE": "zh-CN",
                "MAX_ITERATIONS": 25,
                "TERMINAL_UI_ENABLED": false,
            })),
            json!({
                "UI_LOCALE": "en-US",
                "INTERNAL_CONTEXT_LOCALE": "zh-CN",
            })
        );
    }
}
