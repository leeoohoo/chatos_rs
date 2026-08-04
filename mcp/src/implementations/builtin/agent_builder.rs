// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool_registry::{block_on_result, text_result, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBuilderSkill {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[async_trait]
pub trait AgentBuilderStore: Send + Sync {
    async fn create_agent(&self, request: Value) -> Result<Value, String>;

    async fn update_agent(&self, agent_id: &str, request: Value) -> Result<Option<Value>, String>;
}

#[derive(Clone)]
pub struct AgentBuilderStoreRef(Arc<dyn AgentBuilderStore>);

impl AgentBuilderStoreRef {
    pub fn new(store: Arc<dyn AgentBuilderStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn AgentBuilderStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for AgentBuilderStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentBuilderStoreRef")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct AgentBuilderOptions {
    pub server_name: String,
    pub user_id: Option<String>,
    pub store: Option<AgentBuilderStoreRef>,
}

#[derive(Clone)]
pub struct AgentBuilderService {
    registry: ToolRegistry<ToolHandler>,
    default_user_id: Option<String>,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

impl AgentBuilderService {
    pub fn new(opts: AgentBuilderOptions) -> Result<Self, String> {
        let default_user_id = normalize_optional_string(opts.user_id);
        let mut service = Self {
            registry: ToolRegistry::new(),
            default_user_id,
        };

        service.register_recommend_agent_profile(opts.server_name.as_str());
        service.register_create_memory_agent(opts.store.clone());
        service.register_update_memory_agent(opts.store);
        service.register_preview_agent_context();

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        _conversation_id: Option<&str>,
        _conversation_turn_id: Option<&str>,
        _on_stream_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Result<Value, String> {
        let normalized = normalize_tool_name(name);
        let tool = self
            .registry
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
        self.registry
            .register_tool(name, description, input_schema, handler);
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
                "properties": { "requirement": { "type": "string" } },
                "required": ["requirement"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let requirement = required_string(&args, "requirement")?;
                Ok(text_result(recommend_profile(requirement.as_str())))
            }),
        );
    }

    fn register_create_memory_agent(&mut self, store: Option<AgentBuilderStoreRef>) {
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
                    "project_policy": { "type": "object" }
                },
                "required": ["name", "role_definition"],
                "additionalProperties": false
            }),
            Arc::new(move |args, default_user_id| {
                let store = store
                    .as_ref()
                    .ok_or_else(|| "agent builder store is not configured".to_string())?
                    .inner();
                let payload = build_create_payload(args, default_user_id)?;
                let created = block_on_result(async move { store.create_agent(payload).await })?;
                Ok(text_result(json!({
                    "created": true,
                    "agent": created,
                })))
            }),
        );
    }

    fn register_update_memory_agent(&mut self, store: Option<AgentBuilderStoreRef>) {
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
                    "project_policy": { "type": "object" }
                },
                "required": ["agent_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _default_user_id| {
                let store = store
                    .as_ref()
                    .ok_or_else(|| "agent builder store is not configured".to_string())?
                    .inner();
                let agent_id = required_string(&args, "agent_id")?;
                let payload = build_update_payload(&args);
                let update_agent_id = agent_id.clone();
                let updated = block_on_result(async move {
                    store.update_agent(update_agent_id.as_str(), payload).await
                })?;
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
                        text.push('\n');
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
    }
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
        "engineering" => vec!["code_review", "bug_fix", "test_design"],
        "product" => vec!["requirement_analysis", "roadmap_planning", "prd_writing"],
        "growth" => vec!["campaign_planning", "funnel_analysis", "copywriting"],
        _ => vec!["task_planning", "knowledge_summary", "decision_support"],
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

fn build_create_payload(args: Value, default_user_id: Option<&str>) -> Result<Value, String> {
    let mut payload = json!({
        "user_id": default_user_id.map(str::to_string),
        "name": required_string(&args, "name")?,
        "description": optional_string(&args, "description"),
        "category": optional_string(&args, "category"),
        "role_definition": required_string(&args, "role_definition")?,
        "skills": optional_skill_array(&args, "skills"),
        "skill_ids": optional_string_array(&args, "skill_ids"),
        "default_skill_ids": optional_string_array(&args, "default_skill_ids"),
        "project_policy": optional_object_value(&args, "project_policy"),
        "enabled": args.get("enabled").and_then(Value::as_bool),
    });
    remove_null_fields(&mut payload);
    Ok(payload)
}

fn build_update_payload(args: &Value) -> Value {
    let mut payload = json!({
        "name": optional_string(args, "name"),
        "description": optional_string(args, "description"),
        "category": optional_string(args, "category"),
        "role_definition": optional_string(args, "role_definition"),
        "skills": optional_skill_array(args, "skills"),
        "skill_ids": optional_string_array(args, "skill_ids"),
        "default_skill_ids": optional_string_array(args, "default_skill_ids"),
        "project_policy": optional_object_value(args, "project_policy"),
        "enabled": args.get("enabled").and_then(Value::as_bool),
    });
    remove_null_fields(&mut payload);
    payload
}

fn remove_null_fields(value: &mut Value) {
    if let Some(map) = value.as_object_mut() {
        map.retain(|_, value| !value.is_null());
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
        if trimmed.is_empty() || out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn optional_skill_array(args: &Value, key: &str) -> Option<Vec<AgentBuilderSkill>> {
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
        out.push(AgentBuilderSkill { id, name, content });
    }
    Some(out)
}

fn optional_object_value(args: &Value, key: &str) -> Option<Value> {
    let value = args.get(key)?;
    if value.is_object() {
        Some(value.clone())
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_catalog_retires_standalone_skill_and_plugin_selection() {
        let service = AgentBuilderService::new(AgentBuilderOptions {
            server_name: "agent-builder".to_string(),
            user_id: Some("user-1".to_string()),
            store: None,
        })
        .expect("schema-only Agent Builder service");
        let tools = service.list_tools();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(!names.contains(&"list_available_skills"));
        for tool_name in [
            "create_memory_agent",
            "update_memory_agent",
            "preview_agent_context",
        ] {
            let tool = tools
                .iter()
                .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
                .expect("Agent Builder tool");
            let properties = tool
                .pointer("/inputSchema/properties")
                .and_then(Value::as_object)
                .expect("tool properties");
            assert!(!properties.contains_key("plugin_sources"));
            assert!(!properties.contains_key("mcp_policy"));
            assert!(!properties.contains_key("user_id"));
        }
    }

    #[test]
    fn create_payload_uses_the_bound_owner_instead_of_model_arguments() {
        let payload = build_create_payload(
            json!({
                "user_id": "attacker",
                "name": "Bound Agent",
                "role_definition": "Stay within the owner boundary"
            }),
            Some("owner-1"),
        )
        .expect("create payload");

        assert_eq!(payload["user_id"], "owner-1");
    }
}
