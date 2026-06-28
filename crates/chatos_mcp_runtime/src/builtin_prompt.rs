use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::builtin_catalog::{builtin_kind_by_any, BuiltinMcpKind};
use crate::naming::canonical_prefixed_tool_name;
use crate::types::{McpBuiltinServer, ToolInfo};

mod sections;

#[cfg(test)]
mod tests;

use sections::{prompt_section_registry, PromptSectionRegistry};

const SECTION_GLOBAL: &str = "global";
const SECTION_TASK_MANAGER: &str = "builtin_task_manager";
const SECTION_PROJECT_MANAGEMENT: &str = "builtin_project_management";
const SECTION_ASK_USER: &str = "builtin_ask_user";
const SECTION_CODE_MAINTAINER_READ: &str = "builtin_code_maintainer_read";
const SECTION_CODE_MAINTAINER_WRITE: &str = "builtin_code_maintainer_write";
const SECTION_TERMINAL_CONTROLLER: &str = "builtin_terminal_controller";
const SECTION_REMOTE_CONNECTION_CONTROLLER: &str = "builtin_remote_connection_controller";
const SECTION_BROWSER_TOOLS: &str = "builtin_browser_tools";
const SECTION_WEB_TOOLS: &str = "builtin_web_tools";
const SECTION_NOTEPAD: &str = "builtin_notepad";
const SECTION_CONDITIONAL_CONTACT_MEMORY_READERS: &str = "conditional_contact_memory_readers";
const SECTION_RUNTIME_LIMITATIONS: &str = "runtime_limitations";

const SECTION_ORDER: &[&str] = &[
    SECTION_GLOBAL,
    SECTION_TASK_MANAGER,
    SECTION_PROJECT_MANAGEMENT,
    SECTION_ASK_USER,
    SECTION_CODE_MAINTAINER_READ,
    SECTION_CODE_MAINTAINER_WRITE,
    SECTION_TERMINAL_CONTROLLER,
    SECTION_REMOTE_CONNECTION_CONTROLLER,
    SECTION_BROWSER_TOOLS,
    SECTION_WEB_TOOLS,
    SECTION_NOTEPAD,
    SECTION_CONDITIONAL_CONTACT_MEMORY_READERS,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinMcpPromptLocale {
    ZhCn,
    EnUs,
}

impl BuiltinMcpPromptLocale {
    pub const DEFAULT_KEY: &'static str = "zh-CN";
    pub const ENGLISH_KEY: &'static str = "en-US";

    pub fn from_key(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or(Self::DEFAULT_KEY)
        {
            Self::ENGLISH_KEY => Self::EnUs,
            _ => Self::ZhCn,
        }
    }

    pub fn is_english(self) -> bool {
        matches!(self, Self::EnUs)
    }
}

impl Default for BuiltinMcpPromptLocale {
    fn default() -> Self {
        Self::ZhCn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinMcpPromptBuildResult {
    pub prompt: Option<String>,
    pub selected_section_ids: Vec<String>,
    pub omitted_section_ids: Vec<String>,
    pub requested_builtin_server_names: Vec<String>,
    pub active_builtin_server_names: Vec<String>,
    pub omitted_builtin_server_names: Vec<String>,
    pub runtime_limitations: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct ServerAvailability {
    available_prefixed_tool_names: Vec<String>,
    unavailable_tools: Vec<UnavailableBuiltinTool>,
}

#[derive(Debug, Clone)]
struct UnavailableBuiltinTool {
    prefixed_name: String,
    reason: String,
}

pub fn builtin_mcp_prompt_source_path(locale: BuiltinMcpPromptLocale) -> &'static str {
    sections::builtin_mcp_prompt_source_path(locale)
}

pub fn builtin_mcp_prompt_section_ids(locale: BuiltinMcpPromptLocale) -> Vec<String> {
    prompt_section_registry(locale).ordered_ids.clone()
}

pub fn compose_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: BuiltinMcpPromptLocale,
) -> Option<String> {
    inspect_builtin_mcp_system_prompt(builtin_servers, locale).prompt
}

pub fn inspect_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: BuiltinMcpPromptLocale,
) -> BuiltinMcpPromptBuildResult {
    inspect_builtin_prompt(builtin_servers, prompt_section_registry(locale))
}

pub fn compose_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: BuiltinMcpPromptLocale,
) -> Option<String> {
    inspect_effective_builtin_mcp_system_prompt(
        builtin_servers,
        tool_metadata,
        unavailable_tools,
        locale,
    )
    .prompt
}

pub fn inspect_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: BuiltinMcpPromptLocale,
) -> BuiltinMcpPromptBuildResult {
    inspect_effective_prompt(
        builtin_servers,
        tool_metadata,
        unavailable_tools,
        prompt_section_registry(locale),
    )
}

fn section_id_for_kind(kind: BuiltinMcpKind) -> Option<&'static str> {
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => Some(SECTION_CODE_MAINTAINER_READ),
        BuiltinMcpKind::CodeMaintainerWrite => Some(SECTION_CODE_MAINTAINER_WRITE),
        BuiltinMcpKind::TerminalController => Some(SECTION_TERMINAL_CONTROLLER),
        BuiltinMcpKind::TaskManager => Some(SECTION_TASK_MANAGER),
        BuiltinMcpKind::ProjectManagement => Some(SECTION_PROJECT_MANAGEMENT),
        BuiltinMcpKind::Notepad => Some(SECTION_NOTEPAD),
        BuiltinMcpKind::AskUser => Some(SECTION_ASK_USER),
        BuiltinMcpKind::RemoteConnectionController => Some(SECTION_REMOTE_CONNECTION_CONTROLLER),
        BuiltinMcpKind::WebTools => Some(SECTION_WEB_TOOLS),
        BuiltinMcpKind::BrowserTools => Some(SECTION_BROWSER_TOOLS),
        BuiltinMcpKind::MemorySkillReader
        | BuiltinMcpKind::MemoryCommandReader
        | BuiltinMcpKind::MemoryPluginReader => Some(SECTION_CONDITIONAL_CONTACT_MEMORY_READERS),
        BuiltinMcpKind::AgentBuilder => None,
    }
}

fn section_id_for_server(server: &McpBuiltinServer) -> Option<&'static str> {
    builtin_kind_by_any(server.kind.as_str()).and_then(section_id_for_kind)
}

fn collect_candidate_section_ids(builtin_servers: &[McpBuiltinServer]) -> HashSet<&'static str> {
    builtin_servers
        .iter()
        .filter_map(section_id_for_server)
        .collect()
}

fn compose_prompt_from_selected_sections(
    selected_sections: &HashSet<&'static str>,
    runtime_limitations: Option<String>,
    registry: &PromptSectionRegistry,
) -> Option<String> {
    let mut parts: Vec<String> = SECTION_ORDER
        .iter()
        .filter(|section_id| selected_sections.contains(**section_id))
        .filter_map(|section_id| registry.sections.get(*section_id))
        .map(|content| content.trim())
        .filter(|content| !content.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if let Some(limitations) = runtime_limitations
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
    {
        parts.push(limitations);
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn collect_server_names(builtin_servers: &[McpBuiltinServer]) -> Vec<String> {
    let mut names = builtin_servers
        .iter()
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();
    sort_dedup(&mut names);
    names
}

fn compute_omitted_server_names(
    requested_builtin_server_names: &[String],
    active_builtin_server_names: &[String],
) -> Vec<String> {
    let active = active_builtin_server_names
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    requested_builtin_server_names
        .iter()
        .filter(|name| !active.contains(name.as_str()))
        .cloned()
        .collect()
}

fn inspect_builtin_prompt(
    builtin_servers: &[McpBuiltinServer],
    registry: &PromptSectionRegistry,
) -> BuiltinMcpPromptBuildResult {
    let requested_builtin_server_names = collect_server_names(builtin_servers);
    let candidate_sections = collect_candidate_section_ids(builtin_servers);
    let mut selected_sections = candidate_sections.clone();
    let mut active_builtin_server_names = builtin_servers
        .iter()
        .filter(|server| section_id_for_server(server).is_some())
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();

    sort_dedup(&mut active_builtin_server_names);

    if !selected_sections.is_empty() {
        selected_sections.insert(SECTION_GLOBAL);
    }

    let omitted_builtin_server_names = compute_omitted_server_names(
        &requested_builtin_server_names,
        &active_builtin_server_names,
    );

    BuiltinMcpPromptBuildResult {
        prompt: compose_prompt_from_selected_sections(&selected_sections, None, registry),
        selected_section_ids: ordered_section_ids(&selected_sections),
        omitted_section_ids: Vec::new(),
        requested_builtin_server_names,
        active_builtin_server_names,
        omitted_builtin_server_names,
        runtime_limitations: None,
    }
}

fn collect_server_availability(
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    builtin_servers: &[McpBuiltinServer],
) -> HashMap<String, ServerAvailability> {
    let mut by_server: HashMap<String, ServerAvailability> = builtin_servers
        .iter()
        .map(|server| (server.name.clone(), ServerAvailability::default()))
        .collect();

    for (prefixed_name, info) in tool_metadata {
        if info.server_type != "builtin" {
            continue;
        }
        by_server
            .entry(info.server_name.clone())
            .or_default()
            .available_prefixed_tool_names
            .push(prefixed_name.clone());
    }

    for item in unavailable_tools {
        let Some(server_name) = item
            .get("server_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(tool_name) = item
            .get("tool_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        by_server
            .entry(server_name.to_string())
            .or_default()
            .unavailable_tools
            .push(UnavailableBuiltinTool {
                prefixed_name: canonical_prefixed_tool_name(server_name, tool_name),
                reason,
            });
    }

    for status in by_server.values_mut() {
        sort_dedup(&mut status.available_prefixed_tool_names);
        status
            .unavailable_tools
            .sort_by(|left, right| left.prefixed_name.cmp(&right.prefixed_name));
        status.unavailable_tools.dedup_by(|left, right| {
            left.prefixed_name == right.prefixed_name && left.reason == right.reason
        });
    }

    by_server
}

fn build_runtime_limitations(
    builtin_servers: &[McpBuiltinServer],
    selected_sections: &HashSet<&'static str>,
    availability_by_server: &HashMap<String, ServerAvailability>,
    registry: &PromptSectionRegistry,
) -> Option<String> {
    let mut lines = Vec::new();

    for server in builtin_servers {
        let Some(section_id) = section_id_for_server(server) else {
            continue;
        };
        if !selected_sections.contains(section_id) {
            continue;
        }
        let Some(status) = availability_by_server.get(server.name.as_str()) else {
            continue;
        };
        if status.unavailable_tools.is_empty() {
            continue;
        }

        let tool_list = status
            .unavailable_tools
            .iter()
            .map(|item| format!("`{}`", item.prefixed_name))
            .collect::<Vec<_>>()
            .join(", ");
        if tool_list.is_empty() {
            continue;
        }

        let mut unique_reasons = Vec::new();
        for tool in &status.unavailable_tools {
            let reason = tool.reason.trim();
            if reason.is_empty() || unique_reasons.iter().any(|item: &String| item == reason) {
                continue;
            }
            unique_reasons.push(reason.to_string());
        }

        if unique_reasons.is_empty() {
            lines.push(format!("- 当前不要依赖以下内置 MCP 工具：{tool_list}。"));
        } else {
            lines.push(format!(
                "- 当前不要依赖以下内置 MCP 工具：{}。原因：{}。",
                tool_list,
                unique_reasons.join("；")
            ));
        }
    }

    if lines.is_empty() {
        return None;
    }

    let mut content = registry
        .sections
        .get(SECTION_RUNTIME_LIMITATIONS)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "当前运行时限制：".to_string());

    for line in lines {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(line.as_str());
    }

    Some(content)
}

fn inspect_effective_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    registry: &PromptSectionRegistry,
) -> BuiltinMcpPromptBuildResult {
    let requested_builtin_server_names = collect_server_names(builtin_servers);
    let availability_by_server =
        collect_server_availability(tool_metadata, unavailable_tools, builtin_servers);
    let candidate_sections = collect_candidate_section_ids(builtin_servers);
    let mut selected_sections: HashSet<&'static str> = HashSet::new();
    let mut active_builtin_server_names = Vec::new();
    let mut omitted_builtin_server_names = Vec::new();

    for server in builtin_servers {
        let Some(section_id) = section_id_for_server(server) else {
            omitted_builtin_server_names.push(server.name.clone());
            continue;
        };

        if availability_by_server
            .get(server.name.as_str())
            .is_some_and(|status| !status.available_prefixed_tool_names.is_empty())
        {
            selected_sections.insert(section_id);
            active_builtin_server_names.push(server.name.clone());
        } else {
            omitted_builtin_server_names.push(server.name.clone());
        }
    }

    sort_dedup(&mut active_builtin_server_names);
    sort_dedup(&mut omitted_builtin_server_names);

    if !selected_sections.is_empty() {
        selected_sections.insert(SECTION_GLOBAL);
    }

    let runtime_limitations = build_runtime_limitations(
        builtin_servers,
        &selected_sections,
        &availability_by_server,
        registry,
    );
    let omitted_section_ids = ordered_difference(&candidate_sections, &selected_sections);

    BuiltinMcpPromptBuildResult {
        prompt: compose_prompt_from_selected_sections(
            &selected_sections,
            runtime_limitations.clone(),
            registry,
        ),
        selected_section_ids: ordered_section_ids(&selected_sections),
        omitted_section_ids,
        requested_builtin_server_names,
        active_builtin_server_names,
        omitted_builtin_server_names,
        runtime_limitations,
    }
}

fn ordered_section_ids(section_ids: &HashSet<&'static str>) -> Vec<String> {
    SECTION_ORDER
        .iter()
        .filter(|section_id| section_ids.contains(**section_id))
        .map(|section_id| (*section_id).to_string())
        .collect()
}

fn ordered_difference(
    candidate_sections: &HashSet<&'static str>,
    selected_sections: &HashSet<&'static str>,
) -> Vec<String> {
    SECTION_ORDER
        .iter()
        .filter(|section_id| {
            **section_id != SECTION_GLOBAL
                && candidate_sections.contains(**section_id)
                && !selected_sections.contains(**section_id)
        })
        .map(|section_id| (*section_id).to_string())
        .collect()
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}
