// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, ResolvedMcp, ResolvedSkill};

use crate::mcp::manifest::LocalMcpManifestRecord;

pub(super) fn filter_builtin_kinds(
    capabilities: &ResolvedAgentCapabilities,
    candidates: Vec<BuiltinMcpKind>,
    explicit_selection: bool,
) -> Result<Vec<BuiltinMcpKind>, String> {
    candidates
        .into_iter()
        .filter_map(|kind| match resolved_builtin(capabilities, kind) {
            Some(item) if mcp_is_available(item) => Some(Ok(kind)),
            Some(item) if item.binding.required || explicit_selection => Some(Err(format!(
                "Plugin policy does not allow local MCP {}: {}",
                kind.kind_name(),
                item.reason.as_deref().unwrap_or(item.status.as_str())
            ))),
            None if explicit_selection => Some(Err(format!(
                "Plugin policy does not contain local MCP {}",
                kind.kind_name()
            ))),
            _ => None,
        })
        .collect()
}

pub(super) fn filter_manifests(
    capabilities: &ResolvedAgentCapabilities,
    candidates: Vec<LocalMcpManifestRecord>,
    explicit_selection: bool,
) -> Result<Vec<LocalMcpManifestRecord>, String> {
    candidates
        .into_iter()
        .filter_map(|manifest| {
            let Some(plugin_mcp_id) = manifest.plugin_mcp_id.as_deref() else {
                return explicit_selection.then(|| {
                    Err(format!(
                        "Local MCP has no Plugin Management identity: {}",
                        manifest.display_name
                    ))
                });
            };
            match resolved_mcp(capabilities, plugin_mcp_id) {
                Some(item) if mcp_is_available(item) => Some(Ok(manifest)),
                Some(item) if item.binding.required || explicit_selection => Some(Err(format!(
                    "Plugin policy does not allow local MCP {}: {}",
                    manifest.display_name,
                    item.reason.as_deref().unwrap_or(item.status.as_str())
                ))),
                None if explicit_selection => Some(Err(format!(
                    "Plugin policy does not contain local MCP {}",
                    manifest.display_name
                ))),
                _ => None,
            }
        })
        .collect()
}

pub(super) fn effective_skills<'a>(
    capabilities: &'a ResolvedAgentCapabilities,
    selected_ids: &[String],
) -> Result<Vec<&'a ResolvedSkill>, String> {
    let mut effective = Vec::new();
    for skill in capabilities
        .skills
        .iter()
        .filter(|skill| skill.binding.required)
    {
        if !skill_is_available(skill) {
            return Err(format!(
                "Required local Skill is unavailable: {}",
                skill.resource.display_name
            ));
        }
        effective.push(skill);
    }
    for selected_id in selected_ids {
        let skill = capabilities
            .skills
            .iter()
            .find(|skill| skill.resource.id == *selected_id)
            .ok_or_else(|| format!("Plugin policy does not contain Skill {selected_id}"))?;
        if !skill.binding.required && !skill_is_available(skill) {
            return Err(format!(
                "Plugin policy does not allow selected Skill: {}",
                skill.resource.display_name
            ));
        }
        if !effective
            .iter()
            .any(|existing| existing.resource.id == skill.resource.id)
        {
            effective.push(skill);
        }
    }
    Ok(effective)
}

pub(super) fn parse_ids(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .fold(Vec::new(), |mut values, value| {
            if !values.contains(&value) {
                values.push(value);
            }
            values
        })
}

fn resolved_builtin(
    capabilities: &ResolvedAgentCapabilities,
    kind: BuiltinMcpKind,
) -> Option<&ResolvedMcp> {
    capabilities.mcps.iter().find(|item| {
        chatos_mcp::system_mcp_descriptor_for_record(&item.resource)
            .is_some_and(|descriptor| descriptor.embedded_kind == Some(kind))
    })
}

fn resolved_mcp<'a>(
    capabilities: &'a ResolvedAgentCapabilities,
    resource_id: &str,
) -> Option<&'a ResolvedMcp> {
    capabilities
        .mcps
        .iter()
        .find(|item| item.resource.id == resource_id)
}

fn mcp_is_available(item: &ResolvedMcp) -> bool {
    item.available && item.binding.enabled && item.resource.enabled
}

fn skill_is_available(item: &ResolvedSkill) -> bool {
    item.available && item.binding.enabled && item.resource.enabled
}
