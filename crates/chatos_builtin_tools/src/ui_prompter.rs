use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::tool_registry::{block_on_result, text_result, ToolRegistry};

pub const UI_PROMPT_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;

pub type UiPromptStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptPayload {
    pub prompt_id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default = "default_allow_cancel")]
    pub allow_cancel: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptResponseSubmission {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPromptDecision {
    pub status: String,
    pub response: UiPromptResponseSubmission,
}

#[async_trait]
pub trait UiPrompterStore: Send + Sync {
    async fn execute_prompt(
        &self,
        payload: UiPromptPayload,
        on_stream_chunk: Option<UiPromptStreamChunkCallback>,
    ) -> Result<UiPromptDecision, String>;
}

#[derive(Clone)]
pub struct UiPrompterStoreRef(Arc<dyn UiPrompterStore>);

impl UiPrompterStoreRef {
    pub fn new(store: Arc<dyn UiPrompterStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn UiPrompterStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for UiPrompterStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiPrompterStoreRef").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct UiPrompterOptions {
    pub server_name: String,
    pub prompt_timeout_ms: u64,
    pub store: UiPrompterStoreRef,
}

#[derive(Clone)]
pub struct UiPrompterService {
    registry: ToolRegistry<ToolHandler>,
    default_conversation_id: String,
    default_turn_id: String,
    store: UiPrompterStoreRef,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

struct ToolContext {
    conversation_id: String,
    conversation_turn_id: String,
    on_stream_chunk: Option<UiPromptStreamChunkCallback>,
    store: UiPrompterStoreRef,
}

impl UiPrompterService {
    pub fn new(opts: UiPrompterOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
            default_conversation_id: format!("conversation_{}", Uuid::new_v4().simple()),
            default_turn_id: format!("turn_{}", Uuid::new_v4().simple()),
            store: opts.store,
        };

        let timeout_ms = opts
            .prompt_timeout_ms
            .clamp(10_000, UI_PROMPT_TIMEOUT_MS_DEFAULT);
        let server_name = opts.server_name;

        service.register_tool(
            "prompt_key_values",
            &format!(
                "Prompt user for key/value input fields and wait for submission (server: {server_name})."
            ),
            kv_schema(),
            Arc::new(move |args, ctx| handle_prompt_key_values(args, ctx, timeout_ms)),
        );
        service.register_tool(
            "prompt_choices",
            "Prompt user for a single or multi-choice selection and wait for submission.",
            choice_schema(),
            Arc::new(move |args, ctx| handle_prompt_choices(args, ctx, timeout_ms)),
        );
        service.register_tool(
            "prompt_mixed_form",
            "Prompt user with mixed form fields and optional choice selection in one interaction.",
            mixed_schema(),
            Arc::new(move |args, ctx| handle_prompt_mixed_form(args, ctx, timeout_ms)),
        );

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<UiPromptStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        let conversation = conversation_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_conversation_id.as_str());
        let turn = conversation_turn_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_turn_id.as_str());
        let ctx = ToolContext {
            conversation_id: conversation.to_string(),
            conversation_turn_id: turn.to_string(),
            on_stream_chunk,
            store: self.store.clone(),
        };
        (tool.handler)(args, &ctx)
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.registry
            .register_tool(name, description, input_schema, handler);
    }
}

fn handle_prompt_key_values(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let fields = normalize_kv_fields(args.get("fields"), 50)?;
    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.clone(),
        conversation_turn_id: ctx.conversation_turn_id.clone(),
        tool_call_id: None,
        kind: "kv".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: json!({
            "fields": kv_fields_to_value(fields.as_slice()),
        }),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "values": decision.response.values.unwrap_or_else(|| json!({})),
    })))
}

fn handle_prompt_choices(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let multiple = args
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let options = normalize_choice_options(args.get("options"), 60)?;
    let limits = normalize_choice_limits(
        multiple,
        parse_i64(args.get("min_selections")),
        parse_i64(args.get("max_selections")),
        options.len(),
        parse_i64(args.get("single_min_selections")),
        parse_i64(args.get("single_max_selections")),
    )?;
    let default_selection =
        normalize_default_selection(args.get("default"), multiple, options.as_slice());

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.clone(),
        conversation_turn_id: ctx.conversation_turn_id.clone(),
        tool_call_id: None,
        kind: "choice".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: json!({
            "choice": choice_to_value(multiple, options.as_slice(), &limits, default_selection),
        }),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "selection": decision.response.selection.unwrap_or_else(|| {
            if multiple {
                Value::Array(Vec::new())
            } else {
                Value::String(String::new())
            }
        }),
    })))
}

fn handle_prompt_mixed_form(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let fields = parse_mixed_fields(&args)?;
    let choice = parse_choice_block(build_mixed_choice_input(&args).as_ref())?;

    if fields.is_empty() && choice.is_none() {
        return Err("mixed form requires fields and/or choice".to_string());
    }

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.clone(),
        conversation_turn_id: ctx.conversation_turn_id.clone(),
        tool_call_id: None,
        kind: "mixed".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: Value::Object(build_mixed_payload_map(fields.as_slice(), choice)),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "values": decision.response.values.unwrap_or_else(|| json!({})),
        "selection": decision.response.selection.unwrap_or(Value::Null),
    })))
}

fn execute_prompt(payload: UiPromptPayload, ctx: &ToolContext) -> Result<UiPromptDecision, String> {
    block_on_result(
        ctx.store
            .inner()
            .execute_prompt(payload, ctx.on_stream_chunk.clone()),
    )
}

fn kv_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "fields": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "minLength": 1 },
                        "name": { "type": "string", "minLength": 1 },
                        "id": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }
            },
            "allow_cancel": { "type": "boolean" }
        },
        "required": ["fields"],
        "additionalProperties": false
    })
}

fn choice_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "multiple": { "type": "boolean" },
            "options": {
                "type": "array",
                "minItems": 1,
                "maxItems": 60,
                "items": {
                    "type": "object",
                    "properties": {
                        "value": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" }
                    },
                    "required": ["value"],
                    "additionalProperties": false
                }
            },
            "default": { "type": "string" },
            "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
            "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 },
            "allow_cancel": { "type": "boolean" }
        },
        "required": ["options"],
        "additionalProperties": false
    })
}

fn mixed_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "fields": {
                "type": "array",
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "minLength": 1 },
                        "name": { "type": "string", "minLength": 1 },
                        "id": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }
            },
            "choice": {
                "type": "object",
                "properties": {
                    "multiple": { "type": "boolean" },
                    "options": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 60,
                        "items": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "string", "minLength": 1 },
                                "label": { "type": "string" },
                                "description": { "type": "string" }
                            },
                            "required": ["value"],
                            "additionalProperties": false
                        }
                    },
                    "default": { "type": "string" },
                    "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
                    "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 }
                },
                "required": ["options"],
                "additionalProperties": false
            },
            "allow_cancel": { "type": "boolean" }
        },
        "additionalProperties": false
    })
}

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

fn normalize_choice_options(
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

fn normalize_choice_limits(
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

fn normalize_default_selection(
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

fn normalize_kv_fields(value: Option<&Value>, max_fields: usize) -> Result<Vec<KvField>, String> {
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

fn parse_choice_block(
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
        parse_i64(choice_input.get("single_min_selections")),
        parse_i64(choice_input.get("single_max_selections")),
    )?;
    let default_selection =
        normalize_default_selection(choice_input.get("default"), multiple, options.as_slice());
    Ok(Some((multiple, options, limits, default_selection)))
}

fn kv_fields_to_value(fields: &[KvField]) -> Vec<Value> {
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

fn choice_to_value(
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

fn parse_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|raw| raw as i64)))
}

fn optional_string(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn make_prompt_id() -> String {
    format!("up_{}", Uuid::new_v4().simple())
}

fn trimmed(value: Option<&str>) -> String {
    value
        .map(|item| item.trim().to_string())
        .unwrap_or_default()
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn build_mixed_choice_input(args: &Value) -> Option<Value> {
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

fn parse_mixed_fields(args: &Value) -> Result<Vec<KvField>, String> {
    match args.get("fields") {
        Some(Value::Array(_)) => normalize_kv_fields(args.get("fields"), 50),
        Some(_) => Err("fields must be an array".to_string()),
        None => Ok(Vec::new()),
    }
}

fn build_mixed_payload_map(
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

fn default_timeout_ms() -> u64 {
    UI_PROMPT_TIMEOUT_MS_DEFAULT
}

fn default_allow_cancel() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    #[derive(Debug, Clone)]
    struct EchoPromptStore;

    #[async_trait]
    impl UiPrompterStore for EchoPromptStore {
        async fn execute_prompt(
            &self,
            payload: UiPromptPayload,
            _on_stream_chunk: Option<UiPromptStreamChunkCallback>,
        ) -> Result<UiPromptDecision, String> {
            Ok(UiPromptDecision {
                status: "ok".to_string(),
                response: UiPromptResponseSubmission {
                    status: "ok".to_string(),
                    values: Some(payload.payload),
                    selection: Some(Value::String("yes".to_string())),
                    reason: None,
                },
            })
        }
    }

    fn test_service() -> UiPrompterService {
        UiPrompterService::new(UiPrompterOptions {
            server_name: "ui_prompter".to_string(),
            prompt_timeout_ms: 120_000,
            store: UiPrompterStoreRef::new(Arc::new(EchoPromptStore)),
        })
        .expect("ui prompter should initialize")
    }

    #[test]
    fn ui_prompter_lists_expected_tools() {
        let tools = test_service().list_tools();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(names.contains(&"prompt_key_values"));
        assert!(names.contains(&"prompt_choices"));
        assert!(names.contains(&"prompt_mixed_form"));
    }

    #[test]
    fn mixed_form_requires_fields_or_choice() {
        let err = test_service()
            .call_tool(
                "prompt_mixed_form",
                json!({ "title": "Empty" }),
                Some("session_1"),
                Some("turn_1"),
                None,
            )
            .expect_err("mixed form should reject empty payload");
        assert!(err.contains("fields and/or choice"));
    }

    #[test]
    fn key_value_prompt_normalizes_fields() {
        let result = test_service()
            .call_tool(
                "prompt_key_values",
                json!({
                    "fields": [
                        { "label": "API Token", "secret": true },
                        { "name": "repo", "default": "main" }
                    ]
                }),
                Some("session_1"),
                Some("turn_1"),
                None,
            )
            .expect("prompt should execute");
        let structured = result
            .get("_structured_result")
            .and_then(|value| value.get("values"))
            .expect("structured values");
        let fields = structured
            .get("fields")
            .and_then(Value::as_array)
            .expect("fields payload");
        assert_eq!(
            fields[0].get("key").and_then(Value::as_str),
            Some("api_token")
        );
        assert_eq!(fields[1].get("key").and_then(Value::as_str), Some("repo"));
    }
}
