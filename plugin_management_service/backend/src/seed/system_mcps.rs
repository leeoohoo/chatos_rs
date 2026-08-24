// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use super::*;
#[cfg(test)]
use chatos_mcp::system_mcp_descriptor_by_resource_id;

pub(super) async fn remove_retired_system_mcps(store: &AppStore) -> Result<(), String> {
    let active_resource_ids = system_mcp_catalog()
        .iter()
        .map(|descriptor| descriptor.resource_id.to_string())
        .collect::<Vec<_>>();
    store
        .remove_system_seed_mcps_except(active_resource_ids.as_slice())
        .await?;
    store.delete_retired_task_manager_mcp().await?;
    for resource_id in [
        "system_mcp_sandbox_images",
        "system_mcp_project_environment",
        "system_mcp_project_runtime_environment",
    ] {
        store.delete_mcp(resource_id).await?;
    }
    Ok(())
}

pub(super) async fn seed_system_mcps(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    for descriptor in system_mcp_catalog() {
        seed_system_mcp(store, admin_user_id, descriptor).await?;
    }
    Ok(())
}

async fn seed_system_mcp(
    store: &AppStore,
    admin_user_id: &str,
    descriptor: &SystemMcpDescriptor,
) -> Result<(), String> {
    let now = now_rfc3339();
    let mut desired = system_mcp_record(descriptor, admin_user_id, now.as_str())?;
    let Some(existing) = store.get_mcp(descriptor.resource_id).await? else {
        return store.replace_mcp(&desired).await;
    };

    desired.enabled = existing.enabled;
    desired.created_by = existing.created_by.clone();
    desired.created_at = existing.created_at.clone();
    desired.updated_by = existing.updated_by.clone();
    desired.updated_at = existing.updated_at.clone();
    if provider_skills_are_admin_managed(&existing.metadata) {
        if let Some(provider_skills) = existing.metadata.extra.get("provider_skills") {
            desired
                .metadata
                .extra
                .insert("provider_skills".to_string(), provider_skills.clone());
        }
        if let Some(managed_by) = existing.metadata.extra.get("provider_skills_managed_by") {
            desired
                .metadata
                .extra
                .insert("provider_skills_managed_by".to_string(), managed_by.clone());
        }
    }
    if serde_json::to_value(&desired).map_err(|error| error.to_string())?
        == serde_json::to_value(&existing).map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    desired.updated_by = admin_user_id.to_string();
    desired.updated_at = now;
    store.replace_mcp(&desired).await
}

pub(super) fn system_mcp_record(
    descriptor: &SystemMcpDescriptor,
    admin_user_id: &str,
    now: &str,
) -> Result<McpRecord, String> {
    let provider_skills = Value::Array(
        system_mcp_provider_skills(descriptor.key)
            .into_iter()
            .map(|skill| serde_json::to_value(skill).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let mut extra: BTreeMap<String, Value> = [("provider_skills".to_string(), provider_skills)]
        .into_iter()
        .collect();
    if let SystemMcpToolCatalog::Static(tools) = system_mcp_tool_catalog(descriptor.key)? {
        extra.insert("tool_catalog".to_string(), Value::Array(tools));
    }
    Ok(McpRecord {
        id: descriptor.resource_id.to_string(),
        owner_user_id: admin_user_id.to_string(),
        owner_kind: OWNER_KIND_SYSTEM.to_string(),
        visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_SYSTEM_SEED.to_string(),
        name: descriptor.server_name.to_string(),
        display_name: descriptor.display_name.to_string(),
        description: Some(descriptor.description.to_string()),
        enabled: true,
        runtime: McpRuntime {
            kind: RUNTIME_KIND_SYSTEM.to_string(),
            system_key: Some(descriptor.key.as_str().to_string()),
            server_name: Some(descriptor.server_name.to_string()),
            command: descriptor
                .embedded_kind
                .and_then(|kind| kind.command().map(ToOwned::to_owned)),
            ..McpRuntime::default()
        },
        security: ResourceSecurity {
            allow_writes: Some(descriptor.allow_writes),
            ..ResourceSecurity::default()
        },
        metadata: ResourceMetadata {
            tags: descriptor
                .tags
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            category: descriptor.category.map(ToOwned::to_owned),
            extra,
            ..ResourceMetadata::default()
        },
        plugin_component: PluginComponentOwnership::default(),
        created_by: admin_user_id.to_string(),
        updated_by: admin_user_id.to_string(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

fn provider_skills_are_admin_managed(metadata: &ResourceMetadata) -> bool {
    metadata
        .extra
        .get("provider_skills_managed_by")
        .and_then(Value::as_str)
        .is_some_and(|value| value == "admin")
}

#[cfg(test)]
pub(super) fn provider_skills_for_system_mcp(resource_id: &str) -> Option<Value> {
    let descriptor = system_mcp_descriptor_by_resource_id(resource_id)?;
    serde_json::to_value(system_mcp_provider_skills(descriptor.key)).ok()
}

#[cfg(test)]
pub(super) fn provider_skills_for_builtin_mcp(kind: BuiltinMcpKind) -> Value {
    let descriptor = chatos_mcp::system_mcp_catalog()
        .iter()
        .find(|descriptor| descriptor.embedded_kind == Some(kind))
        .expect("embedded MCP descriptor");
    serde_json::to_value(system_mcp_provider_skills(descriptor.key))
        .unwrap_or_else(|_| Value::Array(Vec::new()))
}

#[cfg(test)]
pub(super) fn builtin_kinds() -> Vec<BuiltinMcpKind> {
    system_mcp_catalog()
        .iter()
        .filter_map(|descriptor| descriptor.embedded_kind)
        .collect()
}

pub(super) fn builtin_resource_id(kind: BuiltinMcpKind) -> String {
    system_mcp_catalog()
        .iter()
        .find(|descriptor| descriptor.embedded_kind == Some(kind))
        .map(|descriptor| descriptor.resource_id.to_string())
        .expect("embedded MCP resource id")
}
