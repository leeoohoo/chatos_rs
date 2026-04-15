mod prompt_flow;
mod schema;
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::mcp_tools::ToolStreamChunkCallback;

use self::prompt_flow::{
    handle_prompt_choices, handle_prompt_key_values, handle_prompt_mixed_form,
};
use self::schema::{choice_schema, kv_schema, mixed_schema};
use self::support::trimmed_non_empty;

#[derive(Debug, Clone)]
pub struct UiPrompterOptions {
    pub server_name: String,
    pub prompt_timeout_ms: u64,
}

#[derive(Clone)]
pub struct UiPrompterService {
    tools: HashMap<String, Tool>,
    default_conversation_id: String,
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

pub(super) struct ToolContext<'a> {
    conversation_id: &'a str,
    conversation_turn_id: &'a str,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
}

impl UiPrompterService {
    pub fn new(opts: UiPrompterOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
            default_conversation_id: format!("conversation_{}", Uuid::new_v4().simple()),
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
        conversation_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;

        let conversation = conversation_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_conversation_id.as_str());
        let turn = conversation_turn_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_turn_id.as_str());

        let ctx = ToolContext {
            conversation_id: conversation,
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
