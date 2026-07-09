// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::env;

use crate::ServiceRuntimeError;

pub fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn env_bool(key: &str, default_value: bool) -> bool {
    env_text(key)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default_value)
}

pub(crate) fn env_u64(key: &str, default_value: u64) -> u64 {
    env_text(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default_value)
}

pub(crate) fn merge_env_config_text(
    values: &mut HashMap<String, String>,
    text: &str,
) -> Result<(), ServiceRuntimeError> {
    let parsed: serde_json::Value = serde_json::from_str(text.trim())?;
    let object = parsed
        .get("env")
        .and_then(serde_json::Value::as_object)
        .or_else(|| parsed.as_object())
        .ok_or_else(|| {
            ServiceRuntimeError::InvalidConfig(
                "expected JSON object or object with an env field".to_string(),
            )
        })?;

    for (key, value) in object {
        if !is_allowed_env_key(key) {
            tracing::warn!(key = key.as_str(), "ignoring invalid config center env key");
            continue;
        }
        let Some(value) = config_value_to_env_text(value)? else {
            continue;
        };
        values.insert(key.clone(), value);
    }
    Ok(())
}

fn config_value_to_env_text(
    value: &serde_json::Value,
) -> Result<Option<String>, ServiceRuntimeError> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.clone())),
        serde_json::Value::Bool(value) => Ok(Some(value.to_string())),
        serde_json::Value::Number(value) => Ok(Some(value.to_string())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Some(serde_json::to_string(value)?))
        }
    }
}

fn is_allowed_env_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && key
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::merge_env_config_text;

    #[test]
    fn merges_config_center_env_values() {
        let mut values = HashMap::new();
        merge_env_config_text(
            &mut values,
            r#"{"env":{"CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS":1500,"FEATURE_FLAG":true,"bad-key":"nope"}}"#,
        )
        .expect("merge env config");

        assert_eq!(
            values.get("CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS"),
            Some(&"1500".to_string())
        );
        assert_eq!(values.get("FEATURE_FLAG"), Some(&"true".to_string()));
        assert!(!values.contains_key("bad-key"));
    }
}
