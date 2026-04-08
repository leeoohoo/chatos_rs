use serde_json::Value;

pub(super) fn optional_string(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

pub(super) fn optional_string_field(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn required_string(args: &Value, key: &str) -> Result<String, String> {
    let value = optional_string(args, key);
    if value.is_empty() {
        Err(format!("{key} is required"))
    } else {
        Ok(value)
    }
}

pub(super) fn parse_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn optional_string_array_field(value: Option<&Value>) -> Option<Vec<String>> {
    value
        .and_then(|item| item.as_array())
        .map(|_| parse_string_array(value))
}
