// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceLimits {
    pub min_selections: i64,
    pub max_selections: i64,
}

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

pub(super) type ChoiceBlock = (bool, Vec<ChoiceOption>, ChoiceLimits, Value);

pub(super) fn normalize_choice_options(
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

pub(super) fn normalize_choice_limits(
    multiple: bool,
    min: Option<i64>,
    max: Option<i64>,
    option_count: usize,
    single_min: Option<i64>,
    single_max: Option<i64>,
) -> Result<ChoiceLimits, String> {
    let count = option_count as i64;

    if !multiple {
        let min_value = single_min.unwrap_or(0).clamp(0, 1);
        let max_value = single_max.unwrap_or(1).clamp(0, 1);
        if min_value > max_value {
            return Err("minSelections must be <= maxSelections".to_string());
        }
        return Ok(ChoiceLimits {
            min_selections: min_value,
            max_selections: max_value,
        });
    }

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

    Ok(ChoiceLimits {
        min_selections: min_value.min(max_value),
        max_selections: max_value,
    })
}

pub(super) fn normalize_default_selection(
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

pub(super) fn normalize_kv_fields(
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
                .map(ToOwned::to_owned)
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

pub(super) fn parse_choice_block(input: Option<&Value>) -> Result<Option<ChoiceBlock>, String> {
    let Some(choice_input) = input else {
        return Ok(None);
    };
    let multiple = choice_input
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let options = normalize_choice_options(choice_input.get("options"), 60)?;
    let limits = normalize_choice_limits(
        multiple,
        parse_i64(choice_input.get("min_selections")),
        parse_i64(choice_input.get("max_selections")),
        options.len(),
        parse_i64(choice_input.get("single_min_selections")),
        parse_i64(choice_input.get("single_max_selections")),
    )?;
    let default_selection =
        normalize_default_selection(choice_input.get("default"), multiple, options.as_slice());
    Ok(Some((multiple, options, limits, default_selection)))
}

pub(super) fn kv_fields_to_value(fields: &[KvField]) -> Vec<Value> {
    fields
        .iter()
        .map(|field| {
            json!({
                "key": field.key,
                "label": field.label,
                "description": field.description,
                "placeholder": field.placeholder,
                "default": field.default_value,
                "required": field.required,
                "multiline": field.multiline,
                "secret": field.secret,
            })
        })
        .collect()
}

pub(super) fn choice_to_value(
    multiple: bool,
    options: &[ChoiceOption],
    limits: &ChoiceLimits,
    default_selection: Value,
) -> Value {
    json!({
        "multiple": multiple,
        "options": choice_options_to_value(options),
        "default": default_selection,
        "min_selections": limits.min_selections,
        "max_selections": limits.max_selections,
    })
}

pub(super) fn parse_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|raw| raw as i64)))
}

pub(super) fn build_mixed_choice_input(args: &Value) -> Option<Value> {
    args.get("choice")
        .and_then(Value::as_object)
        .cloned()
        .map(Value::Object)
        .or_else(|| {
            if args.get("options").is_some() {
                Some(args.clone())
            } else {
                None
            }
        })
}

pub(super) fn parse_mixed_fields(args: &Value) -> Result<Vec<KvField>, String> {
    match args.get("fields") {
        Some(Value::Array(_)) => normalize_kv_fields(args.get("fields"), 50),
        Some(_) => Err("fields must be an array".to_string()),
        None => Ok(Vec::new()),
    }
}

pub(super) fn build_mixed_payload_map(
    fields: &[KvField],
    choice: Option<ChoiceBlock>,
) -> Map<String, Value> {
    let mut payload_map = Map::new();
    if !fields.is_empty() {
        payload_map.insert(
            "fields".to_string(),
            Value::Array(kv_fields_to_value(fields)),
        );
    }
    if let Some((multiple, options, limits, default_selection)) = choice {
        payload_map.insert(
            "choice".to_string(),
            choice_to_value(multiple, options.as_slice(), &limits, default_selection),
        );
    }
    payload_map
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

fn choice_options_to_value(options: &[ChoiceOption]) -> Vec<Value> {
    options
        .iter()
        .map(|option| {
            json!({
                "value": option.value,
                "label": option.label,
                "description": option.description,
            })
        })
        .collect()
}

fn trimmed(value: Option<&str>) -> String {
    value
        .map(|item| item.trim().to_string())
        .unwrap_or_default()
}
