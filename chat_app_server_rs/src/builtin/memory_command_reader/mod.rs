use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::memory_server_client;

#[derive(Debug, Clone)]
pub struct MemoryCommandReaderOptions {
    pub server_name: String,
    pub agent_id: String,
}

#[derive(Clone)]
pub struct MemoryCommandReaderService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

fn normalize_lookup_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

impl MemoryCommandReaderService {
    pub fn new(opts: MemoryCommandReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };
        service.register_get_command_detail(opts.server_name.as_str(), opts.agent_id.as_str());
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

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
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

    fn register_get_command_detail(&mut self, server_name: &str, agent_id: &str) {
        let bound_agent_id = agent_id.trim().to_string();
        self.register_tool(
            "get_command_detail",
            &format!(
                "Read the full content of a command that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "command_ref": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["command_ref"]
            }),
            Arc::new(move |args| {
                let requested_command_ref = args
                    .get("command_ref")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "missing required field: command_ref".to_string())?
                    .to_string();
                let expected = normalize_lookup_token(requested_command_ref.as_str());
                let agent_id = bound_agent_id.clone();

                let payload = block_on_result(async move {
                    let runtime_context =
                        memory_server_client::get_memory_agent_runtime_context(agent_id.as_str())
                            .await?
                            .ok_or_else(|| format!("agent runtime context not found: {}", agent_id))?;
                    let command = runtime_context
                        .runtime_commands
                        .iter()
                        .find(|item| normalize_lookup_token(item.command_ref.as_str()) == expected)
                        .ok_or_else(|| {
                            format!(
                                "command_ref does not belong to current contact agent: {}",
                                requested_command_ref
                            )
                        })?;

                    Ok::<Value, String>(json!({
                        "agent_id": agent_id,
                        "command_ref": command.command_ref.clone(),
                        "name": command.name.clone(),
                        "description": command.description.clone(),
                        "argument_hint": command.argument_hint.clone(),
                        "plugin_source": command.plugin_source.clone(),
                        "source_path": command.source_path.clone(),
                        "content": command.content.clone(),
                        "updated_at": command.updated_at.clone(),
                    }))
                })?;

                Ok(text_result(payload))
            }),
        );
    }
}
