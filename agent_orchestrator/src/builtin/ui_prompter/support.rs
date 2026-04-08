use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::services::ui_prompt_manager::{
    normalize_choice_limits, normalize_choice_options, normalize_default_selection,
    normalize_kv_fields, ChoiceLimits, ChoiceOption, KvField, LimitMode,
};

pub(super) fn parse_choice_block(
    input: Option<&Value>,
) -> Result<Option<(bool, Vec<ChoiceOption>, ChoiceLimits, Value)>, String> {
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
        LimitMode::Clamp,
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

pub(super) fn parse_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|raw| raw as i64)))
}

pub(super) fn optional_string(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

pub(super) fn make_prompt_id() -> String {
    format!("up_{}", Uuid::new_v4().simple())
}

pub(super) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
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
    choice: Option<(bool, Vec<ChoiceOption>, ChoiceLimits, Value)>,
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
