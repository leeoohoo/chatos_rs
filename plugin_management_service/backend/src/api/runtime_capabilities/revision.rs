// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn capability_policy_revision(
    agent: &SystemAgentRecord,
    mcps: &[ResolvedMcp],
    skills: &[ResolvedSkill],
    plugins: &[ResolvedPlugin],
) -> String {
    let mut revision_parts = vec![format!(
        "agent:{}:{}:{}:{}",
        agent.agent_key,
        agent.enabled,
        agent.tool_plane.as_str(),
        agent.updated_at
    )];
    revision_parts.extend(mcps.iter().map(|item| {
        format!(
            "mcp:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.extend(skills.iter().map(|item| {
        format!(
            "skill:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.extend(plugins.iter().map(plugin_revision_part));
    revision_parts.sort();
    let mut hasher = DefaultHasher::new();
    revision_parts.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn plugin_revision_part(item: &ResolvedPlugin) -> String {
    let release = item
        .release
        .as_ref()
        .map(|release| {
            format!(
                "{}:{}:{}:{}",
                release.id,
                release.version,
                release.artifact_sha256,
                release.revoked_at.is_some()
            )
        })
        .unwrap_or_else(|| "missing".to_string());
    let preference = item
        .preference
        .as_ref()
        .map(|preference| {
            format!(
                "{}:{}:{}:{}",
                preference.enabled,
                preference.release_channel,
                preference.enabled_components.join(","),
                preference.updated_at
            )
        })
        .unwrap_or_else(|| "missing".to_string());
    let installation = item
        .installation
        .as_ref()
        .map(|installation| {
            format!(
                "{}:{}:{}:{}:{}:{}:{:?}:{:?}:{:?}",
                installation.device_id,
                installation.release_id,
                installation.version,
                installation.artifact_sha256,
                installation.active,
                installation.last_checked_at,
                installation.dependency_status,
                installation.permission_status,
                installation.auth_status
            )
        })
        .unwrap_or_else(|| "missing".to_string());
    let components = item
        .components
        .iter()
        .map(|component| {
            format!(
                "{}:{:?}:{}:{:?}",
                component.component.component_key,
                component.component.kind,
                component.available,
                component.status
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let conditions = format!(
        "{:?}:{:?}:{:?}:{:?}",
        item.binding.conditions.task_profile,
        item.binding.conditions.project_source_type,
        item.binding.conditions.runtime_provider,
        item.binding.conditions.schedule_mode
    );
    format!(
        "plugin:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{installation}:{components}",
        item.catalog.id,
        item.catalog.enabled,
        item.catalog.updated_at,
        item.binding.required,
        item.binding.enabled,
        item.binding.updated_at,
        item.binding.component_allowlist.join(","),
        conditions,
        release,
        preference,
    )
}
