use serde_json::{json, Value};

use crate::config::Config;
use crate::core::pagination::parse_js_int_value;
use crate::repositories::user_settings as repo;

pub const USER_SETTING_KEYS: &[&str] = &[
    "SUMMARY_ENABLED",
    "DYNAMIC_SUMMARY_ENABLED",
    "SUMMARY_MESSAGE_LIMIT",
    "SUMMARY_MAX_CONTEXT_TOKENS",
    "SUMMARY_KEEP_LAST_N",
    "SUMMARY_TARGET_TOKENS",
    "SUMMARY_COOLDOWN_SECONDS",
    "MAX_ITERATIONS",
    "LOG_LEVEL",
    "HISTORY_LIMIT",
    "CHAT_MAX_TOKENS",
];

fn coerce(value: &Value, key: &str) -> Value {
    if value.is_null() {
        return Value::Null;
    }
    match key {
        "SUMMARY_ENABLED" | "DYNAMIC_SUMMARY_ENABLED" => Value::Bool(js_truthy(value)),
        "SUMMARY_MESSAGE_LIMIT"
        | "SUMMARY_MAX_CONTEXT_TOKENS"
        | "SUMMARY_KEEP_LAST_N"
        | "SUMMARY_TARGET_TOKENS"
        | "SUMMARY_COOLDOWN_SECONDS"
        | "MAX_ITERATIONS"
        | "HISTORY_LIMIT"
        | "CHAT_MAX_TOKENS" => parse_js_int_value(value)
            .map(|n| Value::Number(serde_json::Number::from(n)))
            .unwrap_or(Value::Null),
        "LOG_LEVEL" => Value::String(value.as_str().unwrap_or(&value.to_string()).to_string()),
        _ => value.clone(),
    }
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i != 0
            } else if let Some(f) = n.as_f64() {
                f != 0.0
            } else {
                false
            }
        }
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

pub fn get_default_user_settings() -> Value {
    let cfg = Config::get();
    let max_iterations = std::env::var("MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(25);
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

    json!({
        "SUMMARY_ENABLED": cfg.summary_enabled,
        "DYNAMIC_SUMMARY_ENABLED": cfg.dynamic_summary_enabled,
        "SUMMARY_MESSAGE_LIMIT": cfg.summary_message_limit,
        "SUMMARY_MAX_CONTEXT_TOKENS": cfg.summary_max_context_tokens,
        "SUMMARY_KEEP_LAST_N": cfg.summary_keep_last_n,
        "SUMMARY_TARGET_TOKENS": cfg.summary_target_tokens,
        "SUMMARY_COOLDOWN_SECONDS": cfg.summary_cooldown_seconds,
        "MAX_ITERATIONS": max_iterations,
        "LOG_LEVEL": cfg.log_level,
        "HISTORY_LIMIT": history_limit,
        "CHAT_MAX_TOKENS": chat_max_tokens,
    })
}

pub async fn get_effective_user_settings(user_id: Option<String>) -> Result<Value, String> {
    let mut base = get_default_user_settings();
    if user_id.is_none() {
        return Ok(base);
    }
    let user_id = user_id.unwrap();
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
