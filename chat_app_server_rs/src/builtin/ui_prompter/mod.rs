use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::core::async_bridge::block_on_result;
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::core::tool_io::text_result;
use crate::services::ui_prompt_manager::{
    create_ui_prompt_record, create_ui_prompt_request, normalize_choice_limits,
    normalize_choice_options, normalize_default_selection, normalize_kv_fields,
    redact_response_for_store, update_ui_prompt_response, wait_for_ui_prompt_decision,
    ChoiceLimits, ChoiceOption, KvField, LimitMode, UiPromptDecision, UiPromptPayload,
    UiPromptResponseSubmission, UiPromptStatus, UI_PROMPT_TIMEOUT_ERR,
};
use crate::utils::events::Events;

#[derive(Debug, Clone)]
pub struct UiPrompterOptions {
    pub server_name: String,
    pub prompt_timeout_ms: u64,
}

#[derive(Clone)]
pub struct UiPrompterService {
    tools: HashMap<String, Tool>,
    default_session_id: String,
    default_turn_id: String,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

struct ToolContext<'a> {
    session_id: &'a str,
    conversation_turn_id: &'a str,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
}

impl UiPrompterService {
    pub fn new(opts: UiPrompterOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
            default_session_id: format!("session_{}", Uuid::new_v4().simple()),
            default_turn_id: format!("turn_{}", Uuid::new_v4().simple()),
        };

        let timeout_ms = opts.prompt_timeout_ms.clamp(10_000, 86_400_000);
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
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })
            })
            .collect()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;

        let session = session_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_session_id.as_str());
        let turn = conversation_turn_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_turn_id.as_str());

        let ctx = ToolContext {
            session_id: session,
            conversation_turn_id: turn,
            on_stream_chunk,
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
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
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
        session_id: ctx.session_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
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
        LimitMode::Clamp,
        parse_i64(args.get("single_min_selections")),
        parse_i64(args.get("single_max_selections")),
    )?;
    let default_selection =
        normalize_default_selection(args.get("default"), multiple, options.as_slice());

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        session_id: ctx.session_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
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
    let fields = match args.get("fields") {
        Some(Value::Array(_)) => normalize_kv_fields(args.get("fields"), 50)?,
        Some(_) => return Err("fields must be an array".to_string()),
        None => Vec::new(),
    };

    let choice_value = args
        .get("choice")
        .and_then(Value::as_object)
        .cloned()
        .map(Value::Object)
        .or_else(|| {
            if args.get("options").is_some() {
                Some(args.clone())
            } else {
                None
            }
        });
    let choice = parse_choice_block(choice_value.as_ref())?;

    if fields.is_empty() && choice.is_none() {
        return Err("mixed form requires fields and/or choice".to_string());
    }

    let mut payload_map = Map::new();
    if !fields.is_empty() {
        payload_map.insert(
            "fields".to_string(),
            Value::Array(kv_fields_to_value(fields.as_slice())),
        );
    }
    if let Some((multiple, options, limits, default_selection)) = choice {
        payload_map.insert(
            "choice".to_string(),
            choice_to_value(multiple, options.as_slice(), &limits, default_selection),
        );
    }

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        session_id: ctx.session_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
        tool_call_id: None,
        kind: "mixed".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: Value::Object(payload_map),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "values": decision.response.values.unwrap_or_else(|| json!({})),
        "selection": decision.response.selection.unwrap_or(Value::Null),
    })))
}

fn execute_prompt(payload: UiPromptPayload, ctx: &ToolContext) -> Result<UiPromptDecision, String> {
    block_on_result(create_ui_prompt_record(&payload))?;

    let (registered_payload, receiver) =
        block_on_result(create_ui_prompt_request(payload.clone()))?;
    emit_ui_prompt_required_event(ctx.on_stream_chunk.as_ref(), &registered_payload);

    let decision = match block_on_result(wait_for_ui_prompt_decision(
        registered_payload.prompt_id.as_str(),
        receiver,
        registered_payload.timeout_ms,
    )) {
        Ok(decision) => decision,
        Err(err) if err == UI_PROMPT_TIMEOUT_ERR => {
            let timeout_response = UiPromptResponseSubmission {
                status: UiPromptStatus::Timeout.as_str().to_string(),
                values: None,
                selection: None,
                reason: Some("timeout".to_string()),
            };
            let _ = block_on_result(update_ui_prompt_response(
                registered_payload.prompt_id.as_str(),
                UiPromptStatus::Timeout,
                Some(json!({
                    "status": "timeout",
                })),
            ));
            emit_ui_prompt_resolved_event(
                ctx.on_stream_chunk.as_ref(),
                registered_payload.prompt_id.as_str(),
                UiPromptStatus::Timeout,
            );
            return Ok(UiPromptDecision {
                status: UiPromptStatus::Timeout,
                response: timeout_response,
            });
        }
        Err(err) => return Err(err),
    };

    let redacted_response = redact_response_for_store(&decision.response, &registered_payload);
    let _ = block_on_result(update_ui_prompt_response(
        registered_payload.prompt_id.as_str(),
        decision.status,
        Some(redacted_response),
    ));
    emit_ui_prompt_resolved_event(
        ctx.on_stream_chunk.as_ref(),
        registered_payload.prompt_id.as_str(),
        decision.status,
    );
    Ok(decision)
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
        LimitMode::Clamp,
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

fn emit_ui_prompt_required_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    payload: &UiPromptPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::UI_PROMPT_REQUIRED,
        "data": payload,
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
}

fn emit_ui_prompt_resolved_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    prompt_id: &str,
    status: UiPromptStatus,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::UI_PROMPT_RESOLVED,
        "data": {
            "prompt_id": prompt_id,
            "status": status.as_str(),
        }
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
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
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "required": ["key"],
                    "additionalProperties": false
                }
            },
            "allow_cancel": { "type": "boolean" },
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
            "default": {
                "type": "string"
            },
            "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
            "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 },
            "allow_cancel": { "type": "boolean" },
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
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "required": ["key"],
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
                    "default": {
                        "type": "string"
                    },
                    "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
                    "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 }
                },
                "required": ["options"],
                "additionalProperties": false
            },
            "allow_cancel": { "type": "boolean" },
        },
        "additionalProperties": false
    })
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

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
