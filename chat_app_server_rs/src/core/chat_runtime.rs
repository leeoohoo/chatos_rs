use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::repositories::projects;
use crate::services::builtin_mcp::{
    CODE_MAINTAINER_READ_MCP_ID, CODE_MAINTAINER_WRITE_MCP_ID,
    REMOTE_CONNECTION_CONTROLLER_MCP_ID, TERMINAL_CONTROLLER_MCP_ID,
};
use crate::services::memory_server_client::{
    MemoryAgentRuntimeCommandSummaryDto, MemoryAgentRuntimeContextDto,
};

pub const CONTACT_COMMAND_READER_TOOL_NAME: &str = "memory_command_reader_get_command_detail";

#[derive(Debug, Clone)]
pub struct ParsedContactCommandInvocation {
    pub command_ref: String,
    pub name: String,
    pub plugin_source: String,
    pub source_path: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub content: String,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedImplicitCommandSelection {
    pub command_ref: Option<String>,
    pub name: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRuntimeMetadata {
    pub contact_agent_id: Option<String>,
    pub contact_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub mcp_enabled: Option<bool>,
    #[serde(default)]
    pub enabled_mcp_ids: Vec<String>,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn normalize_id(value: Option<String>) -> Option<String> {
    normalize_optional_string(value)
}

pub fn metadata_string(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_optional_string(cursor.as_str().map(ToOwned::to_owned))
}

pub fn metadata_bool(metadata: Option<&Value>, path: &[&str]) -> Option<bool> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_bool()
}

pub fn metadata_string_list(metadata: Option<&Value>, path: &[&str]) -> Vec<String> {
    let mut cursor = match metadata {
        Some(value) => value,
        None => return Vec::new(),
    };
    for key in path {
        let Some(next) = cursor.get(*key) else {
            return Vec::new();
        };
        cursor = next;
    }
    let Some(items) = cursor.as_array() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn metadata_string_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| metadata_string(metadata, path))
}

fn metadata_bool_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<bool> {
    paths.iter().find_map(|path| metadata_bool(metadata, path))
}

fn metadata_string_list_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Vec<String> {
    paths
        .iter()
        .find_map(|path| {
            let values = metadata_string_list(metadata, path);
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        })
        .unwrap_or_default()
}

impl ChatRuntimeMetadata {
    pub fn from_metadata(metadata: Option<&Value>) -> Self {
        Self {
            contact_agent_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "contact_agent_id"],
                    &["chat_runtime", "contactAgentId"],
                    &["contact", "agent_id"],
                    &["contact", "agentId"],
                    &["ui_contact", "agent_id"],
                    &["ui_contact", "agentId"],
                    &["ui_chat_selection", "selected_agent_id"],
                    &["ui_chat_selection", "selectedAgentId"],
                ],
            ),
            contact_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "contact_id"],
                    &["chat_runtime", "contactId"],
                    &["contact", "contact_id"],
                    &["contact", "contactId"],
                    &["ui_contact", "contact_id"],
                    &["ui_contact", "contactId"],
                ],
            ),
            project_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "project_id"],
                    &["chat_runtime", "projectId"],
                ],
            ),
            project_root: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "project_root"],
                    &["chat_runtime", "projectRoot"],
                ],
            ),
            workspace_root: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "workspace_root"],
                    &["chat_runtime", "workspaceRoot"],
                ],
            ),
            remote_connection_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "remote_connection_id"],
                    &["chat_runtime", "remoteConnectionId"],
                ],
            ),
            mcp_enabled: metadata_bool_aliases(
                metadata,
                &[
                    &["chat_runtime", "mcp_enabled"],
                    &["chat_runtime", "mcpEnabled"],
                ],
            ),
            enabled_mcp_ids: metadata_string_list_aliases(
                metadata,
                &[
                    &["chat_runtime", "enabled_mcp_ids"],
                    &["chat_runtime", "enabledMcpIds"],
                ],
            ),
        }
    }
}

pub fn project_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).project_id
}

fn normalize_lookup_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn command_aliases(command: &MemoryAgentRuntimeCommandSummaryDto) -> Vec<String> {
    let mut out = Vec::new();
    let command_ref = command.command_ref.trim();
    if !command_ref.is_empty() {
        out.push(command_ref.to_ascii_lowercase());
    }
    let command_name = command.name.trim();
    if !command_name.is_empty() {
        out.push(command_name.to_ascii_lowercase());
    }

    let normalized_source_path = command.source_path.trim().replace('\\', "/");
    if !normalized_source_path.is_empty() {
        let file_name = normalized_source_path
            .rsplit('/')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(normalized_source_path.as_str());
        let file_name = file_name
            .strip_suffix(".md")
            .unwrap_or(file_name)
            .trim()
            .to_ascii_lowercase();
        if !file_name.is_empty() {
            out.push(file_name);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn parse_contact_command_invocation(
    user_message: &str,
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
) -> Option<ParsedContactCommandInvocation> {
    let trimmed = user_message.trim();
    let command_line = trimmed.strip_prefix('/')?;
    let command_line = command_line.trim();
    if command_line.is_empty() {
        return None;
    }

    let mut parts = command_line.splitn(2, char::is_whitespace);
    let command_token = parts.next().unwrap_or_default().trim();
    if command_token.is_empty() {
        return None;
    }
    let command_arguments = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let runtime_context = runtime_context?;
    if runtime_context.runtime_commands.is_empty() {
        return None;
    }
    let expected = normalize_lookup_token(command_token);
    let command = runtime_context
        .runtime_commands
        .iter()
        .find(|item| command_aliases(item).iter().any(|alias| alias == &expected))?;

    Some(ParsedContactCommandInvocation {
        command_ref: command.command_ref.trim().to_string(),
        name: command.name.trim().to_string(),
        plugin_source: command.plugin_source.trim().to_string(),
        source_path: command.source_path.trim().to_string(),
        description: command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        argument_hint: command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        content: command.content.trim().to_string(),
        arguments: command_arguments,
    })
}

pub fn compose_contact_command_system_prompt(
    command: Option<&ParsedContactCommandInvocation>,
) -> Option<String> {
    let command = command?;
    if command.command_ref.trim().is_empty()
        || command.plugin_source.trim().is_empty()
        || command.source_path.trim().is_empty()
    {
        return None;
    }

    let mut lines = vec![
        "用户在本轮显式触发了联系人命令，请优先按照命令内容执行。".to_string(),
        format!("command_ref={}", command.command_ref.trim()),
        format!("命令名称={}", command.name.trim()),
        format!("plugin_source={}", command.plugin_source.trim()),
        format!("source_path={}", command.source_path.trim()),
    ];
    if let Some(description) = command.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!("命令简介={}", description));
        }
    }
    if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
        if !argument_hint.is_empty() {
            lines.push(format!("参数提示={}", argument_hint));
        }
    }
    if let Some(arguments) = command.arguments.as_deref().map(str::trim) {
        if !arguments.is_empty() {
            lines.push(format!("用户附加参数={}", arguments));
        }
    }
    let content = command.content.trim();
    if !content.is_empty() {
        lines.push("命令完整内容：".to_string());
        for item in content.lines() {
            lines.push(item.to_string());
        }
    }
    Some(lines.join("\n").trim().to_string())
}

pub fn parse_implicit_command_selections_from_tools_end(
    payload: &Value,
) -> Vec<ParsedImplicitCommandSelection> {
    let mut out = Vec::new();
    let Some(tool_results) = payload.get("tool_results").and_then(Value::as_array) else {
        return out;
    };

    for tool_result in tool_results {
        let Some(name) = tool_result.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.trim() != CONTACT_COMMAND_READER_TOOL_NAME {
            continue;
        }
        if tool_result
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if tool_result
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            == false
        {
            continue;
        }
        let Some(content) = tool_result.get("content").and_then(Value::as_str) else {
            continue;
        };
        let Ok(content_value) = serde_json::from_str::<Value>(content) else {
            continue;
        };
        let plugin_source = content_value
            .get("plugin_source")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let source_path = content_value
            .get("source_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(plugin_source), Some(source_path)) = (plugin_source, source_path) else {
            continue;
        };

        out.push(ParsedImplicitCommandSelection {
            command_ref: content_value
                .get("command_ref")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            name: content_value
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            plugin_source,
            source_path,
        });
    }

    out
}

pub fn compose_contact_system_prompt(
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
) -> Option<String> {
    let agent = runtime_context?;
    let agent_name = agent.name.trim();
    if agent_name.is_empty() {
        return None;
    }

    let mut lines = vec![
        "你正在以联系人智能体身份参与对话。".to_string(),
        format!("联系人名称：{}", agent_name),
    ];

    if let Some(description) = agent.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!("联系人简介：{}", description));
        }
    }
    if let Some(category) = agent.category.as_deref().map(str::trim) {
        if !category.is_empty() {
            lines.push(format!("联系人分类：{}", category));
        }
    }

    lines.push(String::new());
    lines.push("角色定义：".to_string());
    lines.push(agent.role_definition.trim().to_string());

    lines.push(String::new());
    lines.push("与联系人运行时资产相关的可选 skill/plugin/common、以及任务规划时可填写的 capability token，会在专门的任务规划补充上下文中单独给出，这里不重复展开。".to_string());

    Some(lines.join("\n").trim().to_string())
}

fn task_capability_prompt_entry(mcp_id: &str) -> Option<(&'static str, &'static str)> {
    match mcp_id {
        CODE_MAINTAINER_READ_MCP_ID => Some(("read", "查看/读取项目内容")),
        CODE_MAINTAINER_WRITE_MCP_ID => Some(("write", "写入/修改项目内容")),
        TERMINAL_CONTROLLER_MCP_ID => Some(("terminal", "执行终端命令、构建、测试、查看日志")),
        REMOTE_CONNECTION_CONTROLLER_MCP_ID => Some(("remote", "访问当前选中的远程连接")),
        _ => None,
    }
}

pub fn compose_contact_task_planning_prompt(
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
    authorized_builtin_mcp_ids: &[String],
) -> Option<String> {
    let mut lines = vec![
        "以下内容是当前联系人专属的任务规划补充上下文。".to_string(),
        "创建任务时，优先用 `required_builtin_capabilities` 和 `required_context_assets`，不要自己猜内部字段或随机 ID。".to_string(),
    ];

    let capability_entries = authorized_builtin_mcp_ids
        .iter()
        .filter_map(|item| task_capability_prompt_entry(item.as_str()).map(|entry| (item, entry)))
        .collect::<Vec<_>>();
    let allowed_capabilities = capability_entries
        .iter()
        .map(|(_, (capability, _))| capability.to_string())
        .collect::<Vec<_>>();
    let authorized_builtin_mcp_tools = capability_entries
        .iter()
        .map(|(_, (capability, description))| {
            json!({
                "fill_value": capability,
                "description": description,
            })
        })
        .collect::<Vec<_>>();
    if !capability_entries.is_empty() {
        lines.push(String::new());
        lines.push("创建任务时 `required_builtin_capabilities` 可选值如下，只能填这些 token：".to_string());
        for (index, (_, (capability, description))) in capability_entries.iter().enumerate() {
            lines.push(format!(
                "{}. {} | {}",
                index + 1,
                capability,
                description
            ));
        }
    } else {
        lines.push(String::new());
        lines.push("当前 `required_builtin_capabilities` 没有可选值；如果后续你看到这里为空，就不要填写这个字段。".to_string());
    }
    lines.push(String::new());
    lines.push("下面是当前联系人授权内、可用于任务规划的 MCP 选项列表；创建任务时只能从 `fill_value` 里选来填写 `required_builtin_capabilities`：".to_string());
    if let Ok(serialized) = serde_json::to_string_pretty(&json!({
        "authorized_builtin_mcp_tools_allowed": authorized_builtin_mcp_tools,
    })) {
        lines.push("```json".to_string());
        lines.extend(serialized.lines().map(ToOwned::to_owned));
        lines.push("```".to_string());
    }

    let mut allowed_skill_assets = Vec::new();
    let mut allowed_plugin_assets = Vec::new();
    let mut allowed_common_assets = Vec::new();
    if let Some(agent) = runtime_context {
        lines.push(String::new());
        lines.push("当前联系人运行时可直接引用的上下文资产：".to_string());

        if !agent.runtime_skills.is_empty() {
            lines.push("skills:".to_string());
            for (index, skill) in agent.runtime_skills.iter().enumerate() {
                let description = skill
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("未提供");
                let skill_ref = format!("SK{}", index + 1);
                lines.push(format!(
                    "- skill_ref=SK{} | 名称={} | 简介={}",
                    index + 1,
                    skill.name.trim(),
                    description
                ));
                allowed_skill_assets.push(json!({
                    "asset_ref": skill_ref,
                    "name": skill.name.trim(),
                }));
            }
        }

        if !agent.runtime_plugins.is_empty() {
            lines.push("plugins:".to_string());
            for (index, plugin) in agent.runtime_plugins.iter().enumerate() {
                let description = plugin
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        plugin
                            .content_summary
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                    })
                    .unwrap_or("未提供");
                let plugin_ref = format!("PL{}", index + 1);
                lines.push(format!(
                    "- plugin_ref=PL{} | 名称={} | plugin_source={} | 简介={}",
                    index + 1,
                    plugin.name.trim(),
                    plugin.source.trim(),
                    description
                ));
                allowed_plugin_assets.push(json!({
                    "asset_ref": plugin_ref,
                    "name": plugin.name.trim(),
                }));
            }
        }

        if !agent.runtime_commands.is_empty() {
            lines.push("commons:".to_string());
            for command in &agent.runtime_commands {
                let description = command
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("未提供");
                lines.push(format!(
                    "- command_ref={} | 名称={} | 简介={}",
                    command.command_ref.trim(),
                    command.name.trim(),
                    description
                ));
                allowed_common_assets.push(json!({
                    "asset_ref": command.command_ref.trim(),
                    "name": command.name.trim(),
                }));
            }
        }
    }

    lines.push(String::new());
    lines.push("下面是本次 create_tasks 需要填写的可选项，必须只从这里面选：".to_string());
    if let Ok(serialized) = serde_json::to_string_pretty(&json!({
        "required_builtin_capabilities_allowed": allowed_capabilities,
        "required_context_assets_allowed": {
            "skills": allowed_skill_assets,
            "plugins": allowed_plugin_assets,
            "commons": allowed_common_assets,
        }
    })) {
        lines.push("```json".to_string());
        lines.extend(serialized.lines().map(ToOwned::to_owned));
        lines.push("```".to_string());
    }

    let mut examples = Vec::new();
    let example_capabilities = allowed_capabilities.iter().take(2).cloned().collect::<Vec<_>>();
    let skill_ref = runtime_context
        .and_then(|agent| agent.runtime_skills.first())
        .map(|_| "SK1".to_string());
    let plugin_ref = runtime_context
        .and_then(|agent| agent.runtime_plugins.first())
        .map(|_| "PL1".to_string());
    let command_ref = runtime_context
        .and_then(|agent| agent.runtime_commands.first())
        .map(|command| command.command_ref.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut required_context_assets = Vec::new();
    if let Some(asset_ref) = skill_ref.clone() {
        required_context_assets.push(json!({
            "asset_type": "skill",
            "asset_ref": asset_ref,
        }));
    }
    if let Some(asset_ref) = command_ref.clone() {
        required_context_assets.push(json!({
            "asset_type": "common",
            "asset_ref": asset_ref,
        }));
    }
    if !required_context_assets.is_empty() || !example_capabilities.is_empty() {
        examples.push(json!({
            "title": "先梳理需求并形成可执行结果",
            "details": "结合当前轮已经看到的上下文，执行任务并输出明确结果；不要在执行阶段重复做无边界探索。",
            "required_builtin_capabilities": example_capabilities,
            "required_context_assets": required_context_assets,
            "execution_result_contract": {
                "result_required": true,
                "preferred_format": "markdown"
            }
        }));
    }

    if let Some(asset_ref) = plugin_ref {
        let second_example_capabilities = if example_capabilities.is_empty() {
            Vec::new()
        } else {
            vec![example_capabilities[0].clone()]
        };
        examples.push(json!({
            "title": "基于当前联系人插件能力完成任务",
            "details": "优先利用联系人已有插件与技能，不要重新发明流程。",
            "required_builtin_capabilities": second_example_capabilities,
            "required_context_assets": [{
                "asset_type": "plugin",
                "asset_ref": asset_ref,
            }],
            "execution_result_contract": {
                "result_required": true,
                "preferred_format": "markdown"
            }
        }));
    }

    if !examples.is_empty() {
        lines.push(String::new());
        lines.push("你可以直接参考下面这些 create_tasks 调用参数示例：".to_string());
        for (index, example) in examples.into_iter().take(2).enumerate() {
            if let Ok(serialized) = serde_json::to_string_pretty(&example) {
                lines.push(format!("示例 {}:", index + 1));
                lines.push("```json".to_string());
                lines.extend(serialized.lines().map(ToOwned::to_owned));
                lines.push("```".to_string());
            }
        }
    }

    Some(lines.join("\n").trim().to_string())
}

fn normalize_path_text(raw: &str) -> String {
    let mut out = raw.trim().replace('\\', "/");
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

pub async fn resolve_project_runtime(
    user_id: Option<&str>,
    project_id: Option<String>,
    project_root: Option<String>,
) -> (Option<String>, Option<String>) {
    let mut resolved_project_id = normalize_optional_string(project_id);
    let mut resolved_project_root = normalize_optional_string(project_root);

    let Some(project_id) = resolved_project_id.clone() else {
        return (resolved_project_id, resolved_project_root);
    };

    let project = match projects::get_project_by_id(project_id.as_str()).await {
        Ok(Some(project)) => project,
        _ => {
            resolved_project_id = None;
            return (resolved_project_id, resolved_project_root);
        }
    };

    if let (Some(uid), Some(project_owner)) = (user_id, project.user_id.as_deref()) {
        if project_owner != uid {
            resolved_project_id = None;
            return (resolved_project_id, resolved_project_root);
        }
    }

    let expected_root = normalize_path_text(project.root_path.as_str());
    match resolved_project_root.clone() {
        Some(current_root) => {
            if normalize_path_text(current_root.as_str()) != expected_root {
                resolved_project_root = Some(project.root_path);
            }
        }
        None => {
            resolved_project_root = Some(project.root_path);
        }
    }

    (resolved_project_id, resolved_project_root)
}

#[cfg(test)]
mod tests {
    use super::{
        compose_contact_command_system_prompt, compose_contact_system_prompt,
        compose_contact_task_planning_prompt,
        parse_contact_command_invocation, parse_implicit_command_selections_from_tools_end,
        ChatRuntimeMetadata, CONTACT_COMMAND_READER_TOOL_NAME,
    };
    use crate::services::memory_server_client::{
        MemoryAgentRuntimeCommandSummaryDto, MemoryAgentRuntimeContextDto,
        MemoryAgentRuntimePluginSummaryDto, MemoryAgentRuntimeSkillSummaryDto,
    };
    use serde_json::json;

    #[test]
    fn builds_contact_prompt_with_core_identity_only() {
        let prompt = compose_contact_system_prompt(Some(&MemoryAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            name: "小林".to_string(),
            description: Some("负责前端排障".to_string()),
            category: Some("frontend".to_string()),
            model_config_id: Some("model_1".to_string()),
            role_definition: "专注组件与状态问题".to_string(),
            plugin_sources: vec!["frontend_toolkit".to_string()],
            runtime_plugins: vec![MemoryAgentRuntimePluginSummaryDto {
                source: "frontend_toolkit".to_string(),
                name: "前端工具箱".to_string(),
                category: Some("frontend".to_string()),
                description: Some("用于组件设计和渲染排查".to_string()),
                content_summary: Some("1. 技能=组件排障 | 内容片段=定位 UI 异常".to_string()),
                updated_at: Some("2026-03-24T00:00:00Z".to_string()),
            }],
            skills: Vec::new(),
            skill_ids: vec!["skill_a".to_string()],
            runtime_skills: vec![MemoryAgentRuntimeSkillSummaryDto {
                id: "skill_a".to_string(),
                name: "组件排障".to_string(),
                description: Some("定位 UI 异常".to_string()),
                plugin_source: Some("frontend_toolkit".to_string()),
                source_type: "skill_center".to_string(),
                source_path: Some("skills/ui/SKILL.md".to_string()),
                updated_at: Some("2026-03-24T00:00:00Z".to_string()),
            }],
            runtime_commands: vec![MemoryAgentRuntimeCommandSummaryDto {
                command_ref: "CMD1".to_string(),
                name: "team-debug".to_string(),
                description: Some("并行调试命令".to_string()),
                argument_hint: Some("<error> [--hypotheses 3]".to_string()),
                plugin_source: "frontend_toolkit".to_string(),
                source_path: "commands/team-debug.md".to_string(),
                content: "# Team Debug".to_string(),
                updated_at: Some("2026-03-24T00:00:00Z".to_string()),
            }],
            mcp_policy: None,
            project_policy: None,
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        }))
        .expect("prompt");

        assert!(prompt.contains("联系人名称：小林"));
        assert!(prompt.contains("联系人简介：负责前端排障"));
        assert!(prompt.contains("联系人分类：frontend"));
        assert!(prompt.contains("角色定义："));
        assert!(prompt.contains("专注组件与状态问题"));
        assert!(prompt.contains("任务规划补充上下文中单独给出"));
        assert!(!prompt.contains("plugin_ref=PL1"));
        assert!(!prompt.contains("skill_ref=SK1"));
        assert!(!prompt.contains("command_ref=CMD1"));
    }

    #[test]
    fn builds_dynamic_task_planning_prompt_with_examples() {
        let prompt = compose_contact_task_planning_prompt(
            Some(&MemoryAgentRuntimeContextDto {
                agent_id: "agent_1".to_string(),
                name: "小林".to_string(),
                description: Some("负责前端排障".to_string()),
                category: Some("frontend".to_string()),
                model_config_id: Some("model_1".to_string()),
                role_definition: "专注组件与状态问题".to_string(),
                plugin_sources: vec!["frontend_toolkit".to_string()],
                runtime_plugins: vec![MemoryAgentRuntimePluginSummaryDto {
                    source: "frontend_toolkit".to_string(),
                    name: "前端工具箱".to_string(),
                    category: Some("tooling".to_string()),
                    description: Some("常用排障插件".to_string()),
                    content_summary: None,
                    updated_at: None,
                }],
                skills: Vec::new(),
                skill_ids: vec!["skill_1".to_string()],
                runtime_skills: vec![MemoryAgentRuntimeSkillSummaryDto {
                    id: "skill_1".to_string(),
                    name: "页面排障".to_string(),
                    description: Some("用于分析前端报错".to_string()),
                    plugin_source: Some("frontend_toolkit".to_string()),
                    source_type: "skill_center".to_string(),
                    source_path: Some("/skills/debug.md".to_string()),
                    updated_at: None,
                }],
                runtime_commands: vec![MemoryAgentRuntimeCommandSummaryDto {
                    command_ref: "CMD_DEBUG".to_string(),
                    name: "debug".to_string(),
                    description: Some("查看调试命令".to_string()),
                    argument_hint: None,
                    plugin_source: "frontend_toolkit".to_string(),
                    source_path: "/commands/debug.md".to_string(),
                    content: "run debug".to_string(),
                    updated_at: None,
                }],
                mcp_policy: None,
                project_policy: None,
                updated_at: "2026-03-24T00:00:00Z".to_string(),
            }),
            &[
                "builtin_code_maintainer_read".to_string(),
                "builtin_terminal_controller".to_string(),
            ],
        )
        .expect("dynamic planning prompt should exist");

        assert!(prompt.contains("1. read | 查看/读取项目内容"));
        assert!(prompt.contains("2. terminal | 执行终端命令、构建、测试、查看日志"));
        assert!(prompt.contains("\"authorized_builtin_mcp_tools_allowed\""));
        assert!(prompt.contains("\"fill_value\": \"read\""));
        assert!(prompt.contains("\"required_builtin_capabilities_allowed\""));
        assert!(prompt.contains("\"required_context_assets_allowed\""));
        assert!(prompt.contains("skill_ref=SK1"));
        assert!(prompt.contains("plugin_ref=PL1"));
        assert!(prompt.contains("command_ref=CMD_DEBUG"));
        assert!(prompt.contains("\"required_builtin_capabilities\""));
        assert!(!prompt.contains("\"id\":"));
        assert!(!prompt.contains("\"source_path\":"));
        assert!(!prompt.contains("\"plugin_source\":"));
    }

    #[test]
    fn parses_explicit_contact_command_invocation() {
        let runtime_context = MemoryAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            name: "小林".to_string(),
            description: None,
            category: None,
            model_config_id: Some("model_1".to_string()),
            role_definition: "专注问题排查".to_string(),
            plugin_sources: vec!["frontend_toolkit".to_string()],
            runtime_plugins: vec![],
            skills: vec![],
            skill_ids: vec![],
            runtime_skills: vec![],
            runtime_commands: vec![MemoryAgentRuntimeCommandSummaryDto {
                command_ref: "CMD1".to_string(),
                name: "team-debug".to_string(),
                description: Some("并行调试命令".to_string()),
                argument_hint: Some("<error>".to_string()),
                plugin_source: "frontend_toolkit".to_string(),
                source_path: "commands/team-debug.md".to_string(),
                content: "debug steps".to_string(),
                updated_at: None,
            }],
            mcp_policy: None,
            project_policy: None,
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        };
        let command = parse_contact_command_invocation(
            "/team-debug button not render",
            Some(&runtime_context),
        )
        .expect("command");
        assert_eq!(command.command_ref, "CMD1");
        assert_eq!(command.name, "team-debug");
        assert_eq!(command.arguments.as_deref(), Some("button not render"));
        let prompt = compose_contact_command_system_prompt(Some(&command)).expect("prompt");
        assert!(prompt.contains("command_ref=CMD1"));
        assert!(prompt.contains("用户附加参数=button not render"));
    }

    #[test]
    fn resolves_remote_connection_id_from_metadata_aliases() {
        let metadata = json!({
            "chat_runtime": {
                "remoteConnectionId": " conn_1 "
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).remote_connection_id,
            Some("conn_1".to_string())
        );

        let metadata = json!({
            "chat_runtime": {
                "remote_connection_id": "conn_2"
            }
        });
        assert_eq!(
            ChatRuntimeMetadata::from_metadata(Some(&metadata)).remote_connection_id,
            Some("conn_2".to_string())
        );
    }

    #[test]
    fn normalizes_runtime_metadata_from_standard_and_legacy_paths() {
        let metadata = json!({
            "contact": {
                "agentId": " agent_1 ",
                "contact_id": " contact_1 "
            },
            "chat_runtime": {
                "projectId": " project_1 ",
                "project_root": " /tmp/workspace ",
                "workspaceRoot": " /tmp/ws ",
                "remoteConnectionId": " conn_1 ",
                "mcpEnabled": true,
                "enabledMcpIds": ["alpha", " alpha ", "beta", ""]
            }
        });

        let runtime = ChatRuntimeMetadata::from_metadata(Some(&metadata));
        assert_eq!(runtime.contact_agent_id.as_deref(), Some("agent_1"));
        assert_eq!(runtime.contact_id.as_deref(), Some("contact_1"));
        assert_eq!(runtime.project_id.as_deref(), Some("project_1"));
        assert_eq!(runtime.project_root.as_deref(), Some("/tmp/workspace"));
        assert_eq!(runtime.workspace_root.as_deref(), Some("/tmp/ws"));
        assert_eq!(runtime.remote_connection_id.as_deref(), Some("conn_1"));
        assert_eq!(runtime.mcp_enabled, Some(true));
        assert_eq!(runtime.enabled_mcp_ids, vec!["alpha", "beta"]);
    }

    #[test]
    fn parses_implicit_command_selection_from_tools_end_payload() {
        let payload = serde_json::json!({
            "tool_results": [
                {
                    "name": CONTACT_COMMAND_READER_TOOL_NAME,
                    "success": true,
                    "is_error": false,
                    "content": r#"{
                      "command_ref": "CMD2",
                      "name": "team-feature",
                      "plugin_source": "plugins/agent-teams",
                      "source_path": "commands/team-feature.md"
                    }"#
                }
            ]
        });
        let items = parse_implicit_command_selections_from_tools_end(&payload);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command_ref.as_deref(), Some("CMD2"));
        assert_eq!(items[0].name.as_deref(), Some("team-feature"));
        assert_eq!(items[0].plugin_source, "plugins/agent-teams");
        assert_eq!(items[0].source_path, "commands/team-feature.md");
    }
}
