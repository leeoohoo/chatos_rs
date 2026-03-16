use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::memory_server_client::{
    self, CreateMemoryAgentRequestDto, MemoryAgentSkillDto, UpdateMemoryAgentRequestDto,
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
        service.register_tool(
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

        service.register_tool(
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

        service.register_tool(
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

        service.register_tool(
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

        service.register_tool(
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
                    "skill_ids": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["role_definition"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let role_definition = required_string(&args, "role_definition")?;
                let skills = optional_skill_array(&args, "skills").unwrap_or_default();
                let skill_ids = optional_string_array(&args, "skill_ids").unwrap_or_default();
                let mut text = String::new();
                text.push_str("角色定义:\n");
                text.push_str(role_definition.as_str());
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
                    "skills_count": skills.len(),
                    "skill_ids_count": skill_ids.len(),
                })))
            }),
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
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

fn required_string(args: &Value, key: &str) -> Result<String, String> {
    optional_string(args, key).ok_or_else(|| format!("missing required field: {}", key))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_string_array(args: &Value, key: &str) -> Option<Vec<String>> {
    let values = args.get(key)?.as_array()?;
    let mut out = Vec::new();
    for value in values {
        let Some(item) = value.as_str() else {
            continue;
        };
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Some(out)
}

fn optional_skill_array(args: &Value, key: &str) -> Option<Vec<MemoryAgentSkillDto>> {
    let values = args.get(key)?.as_array()?;
    let mut out = Vec::new();
    for item in values {
        let Some(object) = item.as_object() else {
            continue;
        };
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(id), Some(name), Some(content)) = (id, name, content) else {
            continue;
        };
        out.push(MemoryAgentSkillDto { id, name, content });
    }
    Some(out)
}

fn optional_object_value(args: &Value, key: &str) -> Option<Value> {
    let value = args.get(key)?;
    if !value.is_object() {
        return None;
    }
    Some(value.clone())
}

fn normalize_tool_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((_, suffix)) = trimmed.rsplit_once("__") {
        return suffix.trim().to_string();
    }
    trimmed.to_string()
}

fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn recommend_profile(requirement: &str) -> Value {
    let normalized = requirement.trim();
    let category = if contains_any(normalized, &["代码", "开发", "编程", "code", "debug"]) {
        "engineering"
    } else if contains_any(normalized, &["产品", "需求", "roadmap", "用户"]) {
        "product"
    } else if contains_any(normalized, &["运营", "增长", "营销", "campaign"]) {
        "growth"
    } else {
        "general"
    };

    let name = match category {
        "engineering" => "研发协作助手",
        "product" => "产品分析助手",
        "growth" => "增长运营助手",
        _ => "通用业务助手",
    };
    let description = format!(
        "根据需求“{}”生成的建议智能体。",
        truncate_text(normalized, 80)
    );
    let role_definition = format!(
        "你是{name}。请围绕用户目标拆解任务、明确约束、给出可执行步骤，并在必要时主动澄清信息缺口。"
    );
    let skill_suggestions = match category {
        "engineering" => vec![
            "code_review".to_string(),
            "bug_fix".to_string(),
            "test_design".to_string(),
        ],
        "product" => vec![
            "requirement_analysis".to_string(),
            "roadmap_planning".to_string(),
            "prd_writing".to_string(),
        ],
        "growth" => vec![
            "campaign_planning".to_string(),
            "funnel_analysis".to_string(),
            "copywriting".to_string(),
        ],
        _ => vec![
            "task_planning".to_string(),
            "knowledge_summary".to_string(),
            "decision_support".to_string(),
        ],
    };
    json!({
        "name": name,
        "description": description,
        "category": category,
        "role_definition": role_definition,
        "suggested_skill_ids": skill_suggestions,
    })
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(pattern.to_ascii_lowercase().as_str()))
}
