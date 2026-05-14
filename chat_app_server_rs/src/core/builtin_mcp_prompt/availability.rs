use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::sections::{
    ordered_difference, ordered_section_ids, section_id_for_kind, sort_dedup,
    PromptSectionRegistry, SECTION_GLOBAL, SECTION_ORDER, SECTION_RUNTIME_LIMITATIONS,
};
use crate::core::mcp_tools::ToolInfo;
use crate::services::mcp_loader::McpBuiltinServer;

#[derive(Debug, Default, Clone)]
pub(super) struct ServerAvailability {
    pub(super) available_prefixed_tool_names: Vec<String>,
    pub(super) unavailable_tools: Vec<UnavailableBuiltinTool>,
}

#[derive(Debug, Clone)]
pub(super) struct UnavailableBuiltinTool {
    pub(super) prefixed_name: String,
    pub(super) reason: String,
}

pub(super) fn collect_server_availability(
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

pub(super) fn build_runtime_limitations(
    builtin_servers: &[McpBuiltinServer],
    selected_sections: &HashSet<&'static str>,
    availability_by_server: &HashMap<String, ServerAvailability>,
    registry: &PromptSectionRegistry,
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
            lines.push(format!("- 当前不要依赖以下内置 MCP 工具：{}。", tool_list));
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

pub(super) fn inspect_effective_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    registry: &PromptSectionRegistry,
) -> super::BuiltinMcpPromptBuildResult {
    let requested_builtin_server_names = super::compose::collect_server_names(builtin_servers);
    let availability_by_server =
        collect_server_availability(tool_metadata, unavailable_tools, builtin_servers);
    let candidate_sections = super::compose::collect_candidate_section_ids(builtin_servers);
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

    let runtime_limitations = build_runtime_limitations(
        builtin_servers,
        &selected_sections,
        &availability_by_server,
        registry,
    );
    let omitted_section_ids = ordered_difference(&candidate_sections, &selected_sections);

    super::BuiltinMcpPromptBuildResult {
        prompt: super::compose::compose_prompt_from_selected_sections(
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

#[allow(dead_code)]
fn _keep_section_order_reference() -> &'static [&'static str] {
    SECTION_ORDER
}
