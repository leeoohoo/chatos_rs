use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::memory_server_client;

#[derive(Debug, Clone)]
pub struct MemorySkillReaderOptions {
    pub server_name: String,
    pub agent_id: String,
}

#[derive(Clone)]
pub struct MemorySkillReaderService {
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

impl MemorySkillReaderService {
    pub fn new(opts: MemorySkillReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };
        service.register_get_skill_detail(opts.server_name.as_str(), opts.agent_id.as_str());
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

    fn register_get_skill_detail(&mut self, server_name: &str, agent_id: &str) {
        let bound_agent_id = agent_id.trim().to_string();
        self.register_tool(
            "get_skill_detail",
            &format!(
                "Read the full content of a skill that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "skill_id": { "type": "string" }
                },
                "required": ["skill_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let skill_id = args
                    .get("skill_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "missing required field: skill_id".to_string())?
                    .to_string();
                let agent_id = bound_agent_id.clone();

                let payload = block_on_result(async move {
                    let runtime_context =
                        memory_server_client::get_memory_agent_runtime_context(agent_id.as_str())
                            .await?
                            .ok_or_else(|| format!("agent runtime context not found: {}", agent_id))?;

                    if let Some(skill) = runtime_context
                        .skills
                        .iter()
                        .find(|skill| skill.id.trim() == skill_id)
                    {
                        return Ok::<Value, String>(json!({
                            "agent_id": agent_id,
                            "skill_id": skill.id.clone(),
                            "name": skill.name.clone(),
                            "description": Value::Null,
                            "content": skill.content.clone(),
                            "plugin_source": Value::Null,
                            "source_path": Value::Null,
                            "source_type": "inline",
                            "updated_at": runtime_context.updated_at.clone(),
                        }));
                    }

                    let runtime_skill = runtime_context
                        .runtime_skills
                        .iter()
                        .find(|skill| skill.id.trim() == skill_id)
                        .ok_or_else(|| {
                            format!(
                                "skill_id does not belong to current contact agent: {}",
                                skill_id
                            )
                        })?;

                    let full_skill = memory_server_client::get_memory_skill(skill_id.as_str())
                        .await?
                        .ok_or_else(|| format!("skill not found: {}", skill_id))?;

                    Ok::<Value, String>(json!({
                        "agent_id": agent_id,
                        "skill_id": full_skill.id,
                        "name": full_skill.name,
                        "description": full_skill.description,
                        "content": full_skill.content,
                        "plugin_source": runtime_skill.plugin_source.clone().or(Some(full_skill.plugin_source.clone())),
                        "source_path": runtime_skill.source_path.clone().or(Some(full_skill.source_path.clone())),
                        "source_type": runtime_skill.source_type.clone(),
                        "updated_at": runtime_skill.updated_at.clone().or(Some(full_skill.updated_at.clone())),
                    }))
                })?;

                Ok(text_result(payload))
            }),
        );
    }
}
