use std::collections::HashMap;

use serde_json::{Map, Value};

use super::choice::{
    normalize_choice_limits, normalize_choice_options, normalize_choice_selection, ChoiceLimits,
    ChoiceOption, LimitMode,
};
use super::fields::{normalize_kv_fields, KvField};
use super::values::normalize_kv_values;
use super::{UiPromptPayload, UiPromptResponseSubmission};

#[derive(Debug, Clone)]
struct ChoiceConfig {
    multiple: bool,
    options: Vec<ChoiceOption>,
    limits: ChoiceLimits,
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
