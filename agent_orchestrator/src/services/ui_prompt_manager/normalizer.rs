use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::types::{UiPromptPayload, UiPromptResponseSubmission};

const SECRET_MASK: &str = "******";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub placeholder: String,
    pub default_value: String,
    pub required: bool,
    pub multiline: bool,
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy)]
pub enum LimitMode {
    Clamp,
    #[allow(dead_code)]
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceLimits {
    pub min_selections: i64,
    pub max_selections: i64,
}

pub fn normalize_kv_fields(
    value: Option<&Value>,
    max_fields: usize,
) -> Result<Vec<KvField>, String> {
    let fields = value
        .and_then(Value::as_array)
        .ok_or_else(|| "fields is required".to_string())?;
    if fields.is_empty() {
        return Err("fields is required".to_string());
    }
    if fields.len() > max_fields {
        return Err(format!("fields must be <= {max_fields}"));
    }

    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        let key = normalize_unique_kv_field_key(field, index, &seen);
        seen.insert(key.clone());

        let label = {
            let value = trimmed(field.get("label").and_then(Value::as_str));
            if value.is_empty() {
                key.clone()
            } else {
                value
            }
        };

        out.push(KvField {
            key,
            label,
            description: trimmed(field.get("description").and_then(Value::as_str)),
            placeholder: trimmed(field.get("placeholder").and_then(Value::as_str)),
            default_value: field
                .get("default")
                .or_else(|| field.get("default_value"))
                .and_then(Value::as_str)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            required: field
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            multiline: field
                .get("multiline")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            secret: field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }

    Ok(out)
}

fn normalize_unique_kv_field_key(field: &Value, index: usize, seen: &HashSet<String>) -> String {
    let explicit = field
        .get("key")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            field
                .get("name")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            field
                .get("id")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        });

    let base_key = explicit
        .or_else(|| {
            field
                .get("label")
                .and_then(Value::as_str)
                .map(slugify_fallback_key)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            field
                .get("placeholder")
                .and_then(Value::as_str)
                .map(slugify_fallback_key)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| format!("field_{}", index + 1));

    ensure_unique_key(base_key, seen)
}

fn slugify_fallback_key(raw: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;

    for ch in raw.trim().chars() {
        if ch.is_alphanumeric() {
            if ch.is_ascii() {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
            last_sep = false;
            continue;
        }

        if (ch == '_' || ch == '-' || ch.is_whitespace()) && !out.is_empty() && !last_sep {
            out.push('_');
            last_sep = true;
        }
    }

    out.trim_matches('_').to_string()
}

fn ensure_unique_key(base_key: String, seen: &HashSet<String>) -> String {
    if !seen.contains(&base_key) {
        return base_key;
    }

    let mut idx = 2;
    loop {
        let candidate = format!("{}_{}", base_key, idx);
        if !seen.contains(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

pub fn normalize_choice_options(
    value: Option<&Value>,
    max_options: usize,
) -> Result<Vec<ChoiceOption>, String> {
    let options = value
        .and_then(Value::as_array)
        .ok_or_else(|| "options is required".to_string())?;
    if options.is_empty() {
        return Err("options is required".to_string());
    }
    if options.len() > max_options {
        return Err(format!("options must be <= {max_options}"));
    }

    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(options.len());
    for option in options {
        let value = trimmed(option.get("value").and_then(Value::as_str));
        if value.is_empty() {
            return Err("options[].value is required".to_string());
        }
        if seen.contains(&value) {
            return Err(format!("duplicate option value: {value}"));
        }
        seen.insert(value.clone());

        out.push(ChoiceOption {
            value,
            label: trimmed(option.get("label").and_then(Value::as_str)),
            description: trimmed(option.get("description").and_then(Value::as_str)),
        });
    }

    Ok(out)
}

pub fn normalize_choice_limits(
    multiple: bool,
    min: Option<i64>,
    max: Option<i64>,
    option_count: usize,
    mode: LimitMode,
    single_min: Option<i64>,
    single_max: Option<i64>,
) -> Result<ChoiceLimits, String> {
    let count = option_count as i64;

    if !multiple {
        let min_value = match mode {
            LimitMode::Clamp => single_min.unwrap_or(0).clamp(0, 1),
            LimitMode::Strict => {
                let raw = single_min.unwrap_or(0);
                if !(0..=1).contains(&raw) {
                    return Err("single-choice min must be 0 or 1".to_string());
                }
                raw
            }
        };

        let max_value = match mode {
            LimitMode::Clamp => single_max.unwrap_or(1).clamp(0, 1),
            LimitMode::Strict => {
                let raw = single_max.unwrap_or(1);
                if !(0..=1).contains(&raw) {
                    return Err("single-choice max must be 0 or 1".to_string());
                }
                raw
            }
        };

        if min_value > max_value {
            return Err("minSelections must be <= maxSelections".to_string());
        }

        return Ok(ChoiceLimits {
            min_selections: min_value,
            max_selections: max_value,
        });
    }

    if matches!(mode, LimitMode::Clamp) {
        let min_raw = min.unwrap_or(0);
        let max_raw = max.unwrap_or(count);
        let min_value = if min_raw >= 0 {
            min_raw.clamp(0, count)
        } else {
            0
        };
        let max_value = if max_raw >= 1 {
            max_raw.clamp(1, count)
        } else {
            count
        };
        return Ok(ChoiceLimits {
            min_selections: min_value.min(max_value),
            max_selections: max_value,
        });
    }

    let min_raw = min.unwrap_or(0);
    let max_raw = max.unwrap_or(count);
    if min_raw < 0 || min_raw > count {
        return Err(format!(
            "minSelections must be an int between 0 and {count}"
        ));
    }
    if max_raw < 1 || max_raw > count {
        return Err(format!(
            "maxSelections must be an int between 1 and {count}"
        ));
    }
    if min_raw > max_raw {
        return Err("minSelections must be <= maxSelections".to_string());
    }

    Ok(ChoiceLimits {
        min_selections: min_raw,
        max_selections: max_raw,
    })
}

pub fn normalize_default_selection(
    input: Option<&Value>,
    multiple: bool,
    options: &[ChoiceOption],
) -> Value {
    let allowed: HashSet<String> = options.iter().map(|option| option.value.clone()).collect();

    if multiple {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for value in collect_selection_values(input) {
            if value.is_empty() || !allowed.contains(&value) || seen.contains(&value) {
                continue;
            }
            seen.insert(value.clone());
            out.push(Value::String(value));
        }
        return Value::Array(out);
    }

    let selected = input
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| allowed.contains(value))
        .unwrap_or_default();

    Value::String(selected)
}

pub fn normalize_choice_selection(
    selection: Option<&Value>,
    multiple: bool,
    options: &[ChoiceOption],
) -> Value {
    normalize_default_selection(selection, multiple, options)
}

pub fn normalize_kv_values(values: Option<&Value>, fields: &[KvField]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let field_map: HashMap<String, &KvField> = fields
        .iter()
        .map(|field| (field.key.clone(), field))
        .collect();

    if let Some(Value::Object(map)) = values {
        for (key, value) in map {
            let normalized_key = key.trim().to_string();
            if normalized_key.is_empty() || !field_map.contains_key(&normalized_key) {
                continue;
            }

            let normalized_value = if let Some(raw) = value.as_str() {
                raw.to_string()
            } else if value.is_null() {
                String::new()
            } else {
                value.to_string()
            };

            out.insert(normalized_key, normalized_value);
        }
    }

    for field in fields {
        let current = out.get(&field.key).map(|value| value.trim()).unwrap_or("");
        if current.is_empty() && !field.default_value.is_empty() {
            out.insert(field.key.clone(), field.default_value.clone());
        }
    }

    out
}

pub fn parse_response_submission(
    raw: Value,
    prompt: &UiPromptPayload,
) -> Result<UiPromptResponseSubmission, String> {
    let obj = raw
        .as_object()
        .ok_or_else(|| "response payload must be an object".to_string())?;

    let status = normalize_submission_status(obj.get("status").and_then(Value::as_str));
    if status != "ok" {
        if status == "canceled" && !prompt.allow_cancel {
            return Err("cancel is not allowed for this prompt".to_string());
        }
        return Ok(UiPromptResponseSubmission {
            status,
            values: None,
            selection: None,
            reason: obj
                .get("reason")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        });
    }

    let fields = extract_fields(prompt)?;
    let choice = extract_choice(prompt)?;

    let values = if fields.is_empty() {
        None
    } else {
        let normalized_values = normalize_kv_values(obj.get("values"), fields.as_slice());
        ensure_required_fields(fields.as_slice(), &normalized_values)?;

        let mut map = Map::new();
        for (key, value) in normalized_values {
            map.insert(key, Value::String(value));
        }
        Some(Value::Object(map))
    };

    let selection = if let Some(choice_config) = choice {
        let selection_input = obj
            .get("selection")
            .or_else(|| obj.get("value"))
            .or_else(|| obj.get("values"));
        let normalized = normalize_choice_selection(
            selection_input,
            choice_config.multiple,
            choice_config.options.as_slice(),
        );
        validate_choice_limits(&normalized, choice_config.multiple, &choice_config.limits)?;
        Some(normalized)
    } else {
        None
    };

    Ok(UiPromptResponseSubmission {
        status,
        values,
        selection,
        reason: obj
            .get("reason")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    })
}

pub fn redact_prompt_payload(payload: &UiPromptPayload) -> Value {
    let mut prompt_value = serde_json::to_value(payload).unwrap_or_else(|_| json!({}));

    if let Some(fields) = prompt_value
        .get_mut("payload")
        .and_then(|value| value.get_mut("fields"))
        .and_then(Value::as_array_mut)
    {
        for field in fields {
            let secret = field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if secret {
                if let Some(default_value) = field.get_mut("default") {
                    *default_value = Value::String(SECRET_MASK.to_string());
                }
                if let Some(default_value) = field.get_mut("default_value") {
                    *default_value = Value::String(SECRET_MASK.to_string());
                }
            }
        }
    }

    prompt_value
}

pub fn redact_response_for_store(
    response: &UiPromptResponseSubmission,
    payload: &UiPromptPayload,
) -> Value {
    let mut out = serde_json::to_value(response).unwrap_or_else(|_| json!({}));
    let Some(secret_keys) = secret_field_keys(payload) else {
        return out;
    };

    let Some(values_obj) = out.get_mut("values").and_then(Value::as_object_mut) else {
        return out;
    };

    for key in secret_keys {
        if values_obj.contains_key(key.as_str()) {
            values_obj.insert(key, Value::String(SECRET_MASK.to_string()));
        }
    }

    out
}

pub(super) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[derive(Debug, Clone)]
struct ChoiceConfig {
    multiple: bool,
    options: Vec<ChoiceOption>,
    limits: ChoiceLimits,
}

fn extract_fields(prompt: &UiPromptPayload) -> Result<Vec<KvField>, String> {
    match prompt.payload.get("fields") {
        Some(Value::Array(_)) => normalize_kv_fields(prompt.payload.get("fields"), 50),
        Some(_) => Err("payload.fields must be an array".to_string()),
        None => Ok(Vec::new()),
    }
}

fn extract_choice(prompt: &UiPromptPayload) -> Result<Option<ChoiceConfig>, String> {
    let choice_obj = prompt.payload.get("choice");
    let Some(choice_obj) = choice_obj else {
        return Ok(None);
    };

    let multiple = choice_obj
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let options = normalize_choice_options(choice_obj.get("options"), 60)?;
    let limits = normalize_choice_limits(
        multiple,
        choice_obj.get("min_selections").and_then(Value::as_i64),
        choice_obj.get("max_selections").and_then(Value::as_i64),
        options.len(),
        LimitMode::Clamp,
        choice_obj
            .get("single_min_selections")
            .and_then(Value::as_i64),
        choice_obj
            .get("single_max_selections")
            .and_then(Value::as_i64),
    )?;

    Ok(Some(ChoiceConfig {
        multiple,
        options,
        limits,
    }))
}

fn ensure_required_fields(
    fields: &[KvField],
    values: &HashMap<String, String>,
) -> Result<(), String> {
    for field in fields {
        if !field.required {
            continue;
        }

        let value = values.get(&field.key).map(|item| item.trim()).unwrap_or("");
        if value.is_empty() {
            return Err(format!("field {} is required", field.key));
        }
    }

    Ok(())
}

fn validate_choice_limits(
    selection: &Value,
    multiple: bool,
    limits: &ChoiceLimits,
) -> Result<(), String> {
    let count = if multiple {
        selection
            .as_array()
            .map(|arr| arr.len() as i64)
            .unwrap_or(0)
    } else {
        selection
            .as_str()
            .map(|value| if value.trim().is_empty() { 0 } else { 1 })
            .unwrap_or(0)
    };

    if count < limits.min_selections {
        return Err(format!(
            "selection count must be >= {}",
            limits.min_selections
        ));
    }
    if count > limits.max_selections {
        return Err(format!(
            "selection count must be <= {}",
            limits.max_selections
        ));
    }

    Ok(())
}

fn normalize_submission_status(value: Option<&str>) -> String {
    match value
        .map(|item| item.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "ok".to_string())
        .as_str()
    {
        "ok" | "confirm" => "ok".to_string(),
        "cancel" | "canceled" | "cancelled" => "canceled".to_string(),
        "timeout" => "timeout".to_string(),
        _ => "canceled".to_string(),
    }
}

fn collect_selection_values(value: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(value) = value else {
        return out;
    };

    if let Some(array) = value.as_array() {
        for item in array {
            if let Some(text) = item.as_str() {
                out.push(text.trim().to_string());
            }
        }
        return out;
    }

    if let Some(text) = value.as_str() {
        out.push(text.trim().to_string());
    }

    out
}

fn secret_field_keys(payload: &UiPromptPayload) -> Option<Vec<String>> {
    let fields = payload.payload.get("fields").and_then(Value::as_array)?;

    let keys: Vec<String> = fields
        .iter()
        .filter_map(|field| {
            if !field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let key = field
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if key.is_empty() {
                None
            } else {
                Some(key.to_string())
            }
        })
        .collect();

    if keys.is_empty() {
        None
    } else {
        Some(keys)
    }
}

fn trimmed(value: Option<&str>) -> String {
    value
        .map(|item| item.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::normalize_kv_fields;
    use serde_json::json;

    #[test]
    fn normalize_kv_fields_derives_missing_keys_and_dedupes() {
        let input = json!([
            {
                "name": "repo",
                "label": "Repository"
            },
            {
                "label": "API Token"
            },
            {
                "key": "repo",
                "label": "Repository Mirror"
            }
        ]);

        let fields = normalize_kv_fields(Some(&input), 50).expect("normalize fields");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].key, "repo");
        assert_eq!(fields[1].key, "api_token");
        assert_eq!(fields[2].key, "repo_2");
    }
}
