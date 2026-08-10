// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn remove_retired_system_agents(store: &AppStore) -> Result<(), String> {
    for agent_key in RETIRED_SYSTEM_AGENT_KEYS {
        store.delete_bindings_for_agent(agent_key).await?;
        store.delete_agent(agent_key).await?;
    }
    Ok(())
}

pub(super) async fn seed_agents(store: &AppStore) -> Result<(), String> {
    for (agent_key, display_name, service_name, description, include_user_resources, tool_plane) in
        system_agent_specs()
    {
        if let Some(mut existing) = store.get_agent(agent_key).await? {
            let mut changed = false;
            if existing.display_name != display_name {
                existing.display_name = display_name.to_string();
                changed = true;
            }
            if existing.service_name != service_name {
                existing.service_name = service_name.to_string();
                changed = true;
            }
            if existing.scope != "system_internal" {
                existing.scope = "system_internal".to_string();
                changed = true;
            }
            if existing.description.as_deref() != Some(description) {
                existing.description = Some(description.to_string());
                changed = true;
            }
            if existing.managed_by != "system" {
                existing.managed_by = "system".to_string();
                changed = true;
            }
            if existing.include_user_resources != include_user_resources {
                existing.include_user_resources = include_user_resources;
                changed = true;
            }
            if existing.tool_plane != tool_plane {
                existing.tool_plane = tool_plane;
                changed = true;
            }
            if changed {
                existing.updated_at = now_rfc3339();
                store.replace_agent(&existing).await?;
            }
            continue;
        }
        let now = now_rfc3339();
        let record = SystemAgentRecord {
            id: format!("system_agent_{agent_key}"),
            agent_key: agent_key.to_string(),
            display_name: display_name.to_string(),
            service_name: service_name.to_string(),
            scope: "system_internal".to_string(),
            description: Some(description.to_string()),
            enabled: true,
            managed_by: "system".to_string(),
            include_user_resources,
            tool_plane,
            plugin_component: PluginComponentOwnership::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        store.replace_agent(&record).await?;
    }
    Ok(())
}

pub(super) fn system_agent_specs() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    AgentToolPlane,
)> {
    chatos_agent::system_agent_catalog()
        .iter()
        .map(|descriptor| {
            (
                descriptor.key.as_str(),
                descriptor.display_name,
                descriptor.service_name,
                descriptor.description,
                descriptor.include_user_resources,
                descriptor.tool_plane,
            )
        })
        .collect()
}
