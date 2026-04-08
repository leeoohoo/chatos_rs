use serde_json::Value;
use std::collections::HashMap;

pub fn parse_args_json_array(args: &Option<Value>) -> Vec<String> {
    match args {
        Some(Value::String(raw)) => serde_json::from_str::<Vec<Value>>(raw)
            .ok()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(|item| item.trim().to_string()))
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.trim().to_string()))
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn parse_args_json_array_or_whitespace(args: &Option<Value>) -> Vec<String> {
    match args {
        Some(Value::String(raw)) => {
            if let Some(parsed) = serde_json::from_str::<Vec<Value>>(raw).ok().map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(|item| item.trim().to_string()))
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            }) {
                return parsed;
            }

            raw.split_whitespace()
                .map(|item| item.to_string())
                .collect()
        }
        _ => parse_args_json_array(args),
    }
}

pub fn parse_env(env: &Option<Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();

    match env {
        Some(Value::String(raw)) => {
            if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                if let Value::Object(obj) = parsed {
                    for (key, value) in obj {
                        if let Some(parsed_value) = value_to_env_string(&value) {
                            map.insert(key, parsed_value);
                        }
                    }
                }
            }
        }
        Some(Value::Object(obj)) => {
            for (key, value) in obj {
                if let Some(parsed_value) = value_to_env_string(value) {
                    map.insert(key.clone(), parsed_value);
                }
            }
        }
        _ => {}
    }

    map
}

fn value_to_env_string(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_args_json_array, parse_args_json_array_or_whitespace, parse_env};
    use serde_json::json;

    #[test]
    fn parse_args_json_array_keeps_legacy_strict_behavior() {
        let args = Some(json!("--help -v"));
        assert!(parse_args_json_array(&args).is_empty());

        let args = Some(json!(["--help", "-v"]));
        assert_eq!(parse_args_json_array(&args), vec!["--help", "-v"]);
    }

    #[test]
    fn parse_args_json_array_or_whitespace_supports_plain_string_fallback() {
        let args = Some(json!("--help -v"));
        assert_eq!(
            parse_args_json_array_or_whitespace(&args),
            vec!["--help", "-v"]
        );
    }

    #[test]
    fn parse_env_accepts_json_string_and_object() {
        let from_str = Some(json!("{\"A\":\"1\",\"B\":2}"));
        let parsed_str = parse_env(&from_str);
        assert_eq!(parsed_str.get("A").map(String::as_str), Some("1"));
        assert_eq!(parsed_str.get("B").map(String::as_str), Some("2"));

        let from_obj = Some(json!({"A":"1","B":2,"C":null}));
        let parsed_obj = parse_env(&from_obj);
        assert_eq!(parsed_obj.get("A").map(String::as_str), Some("1"));
        assert_eq!(parsed_obj.get("B").map(String::as_str), Some("2"));
        assert!(parsed_obj.get("C").is_none());
    }
}
