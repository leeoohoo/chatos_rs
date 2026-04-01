use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repositories::projects;
use crate::services::memory_server_client::{
    MemoryAgentRuntimeCommandSummaryDto, MemoryAgentRuntimeContextDto,
};

pub const CONTACT_SKILL_READER_TOOL_NAME: &str = "memory_skill_reader_get_skill_detail";
pub const CONTACT_COMMAND_READER_TOOL_NAME: &str = "memory_command_reader_get_command_detail";
pub const CONTACT_PLUGIN_READER_TOOL_NAME: &str = "memory_plugin_reader_get_plugin_detail";

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
    paths.iter()
        .find_map(|path| metadata_string(metadata, path))
}

fn metadata_bool_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<bool> {
    paths.iter().find_map(|path| metadata_bool(metadata, path))
}

fn metadata_string_list_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Vec<String> {
    paths.iter()
        .find_map(|path| {
            let values = metadata_string_list(metadata, path);
            if values.is_empty() { None } else { Some(values) }
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
                &[&["chat_runtime", "project_id"], &["chat_runtime", "projectId"]],
            ),
            project_root: metadata_string_aliases(
                metadata,
                &[&["chat_runtime", "project_root"], &["chat_runtime", "projectRoot"]],
            ),
            workspace_root: metadata_string_aliases(
                metadata,
                &[&["chat_runtime", "workspace_root"], &["chat_runtime", "workspaceRoot"]],
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
                &[&["chat_runtime", "mcp_enabled"], &["chat_runtime", "mcpEnabled"]],
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

pub fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).contact_agent_id
}

pub fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).contact_id
}

pub fn project_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).project_id
}

pub fn project_root_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).project_root
}

pub fn workspace_root_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).workspace_root
}

pub fn remote_connection_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).remote_connection_id
}

pub fn mcp_enabled_from_metadata(metadata: Option<&Value>) -> Option<bool> {
    ChatRuntimeMetadata::from_metadata(metadata).mcp_enabled
}

pub fn enabled_mcp_ids_from_metadata(metadata: Option<&Value>) -> Vec<String> {
    ChatRuntimeMetadata::from_metadata(metadata).enabled_mcp_ids
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
    fn skill_ref(index: usize) -> String {
        format!("SK{}", index + 1)
    }
    fn plugin_ref(index: usize) -> String {
        format!("PL{}", index + 1)
    }

    #[derive(Clone)]
    struct SkillPromptEntry {
        skill_ref: String,
        name: Option<String>,
        plugin_source: Option<String>,
        description: Option<String>,
        source_type: String,
    }
    #[derive(Clone)]
    struct PluginPromptEntry {
        plugin_ref: String,
        name: Option<String>,
    }

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
    lines.push("关联技能（使用 skill_ref，避免长随机ID）：".to_string());
    let mut skill_entries: Vec<SkillPromptEntry> = Vec::new();
    if !agent.runtime_skills.is_empty() {
        for (index, skill) in agent.runtime_skills.iter().enumerate() {
            let entry = SkillPromptEntry {
                skill_ref: skill_ref(index),
                name: normalize_optional_string(Some(skill.name.clone())),
                plugin_source: skill
                    .plugin_source
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
                description: skill
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
                source_type: skill.source_type.trim().to_string(),
            };
            let mut parts = vec![format!("skill_ref={}", entry.skill_ref)];
            if let Some(name) = entry.name.as_deref() {
                parts.push(format!("名称={}", name));
            }
            if let Some(plugin_source) = entry.plugin_source.as_deref() {
                parts.push(format!("plugin_source={}", plugin_source));
            }
            parts.push(format!(
                "简介={}",
                entry.description.as_deref().unwrap_or("未提供")
            ));
            parts.push(format!("来源类型={}", entry.source_type));
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
            skill_entries.push(entry);
        }
    } else if !agent.skill_ids.is_empty() {
        for (index, _skill_id) in agent.skill_ids.iter().enumerate() {
            let entry = SkillPromptEntry {
                skill_ref: skill_ref(index),
                name: None,
                plugin_source: None,
                description: None,
                source_type: "skill_center".to_string(),
            };
            lines.push(format!(
                "{}. skill_ref={} | 简介=未提供 | 来源类型={} | 详情可通过工具查询",
                index + 1,
                entry.skill_ref,
                entry.source_type
            ));
            skill_entries.push(entry);
        }
    } else {
        lines.push("无".to_string());
    }

    lines.push(String::new());
    lines.push("关联插件（使用 plugin_ref，仅给简介）：".to_string());
    let mut plugin_entries: Vec<PluginPromptEntry> = Vec::new();
    if !agent.runtime_plugins.is_empty() {
        for (index, plugin) in agent.runtime_plugins.iter().enumerate() {
            let plugin_source = plugin.source.trim().to_string();
            let mut parts = vec![
                format!("plugin_ref={}", plugin_ref(index)),
                format!("plugin_source={}", plugin_source),
                format!("名称={}", plugin.name.trim()),
            ];
            if let Some(category) = plugin.category.as_deref().map(str::trim) {
                if !category.is_empty() {
                    parts.push(format!("分类={}", category));
                }
            }
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
            parts.push(format!("简介={}", description));
            let related_skills = skill_entries
                .iter()
                .filter(|entry| {
                    entry
                        .plugin_source
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value == plugin_source)
                        .unwrap_or(false)
                })
                .map(|entry| {
                    let skill_name = entry.name.as_deref().unwrap_or("未命名技能");
                    format!("{}({})", entry.skill_ref, skill_name)
                })
                .collect::<Vec<_>>();
            if !related_skills.is_empty() {
                parts.push(format!("覆盖技能={}", related_skills.join(", ")));
            }
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
            plugin_entries.push(PluginPromptEntry {
                plugin_ref: plugin_ref(index),
                name: normalize_optional_string(Some(plugin.name.clone())),
            });
        }
    } else if !agent.plugin_sources.is_empty() {
        for (index, source) in agent.plugin_sources.iter().enumerate() {
            let source = source.trim().to_string();
            let related_skills = skill_entries
                .iter()
                .filter(|entry| {
                    entry
                        .plugin_source
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value == source)
                        .unwrap_or(false)
                })
                .map(|entry| {
                    let skill_name = entry.name.as_deref().unwrap_or("未命名技能");
                    format!("{}({})", entry.skill_ref, skill_name)
                })
                .collect::<Vec<_>>();
            let mut parts = vec![
                format!("plugin_ref={}", plugin_ref(index)),
                format!("plugin_source={}", source),
                "简介=未提供".to_string(),
            ];
            if !related_skills.is_empty() {
                parts.push(format!("覆盖技能={}", related_skills.join(", ")));
            }
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
            plugin_entries.push(PluginPromptEntry {
                plugin_ref: plugin_ref(index),
                name: None,
            });
        }
    } else {
        lines.push("无".to_string());
    }

    lines.push(String::new());
    lines.push("关联命令（使用 command_ref）：".to_string());
    if !agent.runtime_commands.is_empty() {
        for (index, command) in agent.runtime_commands.iter().enumerate() {
            let mut parts = vec![
                format!("command_ref={}", command.command_ref.trim()),
                format!("名称={}", command.name.trim()),
                format!("plugin_source={}", command.plugin_source.trim()),
            ];
            parts.push(format!(
                "简介={}",
                command
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("未提供")
            ));
            if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
                if !argument_hint.is_empty() {
                    parts.push(format!("参数提示={}", argument_hint));
                }
            }
            if let Some(source_path) = normalize_optional_string(Some(command.source_path.clone()))
            {
                parts.push(format!("source_path={}", source_path));
            }
            lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
        }
    } else {
        lines.push("无".to_string());
    }

    if !agent.runtime_commands.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "如果需要查看某个 command 的完整内容，请调用内置工具 `{}`，仅传 `command_ref`（如 `CMD1`）。",
            CONTACT_COMMAND_READER_TOOL_NAME
        ));
    }

    if !plugin_entries.is_empty() {
        lines.push(String::new());
        let plugin_examples = plugin_entries
            .iter()
            .take(3)
            .map(|entry| match entry.name.as_deref() {
                Some(name) => format!("{}({})", entry.plugin_ref, name),
                None => entry.plugin_ref.clone(),
            })
            .collect::<Vec<_>>();
        if plugin_examples.is_empty() {
            lines.push(format!(
                "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 `PL1`）。",
                CONTACT_PLUGIN_READER_TOOL_NAME
            ));
        } else {
            lines.push(format!(
                "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 {}）。",
                CONTACT_PLUGIN_READER_TOOL_NAME,
                plugin_examples.join(", ")
            ));
        }
    }

    if !skill_entries.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "如果需要查看某个 skill 的完整内容，请调用内置工具 `{}`，仅传 `skill_ref`（如 `SK1`）。",
            CONTACT_SKILL_READER_TOOL_NAME
        ));
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
        ChatRuntimeMetadata,
        parse_contact_command_invocation, parse_implicit_command_selections_from_tools_end,
        remote_connection_id_from_metadata, CONTACT_COMMAND_READER_TOOL_NAME,
        CONTACT_PLUGIN_READER_TOOL_NAME, CONTACT_SKILL_READER_TOOL_NAME,
    };
    use crate::services::memory_server_client::{
        MemoryAgentRuntimeCommandSummaryDto, MemoryAgentRuntimeContextDto,
        MemoryAgentRuntimePluginSummaryDto, MemoryAgentRuntimeSkillSummaryDto,
    };
    use serde_json::json;

    #[test]
    fn builds_contact_prompt_with_plugin_and_skill_summaries() {
        let prompt = compose_contact_system_prompt(Some(&MemoryAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            name: "小林".to_string(),
            description: Some("负责前端排障".to_string()),
            category: Some("frontend".to_string()),
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
        assert!(prompt.contains("plugin_source=frontend_toolkit"));
        assert!(prompt.contains("plugin_ref=PL1"));
        assert!(prompt.contains("skill_ref=SK1"));
        assert!(prompt.contains("覆盖技能=SK1(组件排障)"));
        assert!(prompt.contains("command_ref=CMD1"));
        assert!(prompt.contains(CONTACT_COMMAND_READER_TOOL_NAME));
        assert!(prompt.contains(CONTACT_PLUGIN_READER_TOOL_NAME));
        assert!(prompt.contains(CONTACT_SKILL_READER_TOOL_NAME));
    }

    #[test]
    fn parses_explicit_contact_command_invocation() {
        let runtime_context = MemoryAgentRuntimeContextDto {
            agent_id: "agent_1".to_string(),
            name: "小林".to_string(),
            description: None,
            category: None,
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
            remote_connection_id_from_metadata(Some(&metadata)),
            Some("conn_1".to_string())
        );

        let metadata = json!({
            "chat_runtime": {
                "remote_connection_id": "conn_2"
            }
        });
        assert_eq!(
            remote_connection_id_from_metadata(Some(&metadata)),
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
