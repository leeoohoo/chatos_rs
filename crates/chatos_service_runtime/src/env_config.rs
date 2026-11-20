// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::str::FromStr;

pub fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn parse_bool_text(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn env_bool_strict(key: &str, default_value: bool) -> Result<bool, String> {
    let Some(value) = env_text(key) else {
        return Ok(default_value);
    };
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}

pub fn env_flag(key: &str, default_value: bool) -> bool {
    env_text(key)
        .map(|value| parse_bool_text(value.as_str()).unwrap_or(false))
        .unwrap_or(default_value)
}

pub fn env_parse<T>(key: &str) -> Option<T>
where
    T: FromStr,
{
    env_text(key).and_then(|value| value.parse::<T>().ok())
}

pub(crate) fn env_bool(key: &str, default_value: bool) -> bool {
    env_flag(key, default_value)
}

pub(crate) fn env_u64(key: &str, default_value: u64) -> u64 {
    env_parse(key).unwrap_or(default_value)
}

#[cfg(test)]
mod tests {
    use super::parse_bool_text;

    #[test]
    fn parses_supported_boolean_text() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert_eq!(parse_bool_text(value), Some(true));
        }
        for value in ["0", "false", "FALSE", "no", "off"] {
            assert_eq!(parse_bool_text(value), Some(false));
        }
        assert_eq!(parse_bool_text("invalid"), None);
    }
}
