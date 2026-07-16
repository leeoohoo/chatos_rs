// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp_runtime::{
    BuiltinToolProvider, McpBuiltinServer, ToolCallContext, ToolStreamChunkCallback,
};
use chatos_plugin_management_sdk::ResolvedSkill;
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{internal_skill_catalog, internal_skill_instructions, local_skill_inventory, native};

#[derive(Clone)]
pub(crate) struct PreparedLocalSkill {
    pub(crate) skill_id: String,
    pub(crate) display_name: String,
    pub(crate) instructions: String,
    pub(crate) server: Option<McpBuiltinServer>,
    pub(crate) provider: Option<LocalSkillBuiltinProvider>,
}

#[derive(Clone)]
pub(crate) struct LocalSkillBuiltinProvider {
    server_name: String,
    skill_id: String,
    tools: Vec<Value>,
    state: LocalState,
    request: RelayRequest,
}

pub(crate) fn prepare_local_skill(
    skill: &ResolvedSkill,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<PreparedLocalSkill, String> {
    let item = internal_skill_catalog()
        .map_err(|error| error.to_string())?
        .skills
        .into_iter()
        .find(|item| item.skill_id == skill.resource.id)
        .ok_or_else(|| format!("Skill is not bundled in this client: {}", skill.resource.id))?;
    let inventory = local_skill_inventory()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|inventory| inventory.skill_id == item.skill_id)
        .ok_or_else(|| format!("Local Skill inventory is missing: {}", item.skill_id))?;
    if inventory.status != "available" || inventory.dependency_status != "available" {
        return Err(inventory
            .last_error
            .unwrap_or_else(|| format!("Local Skill is unavailable: {}", item.display_name)));
    }
    validate_snapshot(skill, &inventory, request)?;
    let instructions = internal_skill_instructions(item.skill_id.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Local Skill instructions are missing: {}", item.skill_id))?
        .to_string();
    let tools = native::tool_definitions(item.skill_id.as_str(), state, request)
        .map_err(|error| error.to_string())?;
    let server_name = local_skill_server_name(item.skill_id.as_str());
    let (server, provider) = if tools.is_empty() {
        (None, None)
    } else {
        let server = McpBuiltinServer {
            name: server_name.clone(),
            kind: "local_connector_skill".to_string(),
            workspace_dir: format!(
                "local://connector/{}/{}",
                request.device_id.as_deref().unwrap_or_default(),
                request.workspace_id
            ),
            user_id: request.owner_user_id.clone(),
            project_id: request.headers.get("x-task-runner-task-id").cloned(),
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: item
                .permissions
                .iter()
                .any(|permission| permission == "workspace.write"),
            max_file_bytes: 2 * 1024 * 1024,
            max_write_bytes: 2 * 1024 * 1024,
            search_limit: 0,
        };
        let provider = LocalSkillBuiltinProvider {
            server_name,
            skill_id: item.skill_id.clone(),
            tools,
            state: state.clone(),
            request: request.clone(),
        };
        (Some(server), Some(provider))
    };
    Ok(PreparedLocalSkill {
        skill_id: item.skill_id,
        display_name: item.display_name,
        instructions,
        server,
        provider,
    })
}

fn validate_snapshot(
    skill: &ResolvedSkill,
    inventory: &super::LocalSkillInventoryItem,
    request: &RelayRequest,
) -> Result<(), String> {
    let installation = skill
        .installation
        .as_ref()
        .ok_or_else(|| format!("Skill installation is missing: {}", skill.resource.id))?;
    let device_id = request.device_id.as_deref().unwrap_or_default();
    if installation.device_id != device_id
        || installation.skill_id != inventory.skill_id
        || installation.bundle_id != inventory.bundle_id
        || installation.version != inventory.version
        || installation.bundle_hash != inventory.bundle_hash
        || installation.status != "available"
        || installation.dependency_status != "available"
    {
        return Err(format!(
            "Skill bundle snapshot is not available on this device: {}",
            skill.resource.id
        ));
    }
    let content = &skill.resource.content;
    if content.bundle_id.as_deref() != Some(inventory.bundle_id.as_str())
        || content.bundle_version.as_deref() != Some(inventory.version.as_str())
        || content.bundle_hash.as_deref() != Some(inventory.bundle_hash.as_str())
    {
        return Err(format!(
            "Skill policy bundle does not match the embedded client bundle: {}",
            skill.resource.id
        ));
    }
    Ok(())
}

#[async_trait]
impl BuiltinToolProvider for LocalSkillBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        if !self
            .tools
            .iter()
            .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        {
            return Err(format!("Local Skill operation is not published: {name}"));
        }
        native::execute(
            self.skill_id.as_str(),
            name,
            &args,
            &self.state,
            &self.request,
        )
        .map_err(|error| error.to_string())
    }
}

fn local_skill_server_name(skill_id: &str) -> String {
    let suffix = skill_id
        .trim()
        .strip_prefix("internal_skill_")
        .unwrap_or(skill_id)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("local_skill_{suffix}")
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
