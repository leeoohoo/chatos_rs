use std::collections::{HashMap, HashSet};

use crate::services::mcp_loader::McpBuiltinServer;

use super::sections::{
    ordered_section_ids, section_id_for_kind, sort_dedup, PromptSectionRegistry, SECTION_GLOBAL,
    SECTION_ORDER,
};

pub(super) fn collect_candidate_section_ids(
    builtin_servers: &[McpBuiltinServer],
) -> HashSet<&'static str> {
    builtin_servers
        .iter()
        .filter_map(|server| section_id_for_kind(server.kind))
        .collect()
}

pub(super) fn compose_prompt_from_selected_sections(
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

pub(super) fn collect_server_names(builtin_servers: &[McpBuiltinServer]) -> Vec<String> {
    let mut names = builtin_servers
        .iter()
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();
    sort_dedup(&mut names);
    names
}

pub(super) fn compute_omitted_server_names(
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

pub(super) fn inspect_builtin_prompt(
    builtin_servers: &[McpBuiltinServer],
    registry: &PromptSectionRegistry,
) -> super::BuiltinMcpPromptBuildResult {
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

    let omitted_builtin_server_names = compute_omitted_server_names(
        &requested_builtin_server_names,
        &active_builtin_server_names,
    );

    super::BuiltinMcpPromptBuildResult {
        prompt: compose_prompt_from_selected_sections(&selected_sections, None, registry),
        selected_section_ids: ordered_section_ids(&selected_sections),
        omitted_section_ids: Vec::new(),
        requested_builtin_server_names,
        active_builtin_server_names,
        omitted_builtin_server_names,
        runtime_limitations: None,
    }
}

#[allow(dead_code)]
fn _keep_hash_map_reference(_: &HashMap<String, String>) {}
