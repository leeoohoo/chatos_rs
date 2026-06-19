use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tool_registry::{block_on_result, text_result, ToolRegistry};

mod payload;
mod schema;
#[cfg(test)]
mod tests;

use self::payload::{
    build_mixed_choice_input, build_mixed_payload_map, choice_to_value, kv_fields_to_value,
    normalize_choice_limits, normalize_choice_options, normalize_default_selection,
    normalize_kv_fields, parse_choice_block, parse_i64, parse_mixed_fields,
};
pub use self::payload::{ChoiceLimits, ChoiceOption, KvField};
use self::schema::{choice_schema, kv_schema, mixed_schema};

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

fn default_timeout_ms() -> u64 {
    UI_PROMPT_TIMEOUT_MS_DEFAULT
}

fn default_allow_cancel() -> bool {
    true
}
