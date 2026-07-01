// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool_registry::{block_on_result, text_result, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInlineSkill {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRuntimeSkill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub plugin_source: Option<String>,
    pub source_type: String,
    pub source_path: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRuntimeCommand {
    pub command_ref: String,
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub content: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRuntimePlugin {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content_summary: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRuntimeContext {
    pub agent_id: String,
    pub user_id: String,
    pub skills: Vec<MemoryInlineSkill>,
    pub skill_ids: Vec<String>,
    pub runtime_skills: Vec<MemoryRuntimeSkill>,
    pub runtime_commands: Vec<MemoryRuntimeCommand>,
    pub runtime_plugins: Vec<MemoryRuntimePlugin>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFullSkill {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub plugin_source: Option<String>,
    pub source_path: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFullPlugin {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub content: Option<String>,
    pub commands: Vec<Value>,
    pub command_count: i64,
    pub updated_at: String,
}

#[async_trait]
pub trait MemoryReaderStore: Send + Sync {
    async fn get_agent_runtime_context(
        &self,
        agent_id: &str,
    ) -> Result<Option<MemoryRuntimeContext>, String>;

    async fn get_skill(
        &self,
        user_id: &str,
        skill_id: &str,
    ) -> Result<Option<MemoryFullSkill>, String>;

    async fn get_skill_plugin(
        &self,
        user_id: &str,
        source: &str,
    ) -> Result<Option<MemoryFullPlugin>, String>;
}

#[derive(Clone)]
pub struct MemoryReaderStoreRef(Arc<dyn MemoryReaderStore>);

impl MemoryReaderStoreRef {
    pub fn new(store: Arc<dyn MemoryReaderStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn MemoryReaderStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for MemoryReaderStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryReaderStoreRef")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct MemorySkillReaderOptions {
    pub server_name: String,
    pub agent_id: String,
    pub store: MemoryReaderStoreRef,
}

#[derive(Debug, Clone)]
pub struct MemoryCommandReaderOptions {
    pub server_name: String,
    pub agent_id: String,
    pub store: MemoryReaderStoreRef,
}

#[derive(Debug, Clone)]
pub struct MemoryPluginReaderOptions {
    pub server_name: String,
    pub agent_id: String,
    pub store: MemoryReaderStoreRef,
}

#[derive(Clone)]
pub struct MemorySkillReaderService {
    registry: ToolRegistry<ToolHandler>,
}

#[derive(Clone)]
pub struct MemoryCommandReaderService {
    registry: ToolRegistry<ToolHandler>,
}

#[derive(Clone)]
pub struct MemoryPluginReaderService {
    registry: ToolRegistry<ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl MemorySkillReaderService {
    pub fn new(opts: MemorySkillReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        service.register_get_skill_detail(opts.server_name, opts.agent_id, opts.store);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        call_registered(&self.registry, name, args)
    }

    fn register_get_skill_detail(
        &mut self,
        server_name: String,
        agent_id: String,
        store: MemoryReaderStoreRef,
    ) {
        self.registry.register_tool(
            "get_skill_detail",
            &format!(
                "Read the full content of a skill that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": { "skill_ref": { "type": "string" } },
                "additionalProperties": false,
                "required": ["skill_ref"]
            }),
            Arc::new(move |args| {
                let requested_skill_ref = required_ref(&args, "skill_ref")?;
                let payload =
                    load_skill_detail(store.inner(), agent_id.clone(), requested_skill_ref)?;
                Ok(text_result(payload))
            }),
        );
    }
}

impl MemoryCommandReaderService {
    pub fn new(opts: MemoryCommandReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        service.register_get_command_detail(opts.server_name, opts.agent_id, opts.store);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        call_registered(&self.registry, name, args)
    }

    fn register_get_command_detail(
        &mut self,
        server_name: String,
        agent_id: String,
        store: MemoryReaderStoreRef,
    ) {
        self.registry.register_tool(
            "get_command_detail",
            &format!(
                "Read the full content of a command that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": { "command_ref": { "type": "string" } },
                "additionalProperties": false,
                "required": ["command_ref"]
            }),
            Arc::new(move |args| {
                let requested_command_ref = required_ref(&args, "command_ref")?;
                let payload =
                    load_command_detail(store.inner(), agent_id.clone(), requested_command_ref)?;
                Ok(text_result(payload))
            }),
        );
    }
}

impl MemoryPluginReaderService {
    pub fn new(opts: MemoryPluginReaderOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        service.register_get_plugin_detail(opts.server_name, opts.agent_id, opts.store);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        call_registered(&self.registry, name, args)
    }

    fn register_get_plugin_detail(
        &mut self,
        server_name: String,
        agent_id: String,
        store: MemoryReaderStoreRef,
    ) {
        self.registry.register_tool(
            "get_plugin_detail",
            &format!(
                "Read the full content of a plugin that belongs to the current contact agent (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": { "plugin_ref": { "type": "string" } },
                "additionalProperties": false,
                "required": ["plugin_ref"]
            }),
            Arc::new(move |args| {
                let requested_plugin_ref = required_ref(&args, "plugin_ref")?;
                let payload =
                    load_plugin_detail(store.inner(), agent_id.clone(), requested_plugin_ref)?;
                Ok(text_result(payload))
            }),
        );
    }
}

fn call_registered(
    registry: &ToolRegistry<ToolHandler>,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    let tool = registry
        .get(name)
        .ok_or_else(|| format!("Tool not found: {name}"))?;
    (tool.handler)(args)
}

fn required_ref(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing required field: {}", key))
}

fn normalize_lookup_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn skill_ref(index: usize) -> String {
    format!("SK{}", index + 1)
}

fn plugin_ref(index: usize) -> String {
    format!("PL{}", index + 1)
}

fn load_runtime_context(
    store: Arc<dyn MemoryReaderStore>,
    agent_id: String,
) -> Result<MemoryRuntimeContext, String> {
    block_on_result(async move {
        store
            .get_agent_runtime_context(agent_id.as_str())
            .await?
            .ok_or_else(|| format!("agent runtime context not found: {}", agent_id))
    })
}

fn load_skill_detail(
    store: Arc<dyn MemoryReaderStore>,
    agent_id: String,
    requested_skill_ref: String,
) -> Result<Value, String> {
    let requested_token = normalize_lookup_token(requested_skill_ref.as_str());
    let runtime_context = load_runtime_context(store.clone(), agent_id.clone())?;
    let mut resolved_skill_id: Option<String> = None;
    let mut resolved_skill_ref: Option<String> = None;

    for (index, runtime_skill) in runtime_context.runtime_skills.iter().enumerate() {
        let current_ref = skill_ref(index);
        if requested_token == normalize_lookup_token(current_ref.as_str()) {
            resolved_skill_id = Some(runtime_skill.id.clone());
            resolved_skill_ref = Some(current_ref);
            break;
        }
    }

    if resolved_skill_id.is_none() {
        for (index, raw_skill_id) in runtime_context.skill_ids.iter().enumerate() {
            let current_ref = skill_ref(index);
            if requested_token == normalize_lookup_token(current_ref.as_str()) {
                resolved_skill_id = Some(raw_skill_id.clone());
                resolved_skill_ref = Some(current_ref);
                break;
            }
        }
    }

    if resolved_skill_id.is_none() {
        for (index, inline_skill) in runtime_context.skills.iter().enumerate() {
            let current_ref = skill_ref(index);
            if requested_token == normalize_lookup_token(current_ref.as_str()) {
                resolved_skill_id = Some(inline_skill.id.clone());
                resolved_skill_ref = Some(current_ref);
                break;
            }
        }
    }

    let resolved_skill_id = resolved_skill_id.ok_or_else(|| {
        format!(
            "skill_ref does not belong to current contact agent: {}",
            requested_skill_ref
        )
    })?;

    if let Some(skill) = runtime_context
        .skills
        .iter()
        .find(|skill| skill.id.trim() == resolved_skill_id.as_str())
    {
        return Ok(json!({
            "agent_id": agent_id,
            "skill_ref": resolved_skill_ref,
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
        .find(|skill| skill.id.trim() == resolved_skill_id.as_str());
    let full_skill = block_on_result(async move {
        store
            .get_skill(runtime_context.user_id.as_str(), resolved_skill_id.as_str())
            .await?
            .ok_or_else(|| format!("skill not found: {}", resolved_skill_id))
    })?;

    Ok(json!({
        "agent_id": agent_id,
        "skill_ref": resolved_skill_ref,
        "name": full_skill.name,
        "description": full_skill.description,
        "content": full_skill.content,
        "plugin_source": runtime_skill
            .and_then(|value| value.plugin_source.clone())
            .or(full_skill.plugin_source),
        "source_path": runtime_skill
            .and_then(|value| value.source_path.clone())
            .or(full_skill.source_path),
        "source_type": runtime_skill
            .map(|value| value.source_type.clone())
            .unwrap_or_else(|| "skill_center".to_string()),
        "updated_at": runtime_skill
            .and_then(|value| value.updated_at.clone())
            .or(Some(full_skill.updated_at)),
    }))
}

fn load_command_detail(
    store: Arc<dyn MemoryReaderStore>,
    agent_id: String,
    requested_command_ref: String,
) -> Result<Value, String> {
    let expected = normalize_lookup_token(requested_command_ref.as_str());
    let runtime_context = load_runtime_context(store, agent_id.clone())?;
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

    Ok(json!({
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
}

fn load_plugin_detail(
    store: Arc<dyn MemoryReaderStore>,
    agent_id: String,
    requested_plugin_ref: String,
) -> Result<Value, String> {
    let expected = normalize_lookup_token(requested_plugin_ref.as_str());
    let runtime_context = load_runtime_context(store.clone(), agent_id.clone())?;

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
    let plugin = block_on_result(async move {
        store
            .get_skill_plugin(runtime_context.user_id.as_str(), resolved_source.as_str())
            .await?
            .ok_or_else(|| format!("plugin not found: {}", resolved_source))
    })?;
    let runtime_entry = runtime_context
        .runtime_plugins
        .iter()
        .find(|item| item.source.trim() == plugin.source.as_str());
    let related_skills = runtime_context
        .runtime_skills
        .iter()
        .filter(|item| {
            item.plugin_source
                .as_deref()
                .map(str::trim)
                .map(|value| value == plugin.source.as_str())
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

    Ok(json!({
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
}
