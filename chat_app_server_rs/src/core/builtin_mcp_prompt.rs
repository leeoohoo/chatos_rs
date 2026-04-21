use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::Value;

use crate::core::mcp_tools::ToolInfo;
use crate::services::builtin_mcp::BuiltinMcpKind;
use crate::services::mcp_loader::McpBuiltinServer;

const SECTION_GLOBAL: &str = "global";
const SECTION_TASK_MANAGER: &str = "builtin_task_manager";
const SECTION_UI_PROMPTER: &str = "builtin_ui_prompter";
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
    SECTION_UI_PROMPTER,
    SECTION_CODE_MAINTAINER_READ,
    SECTION_CODE_MAINTAINER_WRITE,
    SECTION_TERMINAL_CONTROLLER,
    SECTION_REMOTE_CONNECTION_CONTROLLER,
    SECTION_BROWSER_TOOLS,
    SECTION_WEB_TOOLS,
    SECTION_NOTEPAD,
    SECTION_CONDITIONAL_CONTACT_MEMORY_READERS,
];

const BUILTIN_MCP_PROMPT_SOURCE_PATH: &str = "BUILTIN_MCP_PROMPT.md";
const BUILTIN_MCP_PROMPT_SOURCE: &str = include_str!("../../../BUILTIN_MCP_PROMPT.md");

#[derive(Debug, Clone)]
struct PromptSectionRegistry {
    ordered_ids: Vec<String>,
    sections: HashMap<String, String>,
}

static PROMPT_SECTION_REGISTRY: Lazy<PromptSectionRegistry> =
    Lazy::new(|| parse_prompt_sections(BUILTIN_MCP_PROMPT_SOURCE));

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuiltinMcpPromptBuildResult {
    pub prompt: Option<String>,
    pub selected_section_ids: Vec<String>,
    pub omitted_section_ids: Vec<String>,
    pub requested_builtin_server_names: Vec<String>,
    pub active_builtin_server_names: Vec<String>,
    pub omitted_builtin_server_names: Vec<String>,
    pub runtime_limitations: Option<String>,
}

pub fn builtin_mcp_prompt_source_path() -> &'static str {
    BUILTIN_MCP_PROMPT_SOURCE_PATH
}

pub fn builtin_mcp_prompt_section_ids() -> Vec<String> {
    PROMPT_SECTION_REGISTRY.ordered_ids.clone()
}

pub fn compose_builtin_mcp_system_prompt(builtin_servers: &[McpBuiltinServer]) -> Option<String> {
    inspect_builtin_mcp_system_prompt(builtin_servers).prompt
}

pub fn inspect_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
) -> BuiltinMcpPromptBuildResult {
    let requested_builtin_server_names = collect_server_names(builtin_servers);
    let candidate_sections = collect_candidate_section_ids(builtin_servers);
    let mut selected_sections = candidate_sections.clone();
    let mut active_builtin_server_names = builtin_servers
        .iter()
        .filter(|server| section_id_for_kind(server.kind).is_some())
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();

    sort_dedup(&mut active_builtin_server_names);

    if !selected_sections.is_empty() {
        selected_sections.insert(SECTION_GLOBAL);
    }

    let omitted_builtin_server_names =
        compute_omitted_server_names(&requested_builtin_server_names, &active_builtin_server_names);

    BuiltinMcpPromptBuildResult {
        prompt: compose_prompt_from_selected_sections(&selected_sections, None),
        selected_section_ids: ordered_section_ids(&selected_sections),
        omitted_section_ids: Vec::new(),
        requested_builtin_server_names,
        active_builtin_server_names,
        omitted_builtin_server_names,
        runtime_limitations: None,
    }
}

pub fn compose_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
) -> Option<String> {
    inspect_effective_builtin_mcp_system_prompt(builtin_servers, tool_metadata, unavailable_tools)
        .prompt
}

pub fn inspect_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
) -> BuiltinMcpPromptBuildResult {
    let requested_builtin_server_names = collect_server_names(builtin_servers);
    let availability_by_server =
        collect_server_availability(tool_metadata, unavailable_tools, builtin_servers);
    let candidate_sections = collect_candidate_section_ids(builtin_servers);
    let mut selected_sections: HashSet<&'static str> = HashSet::new();
    let mut active_builtin_server_names = Vec::new();
    let mut omitted_builtin_server_names = Vec::new();

    for server in builtin_servers {
        let Some(section_id) = section_id_for_kind(server.kind) else {
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

    let runtime_limitations =
        build_runtime_limitations(builtin_servers, &selected_sections, &availability_by_server);
    let omitted_section_ids = ordered_difference(&candidate_sections, &selected_sections);

    BuiltinMcpPromptBuildResult {
        prompt: compose_prompt_from_selected_sections(
            &selected_sections,
            runtime_limitations.clone(),
        ),
        selected_section_ids: ordered_section_ids(&selected_sections),
        omitted_section_ids,
        requested_builtin_server_names,
        active_builtin_server_names,
        omitted_builtin_server_names,
        runtime_limitations,
    }
}

fn compose_prompt_from_selected_sections(
    selected_sections: &HashSet<&'static str>,
    runtime_limitations: Option<String>,
) -> Option<String> {
    let mut parts: Vec<String> = SECTION_ORDER
        .iter()
        .filter(|section_id| selected_sections.contains(**section_id))
        .filter_map(|section_id| PROMPT_SECTION_REGISTRY.sections.get(*section_id))
        .map(|content| content.trim())
        .filter(|content| !content.is_empty())
        .map(|content| content.to_string())
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

fn collect_candidate_section_ids(builtin_servers: &[McpBuiltinServer]) -> HashSet<&'static str> {
    builtin_servers
        .iter()
        .filter_map(|server| section_id_for_kind(server.kind))
        .collect()
}

fn section_id_for_kind(kind: BuiltinMcpKind) -> Option<&'static str> {
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => Some(SECTION_CODE_MAINTAINER_READ),
        BuiltinMcpKind::CodeMaintainerWrite => Some(SECTION_CODE_MAINTAINER_WRITE),
        BuiltinMcpKind::TerminalController => Some(SECTION_TERMINAL_CONTROLLER),
        BuiltinMcpKind::TaskManager => Some(SECTION_TASK_MANAGER),
        BuiltinMcpKind::Notepad => Some(SECTION_NOTEPAD),
        BuiltinMcpKind::UiPrompter => Some(SECTION_UI_PROMPTER),
        BuiltinMcpKind::RemoteConnectionController => Some(SECTION_REMOTE_CONNECTION_CONTROLLER),
        BuiltinMcpKind::WebTools => Some(SECTION_WEB_TOOLS),
        BuiltinMcpKind::BrowserTools => Some(SECTION_BROWSER_TOOLS),
        BuiltinMcpKind::MemorySkillReader
        | BuiltinMcpKind::MemoryCommandReader
        | BuiltinMcpKind::MemoryPluginReader => Some(SECTION_CONDITIONAL_CONTACT_MEMORY_READERS),
        BuiltinMcpKind::AgentBuilder => None,
    }
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
                prefixed_name: format!("{}_{}", server_name, tool_name),
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
) -> Option<String> {
    let mut lines = Vec::new();

    for server in builtin_servers {
        let Some(section_id) = section_id_for_kind(server.kind) else {
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
            lines.push(format!(
                "- 当前不要依赖以下内置 MCP 工具：{}。",
                tool_list
            ));
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

    let mut content = PROMPT_SECTION_REGISTRY
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

fn parse_prompt_sections(markdown: &str) -> PromptSectionRegistry {
    let mut ordered_ids = Vec::new();
    let mut sections = HashMap::new();
    let mut current_id: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in markdown.lines() {
        if let Some(section_id) = parse_section_header(line) {
            flush_section(
                &mut ordered_ids,
                &mut sections,
                &mut current_id,
                &mut current_lines,
            );
            current_id = Some(section_id.to_string());
            continue;
        }

        if current_id.is_some() {
            current_lines.push(line);
        }
    }

    flush_section(
        &mut ordered_ids,
        &mut sections,
        &mut current_id,
        &mut current_lines,
    );

    PromptSectionRegistry {
        ordered_ids,
        sections,
    }
}

fn flush_section(
    ordered_ids: &mut Vec<String>,
    sections: &mut HashMap<String, String>,
    current_id: &mut Option<String>,
    current_lines: &mut Vec<&str>,
) {
    let Some(section_id) = current_id.take() else {
        current_lines.clear();
        return;
    };

    let content = current_lines.join("\n").trim().to_string();
    current_lines.clear();
    if !content.is_empty() {
        if !ordered_ids.iter().any(|item| item == &section_id) {
            ordered_ids.push(section_id.clone());
        }
        sections.insert(section_id, content);
    }
}

fn parse_section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !(trimmed.starts_with("## [") && trimmed.ends_with(']')) {
        return None;
    }

    let inner = &trimmed[4..trimmed.len() - 1];
    let section_id = inner.trim();
    if section_id.is_empty() {
        None
    } else {
        Some(section_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
        compose_builtin_mcp_system_prompt, compose_effective_builtin_mcp_system_prompt,
        inspect_builtin_mcp_system_prompt, inspect_effective_builtin_mcp_system_prompt,
    };
    use crate::core::mcp_tools::ToolInfo;
    use crate::services::builtin_mcp::{
        BuiltinMcpKind, BROWSER_TOOLS_SERVER_NAME, WEB_TOOLS_SERVER_NAME,
    };
    use crate::services::mcp_loader::McpBuiltinServer;

    fn build_builtin_server(kind: BuiltinMcpKind) -> McpBuiltinServer {
        McpBuiltinServer {
            name: "builtin".to_string(),
            kind,
            workspace_dir: ".".to_string(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            allow_writes: false,
            max_file_bytes: 0,
            max_write_bytes: 0,
            search_limit: 0,
        }
    }

    #[test]
    fn source_metadata_exposes_prompt_path_and_sections() {
        assert_eq!(builtin_mcp_prompt_source_path(), "BUILTIN_MCP_PROMPT.md");
        let section_ids = builtin_mcp_prompt_section_ids();
        assert!(section_ids.iter().any(|item| item == "global"));
        assert!(section_ids.iter().any(|item| item == "runtime_limitations"));
    }

    #[test]
    fn returns_none_when_no_supported_builtin_sections_are_selected() {
        let prompt = compose_builtin_mcp_system_prompt(&[]);
        assert!(prompt.is_none());

        let prompt = compose_builtin_mcp_system_prompt(&[build_builtin_server(
            BuiltinMcpKind::AgentBuilder,
        )]);
        assert!(prompt.is_none());
    }

    #[test]
    fn inspect_builtin_prompt_marks_unsupported_servers_as_omitted() {
        let info = inspect_builtin_mcp_system_prompt(&[McpBuiltinServer {
            name: "agent_builder".to_string(),
            kind: BuiltinMcpKind::AgentBuilder,
            ..build_builtin_server(BuiltinMcpKind::AgentBuilder)
        }]);

        assert!(info.prompt.is_none());
        assert_eq!(info.requested_builtin_server_names, vec!["agent_builder"]);
        assert!(info.active_builtin_server_names.is_empty());
        assert_eq!(info.omitted_builtin_server_names, vec!["agent_builder"]);
    }

    #[test]
    fn includes_global_and_selected_sections_only() {
        let prompt = compose_builtin_mcp_system_prompt(&[
            build_builtin_server(BuiltinMcpKind::TaskManager),
            build_builtin_server(BuiltinMcpKind::UiPrompter),
        ])
        .expect("prompt");

        assert!(prompt.contains("你是 Chatos 中一个“内置 MCP 优先”的助手。"));
        assert!(prompt.contains("`task_manager_add_task`"));
        assert!(prompt.contains("`ui_prompter_prompt_choices`"));
        assert!(!prompt.contains("`code_maintainer_read_read_file`"));
    }

    #[test]
    fn keeps_browser_and_web_sections_together_in_stable_order() {
        let prompt = compose_builtin_mcp_system_prompt(&[
            build_builtin_server(BuiltinMcpKind::WebTools),
            build_builtin_server(BuiltinMcpKind::BrowserTools),
            build_builtin_server(BuiltinMcpKind::BrowserTools),
        ])
        .expect("prompt");

        let browser_idx = prompt
            .find(format!("`{}_browser_inspect`", BROWSER_TOOLS_SERVER_NAME).as_str())
            .expect("browser section");
        let web_idx = prompt
            .find(format!("`{}_web_research`", WEB_TOOLS_SERVER_NAME).as_str())
            .expect("web section");
        assert!(browser_idx < web_idx);
        assert_eq!(prompt.matches("`browser_tools_browser_inspect`").count(), 1);
    }

    #[test]
    fn includes_memory_reader_section_when_contact_reader_tools_are_present() {
        let prompt = compose_builtin_mcp_system_prompt(&[
            build_builtin_server(BuiltinMcpKind::MemorySkillReader),
            build_builtin_server(BuiltinMcpKind::MemoryCommandReader),
        ])
        .expect("prompt");

        assert!(prompt.contains("`memory_skill_reader_get_skill_detail`"));
        assert!(prompt.contains("`memory_command_reader_get_command_detail`"));
        assert!(prompt.contains("`memory_plugin_reader_get_plugin_detail`"));
    }

    #[test]
    fn effective_prompt_drops_fully_unavailable_sections() {
        let info = inspect_effective_builtin_mcp_system_prompt(
            &[build_builtin_server(BuiltinMcpKind::BrowserTools)],
            &HashMap::new(),
            &[json!({
                "server_name": "builtin",
                "tool_name": "browser_inspect",
                "reason": "agent-browser unavailable"
            })],
        );

        assert!(info.prompt.is_none());
        assert_eq!(info.omitted_section_ids, vec!["builtin_browser_tools"]);
        assert_eq!(info.omitted_builtin_server_names, vec!["builtin"]);
    }

    #[test]
    fn effective_prompt_keeps_available_sections_and_appends_runtime_limitations() {
        let mut tool_metadata = HashMap::new();
        tool_metadata.insert(
            "memory_skill_reader_get_skill_detail".to_string(),
            ToolInfo {
                original_name: "get_skill_detail".to_string(),
                server_name: "memory_skill_reader".to_string(),
                server_type: "builtin".to_string(),
                server_url: None,
                server_config: None,
                tool_info: json!({}),
            },
        );

        let prompt = compose_effective_builtin_mcp_system_prompt(
            &[
                McpBuiltinServer {
                    name: "memory_skill_reader".to_string(),
                    kind: BuiltinMcpKind::MemorySkillReader,
                    ..build_builtin_server(BuiltinMcpKind::MemorySkillReader)
                },
                McpBuiltinServer {
                    name: "memory_plugin_reader".to_string(),
                    kind: BuiltinMcpKind::MemoryPluginReader,
                    ..build_builtin_server(BuiltinMcpKind::MemoryPluginReader)
                },
            ],
            &tool_metadata,
            &[json!({
                "server_name": "memory_plugin_reader",
                "tool_name": "get_plugin_detail",
                "reason": "plugin source unavailable"
            })],
        )
        .expect("prompt");

        assert!(prompt.contains("`memory_skill_reader_get_skill_detail`"));
        assert!(prompt.contains("这一 section 由系统根据当前实际成功注册与失败不可用的内置 MCP 工具动态补全。"));
        assert!(prompt.contains("`memory_plugin_reader_get_plugin_detail`"));
        assert!(prompt.contains("plugin source unavailable"));
    }
}
