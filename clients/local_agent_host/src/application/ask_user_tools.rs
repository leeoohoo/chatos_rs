// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde_json::{json, Map, Value};
use std::{collections::HashSet, sync::Arc};

pub const ASK_USER_KEY_VALUES_TOOL: &str = "ask_user_prompt_key_values";
pub const ASK_USER_CHOICES_TOOL: &str = "ask_user_prompt_choices";
pub const ASK_USER_MIXED_FORM_TOOL: &str = "ask_user_prompt_mixed_form";
pub const ASK_USER_TOOL_NAMES: [&str; 3] = [
    ASK_USER_KEY_VALUES_TOOL,
    ASK_USER_CHOICES_TOOL,
    ASK_USER_MIXED_FORM_TOOL,
];
const ASK_USER_TIMEOUT_MS: u64 = 86_400_000;

#[derive(Clone)]
pub struct LocalAskUserToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalAskUserToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Ask User tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        let executor: Arc<dyn LocalToolExecutor> = Arc::new(self.clone());
        for name in ASK_USER_TOOL_NAMES {
            registry.register_shared(name, Arc::clone(&executor))?;
        }
        Ok(())
    }

    async fn execute(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        let run = self
            .runtime
            .get_run_for_host_worker(&invocation.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("parent Run not found: {}", invocation.run_id))?;
        validate_task_run(&run, &self.owner_user_id)?;
        let (kind, payload) = match invocation.tool_name.as_str() {
            ASK_USER_KEY_VALUES_TOOL => (
                "kv",
                json!({"fields": normalize_fields(invocation.arguments.get("fields"), true)?}),
            ),
            ASK_USER_CHOICES_TOOL => (
                "choice",
                json!({"choice": normalize_choice(&invocation.arguments, false)?}),
            ),
            ASK_USER_MIXED_FORM_TOOL => {
                let fields = normalize_fields(invocation.arguments.get("fields"), false)?;
                let choice = invocation
                    .arguments
                    .get("choice")
                    .map(|value| normalize_choice(value, false))
                    .transpose()?;
                if fields.is_empty() && choice.is_none() {
                    return Err("mixed form requires fields and/or choice".to_string());
                }
                let mut payload = Map::new();
                if !fields.is_empty() {
                    payload.insert("fields".to_string(), Value::Array(fields));
                }
                if let Some(choice) = choice {
                    payload.insert("choice".to_string(), choice);
                }
                ("mixed", Value::Object(payload))
            }
            name => return Err(format!("unsupported Ask User tool: {name}")),
        };
        Ok(json!({
            "waiting_for_user": true,
            "prompt": {
                "kind": kind,
                "tool_call_id": invocation.call_id,
                "title": optional_text(&invocation.arguments, "title", 500)?,
                "message": optional_text(&invocation.arguments, "message", 4_000)?,
                "allow_cancel": invocation.arguments.get("allow_cancel")
                    .and_then(Value::as_bool).unwrap_or(true),
                "timeout_ms": ASK_USER_TIMEOUT_MS,
                "payload": payload
            }
        }))
    }
}

#[async_trait]
impl LocalToolExecutor for LocalAskUserToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(match self.execute(invocation).await {
            Ok(output) => LocalAgentToolOutcome::Succeeded { output },
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "local_ask_user_tool"}),
            },
        })
    }
}

fn validate_task_run(run: &LocalAgentRunRecord, owner_user_id: &str) -> Result<(), String> {
    if run.owner_user_id != owner_user_id
        || run.profile_key != "task_execution"
        || run.owner_entity_type != "task"
    {
        return Err("Ask User can only be called by the active local Task".to_string());
    }
    Ok(())
}

fn normalize_fields(value: Option<&Value>, required: bool) -> Result<Vec<Value>, String> {
    let Some(fields) = value.and_then(Value::as_array) else {
        return if required {
            Err("fields is required".to_string())
        } else {
            Ok(Vec::new())
        };
    };
    if (required && fields.is_empty()) || fields.len() > 50 {
        return Err("fields must contain 1..=50 items".to_string());
    }
    let mut seen = HashSet::new();
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let object = field
                .as_object()
                .ok_or_else(|| "fields[] must be an object".to_string())?;
            let key = ["key", "name", "id"]
                .into_iter()
                .find_map(|name| object.get(name).and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("field_{}", index + 1));
            if key.len() > 256 || !seen.insert(key.clone()) {
                return Err(format!("invalid or duplicated field key: {key}"));
            }
            let label = object
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(key.as_str());
            Ok(json!({
                "key": key,
                "label": label,
                "description": limited_text(object.get("description"), 1_000)?,
                "placeholder": limited_text(object.get("placeholder"), 1_000)?,
                "default_value": limited_text(object.get("default"), 4_000)?,
                "required": object.get("required").and_then(Value::as_bool).unwrap_or(false),
                "multiline": object.get("multiline").and_then(Value::as_bool).unwrap_or(false),
                "secret": object.get("secret").and_then(Value::as_bool).unwrap_or(false)
            }))
        })
        .collect()
}

fn normalize_choice(value: &Value, nested: bool) -> Result<Value, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "choice input must be an object".to_string())?;
    let options = object
        .get("options")
        .and_then(Value::as_array)
        .ok_or_else(|| "options is required".to_string())?;
    if options.is_empty() || options.len() > 60 {
        return Err("options must contain 1..=60 items".to_string());
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(options.len());
    for option in options {
        let option = option
            .as_object()
            .ok_or_else(|| "options[] must be an object".to_string())?;
        let value = option
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "options[].value is required".to_string())?;
        if value.len() > 1_000 || !seen.insert(value.to_string()) {
            return Err(format!("invalid or duplicated option value: {value}"));
        }
        let label = option
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(value);
        normalized.push(json!({
            "value": value,
            "label": label,
            "description": limited_text(option.get("description"), 1_000)?
        }));
    }
    let multiple = object
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_default = if multiple { normalized.len() as i64 } else { 1 };
    let minimum = integer(object.get("min_selections"), 0).clamp(0, max_default);
    let maximum = integer(object.get("max_selections"), max_default)
        .clamp(minimum.max(1), max_default.max(1));
    let default = normalize_default(object.get("default"), multiple, &seen);
    let _ = nested;
    Ok(json!({
        "multiple": multiple,
        "options": normalized,
        "default": default,
        "min_selections": minimum,
        "max_selections": maximum
    }))
}

fn normalize_default(value: Option<&Value>, multiple: bool, allowed: &HashSet<String>) -> Value {
    if multiple {
        let mut seen = HashSet::new();
        return Value::Array(
            value
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| allowed.contains(*value) && seen.insert((*value).to_string()))
                .map(|value| Value::String(value.to_string()))
                .collect(),
        );
    }
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| allowed.contains(*value))
        .map(|value| Value::String(value.to_string()))
        .unwrap_or_else(|| Value::String(String::new()))
}

fn integer(value: Option<&Value>, default: i64) -> i64 {
    value.and_then(Value::as_i64).unwrap_or(default)
}

fn optional_text(value: &Value, key: &str, max: usize) -> Result<String, String> {
    limited_text(value.get(key), max)
}

fn limited_text(value: Option<&Value>, max: usize) -> Result<String, String> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    let text = value
        .as_str()
        .ok_or_else(|| "Ask User text fields must be strings".to_string())?
        .trim();
    if text.len() > max || text.contains('\0') {
        return Err(format!("Ask User text exceeds the {max} byte limit"));
    }
    Ok(text.to_string())
}

pub fn ask_user_model_tools() -> Vec<Value> {
    vec![
        ask_user_tool(
            ASK_USER_KEY_VALUES_TOOL,
            "Ask the user for structured values and wait for their submission.",
            fields_schema(true),
        ),
        ask_user_tool(
            ASK_USER_CHOICES_TOOL,
            "Ask the user to choose one or more options and wait for their submission.",
            choice_schema(),
        ),
        ask_user_tool(
            ASK_USER_MIXED_FORM_TOOL,
            "Ask the user for fields and an optional choice in one form, then wait.",
            mixed_schema(),
        ),
    ]
}

fn ask_user_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({"type": "function", "name": name, "description": description, "parameters": parameters})
}

fn common_properties() -> Map<String, Value> {
    Map::from_iter([
        (
            "title".to_string(),
            json!({"type": "string", "maxLength": 500}),
        ),
        (
            "message".to_string(),
            json!({"type": "string", "maxLength": 4000}),
        ),
        ("allow_cancel".to_string(), json!({"type": "boolean"})),
    ])
}

fn field_items_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "key": {"type": "string", "minLength": 1, "maxLength": 256},
            "name": {"type": "string", "minLength": 1, "maxLength": 256},
            "id": {"type": "string", "minLength": 1, "maxLength": 256},
            "label": {"type": "string"}, "description": {"type": "string"},
            "placeholder": {"type": "string"}, "default": {"type": "string"},
            "required": {"type": "boolean"}, "multiline": {"type": "boolean"},
            "secret": {"type": "boolean"}
        },
        "additionalProperties": false
    })
}

fn fields_schema(required: bool) -> Value {
    let mut properties = common_properties();
    properties.insert(
        "fields".to_string(),
        json!({
            "type": "array", "minItems": 1, "maxItems": 50, "items": field_items_schema()
        }),
    );
    json!({
        "type": "object", "properties": properties,
        "required": if required { json!(["fields"]) } else { json!([]) },
        "additionalProperties": false
    })
}

fn choice_block_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "multiple": {"type": "boolean"},
            "options": {"type": "array", "minItems": 1, "maxItems": 60, "items": {
                "type": "object", "properties": {
                    "value": {"type": "string", "minLength": 1},
                    "label": {"type": "string"}, "description": {"type": "string"}
                }, "required": ["value"], "additionalProperties": false
            }},
            "default": {},
            "min_selections": {"type": "integer", "minimum": 0, "maximum": 60},
            "max_selections": {"type": "integer", "minimum": 1, "maximum": 60}
        },
        "required": ["options"], "additionalProperties": false
    })
}

fn choice_schema() -> Value {
    let mut properties = common_properties();
    let Value::Object(choice) = choice_block_schema() else {
        unreachable!()
    };
    let choice_properties = choice
        .get("properties")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let Value::Object(choice_properties) = choice_properties else {
        unreachable!()
    };
    properties.extend(choice_properties);
    json!({
        "type": "object", "properties": properties,
        "required": ["options"], "additionalProperties": false
    })
}

fn mixed_schema() -> Value {
    let mut properties = common_properties();
    properties.insert(
        "fields".to_string(),
        json!({
            "type": "array", "maxItems": 50, "items": field_items_schema()
        }),
    );
    properties.insert("choice".to_string(), choice_block_schema());
    json!({"type": "object", "properties": properties, "additionalProperties": false})
}
