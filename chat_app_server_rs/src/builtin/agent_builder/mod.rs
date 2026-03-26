mod profile;
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::memory_server_client::{
    self, CreateMemoryAgentRequestDto, UpdateMemoryAgentRequestDto,
};

use self::profile::recommend_profile;
use self::support::{
    normalize_optional_string, normalize_tool_name, optional_object_value, optional_skill_array,
    optional_string, optional_string_array, required_string, truncate_text,
};

#[derive(Debug, Clone)]
pub struct AgentBuilderOptions {
    pub server_name: String,
    pub user_id: Option<String>,
}

#[derive(Clone)]
pub struct AgentBuilderService {
    tools: HashMap<String, Tool>,
    default_user_id: Option<String>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

impl AgentBuilderService {
    pub fn new(opts: AgentBuilderOptions) -> Result<Self, String> {
        let default_user_id = normalize_optional_string(opts.user_id);
        let mut service = Self {
            tools: HashMap::new(),
            default_user_id,
        };

        let server_name = opts.server_name;
        service.register_recommend_agent_profile(server_name.as_str());
        service.register_list_available_skills();
        service.register_create_memory_agent();
        service.register_update_memory_agent();
        service.register_preview_agent_context();

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
        _session_id: Option<&str>,
        _conversation_turn_id: Option<&str>,
        _on_stream_chunk: Option<crate::core::mcp_tools::ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let normalized = normalize_tool_name(name);
        let tool = self
            .tools
            .get(normalized.as_str())
            .ok_or_else(|| format!("Unknown tool: {}", name))?;
        (tool.handler)(args, self.default_user_id.as_deref())
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

    fn register_recommend_agent_profile(&mut self, server_name: &str) {
        self.register_tool(
            "recommend_agent_profile",
            &format!(
                "Analyze user intent and propose an agent profile (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "requirement": { "type": "string" }
                },
                "required": ["requirement"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let requirement = required_string(&args, "requirement")?;
                let recommendation = recommend_profile(requirement.as_str());
                Ok(text_result(json!(recommendation)))
            }),
        );
    }

    fn register_list_available_skills(&mut self) {
        self.register_tool(
            "list_available_skills",
            "List available skills from Memory agents for the current user.",
            json!({
                "type": "object",
                "properties": {
                    "user_id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, default_user_id| {
                let user_id = optional_string(&args, "user_id")
                    .or_else(|| default_user_id.map(|value| value.to_string()));
                let result = block_on_result(async move {
                    let agents = memory_server_client::list_memory_agents(
                        user_id.as_deref(),
                        Some(true),
                        Some(300),
                        0,
                    )
                    .await?;
                    let mut skill_map: HashMap<String, Value> = HashMap::new();
                    for agent in agents {
                        for skill in agent.skills {
                            let skill_id = skill.id.trim().to_string();
                            if skill_id.is_empty() {
                                continue;
                            }
                            skill_map.entry(skill_id.clone()).or_insert_with(|| {
                                json!({
                                    "id": skill_id,
                                    "name": skill.name,
                                    "content_preview": truncate_text(skill.content.as_str(), 400),
                                    "source": "memory_agent_embedded",
                                })
                            });
                        }
                        for skill_id in agent.skill_ids {
                            let normalized = skill_id.trim().to_string();
                            if normalized.is_empty() {
                                continue;
                            }
                            skill_map.entry(normalized.clone()).or_insert_with(|| {
                                json!({
                                    "id": normalized,
                                    "name": "",
                                    "content_preview": "",
                                    "source": "memory_agent_reference",
                                })
                            });
                        }
                    }
                    let mut items = skill_map.into_values().collect::<Vec<_>>();
                    items.sort_by(|left, right| {
                        let left_id = left.get("id").and_then(Value::as_str).unwrap_or("");
                        let right_id = right.get("id").and_then(Value::as_str).unwrap_or("");
                        left_id.cmp(right_id)
                    });
                    Ok::<Value, String>(json!({
                        "items": items,
                        "count": items.len(),
                    }))
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_create_memory_agent(&mut self) {
        self.register_tool(
            "create_memory_agent",
            "Create a Memory agent with role definition and skills.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "role_definition": { "type": "string" },
                    "description": { "type": "string" },
                    "category": { "type": "string" },
                    "user_id": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "plugin_sources": { "type": "array", "items": { "type": "string" } },
                    "skill_ids": { "type": "array", "items": { "type": "string" } },
                    "default_skill_ids": { "type": "array", "items": { "type": "string" } },
                    "skills": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["id", "name", "content"],
                            "additionalProperties": false
                        }
                    },
                    "mcp_policy": { "type": "object" },
                    "project_policy": { "type": "object" }
                },
                "required": ["name", "role_definition"],
                "additionalProperties": false
            }),
            Arc::new(move |args, default_user_id| {
                let name = required_string(&args, "name")?;
                let role_definition = required_string(&args, "role_definition")?;
                let user_id = optional_string(&args, "user_id")
                    .or_else(|| default_user_id.map(|value| value.to_string()));
                let payload = CreateMemoryAgentRequestDto {
                    user_id,
                    name,
                    description: optional_string(&args, "description"),
                    category: optional_string(&args, "category"),
                    role_definition,
                    plugin_sources: optional_string_array(&args, "plugin_sources"),
                    skills: optional_skill_array(&args, "skills"),
                    skill_ids: optional_string_array(&args, "skill_ids"),
                    default_skill_ids: optional_string_array(&args, "default_skill_ids"),
                    mcp_policy: optional_object_value(&args, "mcp_policy"),
                    project_policy: optional_object_value(&args, "project_policy"),
                    enabled: args.get("enabled").and_then(Value::as_bool),
                };

                let created = block_on_result(memory_server_client::create_memory_agent(&payload))?;
                Ok(text_result(json!({
                    "created": true,
                    "agent": created,
                })))
            }),
        );
    }

    fn register_update_memory_agent(&mut self) {
        self.register_tool(
            "update_memory_agent",
            "Update an existing Memory agent configuration.",
            json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "name": { "type": "string" },
                    "role_definition": { "type": "string" },
                    "description": { "type": "string" },
                    "category": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "plugin_sources": { "type": "array", "items": { "type": "string" } },
                    "skill_ids": { "type": "array", "items": { "type": "string" } },
                    "default_skill_ids": { "type": "array", "items": { "type": "string" } },
                    "skills": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["id", "name", "content"],
                            "additionalProperties": false
                        }
                    },
                    "mcp_policy": { "type": "object" },
                    "project_policy": { "type": "object" }
                },
                "required": ["agent_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let agent_id = required_string(&args, "agent_id")?;
                let payload = UpdateMemoryAgentRequestDto {
                    name: optional_string(&args, "name"),
                    description: optional_string(&args, "description"),
                    category: optional_string(&args, "category"),
                    role_definition: optional_string(&args, "role_definition"),
                    plugin_sources: optional_string_array(&args, "plugin_sources"),
                    skills: optional_skill_array(&args, "skills"),
                    skill_ids: optional_string_array(&args, "skill_ids"),
                    default_skill_ids: optional_string_array(&args, "default_skill_ids"),
                    mcp_policy: optional_object_value(&args, "mcp_policy"),
                    project_policy: optional_object_value(&args, "project_policy"),
                    enabled: args.get("enabled").and_then(Value::as_bool),
                };

                let updated = block_on_result(memory_server_client::update_memory_agent(
                    agent_id.as_str(),
                    &payload,
                ))?;
                match updated {
                    Some(agent) => Ok(text_result(json!({
                        "updated": true,
                        "agent": agent,
                    }))),
                    None => Err(format!("agent not found: {}", agent_id)),
                }
            }),
        );
    }

    fn register_preview_agent_context(&mut self) {
        self.register_tool(
            "preview_agent_context",
            "Preview final runtime context text from role and skills.",
            json!({
                "type": "object",
                "properties": {
                    "role_definition": { "type": "string" },
                    "skills": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["id", "name", "content"],
                            "additionalProperties": false
                        }
                    },
                    "plugin_sources": { "type": "array", "items": { "type": "string" } },
                    "skill_ids": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["role_definition"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let role_definition = required_string(&args, "role_definition")?;
                let skills = optional_skill_array(&args, "skills").unwrap_or_default();
                let plugin_sources =
                    optional_string_array(&args, "plugin_sources").unwrap_or_default();
                let skill_ids = optional_string_array(&args, "skill_ids").unwrap_or_default();
                let mut text = String::new();
                text.push_str("角色定义:\n");
                text.push_str(role_definition.as_str());
                if !plugin_sources.is_empty() {
                    text.push_str("\n\n插件范围: ");
                    text.push_str(plugin_sources.join(", ").as_str());
                }
                if !skills.is_empty() {
                    text.push_str("\n\n技能上下文:\n");
                    for (index, skill) in skills.iter().enumerate() {
                        text.push_str(
                            format!("{}. {} ({})\n", index + 1, skill.name, skill.id).as_str(),
                        );
                        text.push_str(skill.content.as_str());
                        text.push_str("\n");
                    }
                }
                if !skill_ids.is_empty() {
                    text.push_str("\n技能引用ID: ");
                    text.push_str(skill_ids.join(", ").as_str());
                }
                Ok(text_result(json!({
                    "preview": text,
                    "role_definition_chars": role_definition.chars().count(),
                    "plugin_sources_count": plugin_sources.len(),
                    "skills_count": skills.len(),
                    "skill_ids_count": skill_ids.len(),
                })))
            }),
        );
    }
}
