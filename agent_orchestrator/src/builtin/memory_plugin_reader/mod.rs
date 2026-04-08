use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::memory_server_client;

#[derive(Debug, Clone)]
pub struct MemoryPluginReaderOptions {
    pub server_name: String,
    pub agent_id: String,
}

#[derive(Clone)]
pub struct MemoryPluginReaderService {
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

fn plugin_ref(index: usize) -> String {
    format!("PL{}", index + 1)
}

impl MemoryPluginReaderService {
    pub fn new(opts: MemoryPluginReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };
        service.register_get_plugin_detail(opts.server_name.as_str(), opts.agent_id.as_str());
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

    fn register_get_plugin_detail(&mut self, server_name: &str, agent_id: &str) {
        let bound_agent_id = agent_id.trim().to_string();
        self.register_tool(
            "get_plugin_detail",
            &format!(
                "Read the full content of a plugin that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "plugin_ref": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["plugin_ref"]
            }),
            Arc::new(move |args| {
                let requested_plugin_ref = args
                    .get("plugin_ref")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "missing required field: plugin_ref".to_string())?
                    .to_string();
                let expected = normalize_lookup_token(requested_plugin_ref.as_str());
                let agent_id = bound_agent_id.clone();

                let payload = block_on_result(async move {
                    let runtime_context =
                        memory_server_client::get_memory_agent_runtime_context(agent_id.as_str())
                            .await?
                            .ok_or_else(|| format!("agent runtime context not found: {}", agent_id))?;

                    let mut resolved_source: Option<String> = None;
                    let mut resolved_plugin_ref: Option<String> = None;
                    for (index, plugin) in runtime_context.runtime_plugins.iter().enumerate() {
                        let current_ref = plugin_ref(index);
                        if normalize_lookup_token(current_ref.as_str()) == expected {
                            let source = plugin.source.trim();
                            if source.is_empty() {
                                break;
                            }
                            resolved_source = Some(source.to_string());
                            resolved_plugin_ref = Some(current_ref);
                            break;
                        }
                    }

                    let resolved_source = resolved_source.ok_or_else(|| {
                        format!(
                            "plugin_ref does not belong to current contact agent: {}",
                            requested_plugin_ref
                        )
                    })?;
                    let plugin = memory_server_client::get_memory_skill_plugin(resolved_source.as_str())
                        .await?
                        .ok_or_else(|| format!("plugin not found: {}", resolved_source))?;
                    let runtime_entry = runtime_context
                        .runtime_plugins
                        .iter()
                        .find(|item| item.source.trim() == resolved_source.as_str());
                    let related_skills = runtime_context
                        .runtime_skills
                        .iter()
                        .filter(|item| {
                            item.plugin_source
                                .as_deref()
                                .map(str::trim)
                                .map(|value| value == resolved_source.as_str())
                                .unwrap_or(false)
                        })
                        .map(|item| {
                            json!({
                                "id": item.id,
                                "name": item.name,
                                "description": item.description,
                                "source_type": item.source_type,
                                "source_path": item.source_path,
                                "updated_at": item.updated_at,
                            })
                        })
                        .collect::<Vec<_>>();

                    Ok::<Value, String>(json!({
                        "agent_id": agent_id,
                        "plugin_ref": resolved_plugin_ref,
                        "source": plugin.source,
                        "name": plugin.name,
                        "category": runtime_entry.and_then(|item| item.category.clone()).or(plugin.category),
                        "description": runtime_entry.and_then(|item| item.description.clone()).or(plugin.description),
                        "version": plugin.version,
                        "repository": plugin.repository,
                        "branch": plugin.branch,
                        "content": plugin.content,
                        "commands": plugin.commands,
                        "command_count": plugin.command_count,
                        "related_skills": related_skills,
                        "updated_at": runtime_entry
                            .and_then(|item| item.updated_at.clone())
                            .or(Some(plugin.updated_at)),
                    }))
                })?;

                Ok(text_result(payload))
            }),
        );
    }
}
