use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::support::trimmed;

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
