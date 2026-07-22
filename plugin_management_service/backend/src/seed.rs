// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

#[cfg(test)]
use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp::{
    system_mcp_catalog, system_mcp_provider_skills, system_mcp_tool_catalog, SystemMcpDescriptor,
    SystemMcpToolCatalog,
};
use chatos_mcp_runtime::BuiltinMcpKind;
use serde_json::Value;

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};

mod agent_prompts;
mod internal_skills;

use agent_prompts::{backfill_agent_prompt_versions, seed_agent_prompts};
use internal_skills::{internal_skill_catalog, seed_internal_skills};

pub use chatos_plugin_management_sdk::{
    CHATOS_TASK_RUNNER_MCP_RESOURCE_ID, LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
    SANDBOX_IMAGES_MCP_RESOURCE_ID,
};
const RETIRED_SYSTEM_AGENT_KEYS: &[&str] = &[
    "chatos_plan_agent",
    "chatos_async_planner",
    "chatos_chat_runtime",
    "project_environment_agent",
    "local_connector_client_agent",
    "memory_engine_context_agent",
];

pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    remove_retired_system_agents(store).await?;
    seed_system_mcps(store, admin_user_id).await?;
    seed_internal_skills(store, admin_user_id).await?;
    seed_agents(store).await?;
    seed_agent_prompts(store, admin_user_id).await?;
    seed_agent_bindings(store, admin_user_id).await?;
    Ok(())
}

pub async fn ensure_agent_prompt_version_history(store: &AppStore) -> Result<(), String> {
    backfill_agent_prompt_versions(store).await
}

async fn remove_retired_system_agents(store: &AppStore) -> Result<(), String> {
    for agent_key in RETIRED_SYSTEM_AGENT_KEYS {
        store.delete_bindings_for_agent(agent_key).await?;
        store.delete_agent(agent_key).await?;
    }
    Ok(())
}

async fn seed_system_mcps(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
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

fn system_mcp_record(
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
fn provider_skills_for_system_mcp(resource_id: &str) -> Option<Value> {
    let descriptor = system_mcp_descriptor_by_resource_id(resource_id)?;
    serde_json::to_value(system_mcp_provider_skills(descriptor.key)).ok()
}

#[cfg(test)]
fn provider_skills_for_builtin_mcp(kind: BuiltinMcpKind) -> Value {
    let descriptor = chatos_mcp::system_mcp_catalog()
        .iter()
        .find(|descriptor| descriptor.embedded_kind == Some(kind))
        .expect("embedded MCP descriptor");
    serde_json::to_value(system_mcp_provider_skills(descriptor.key))
        .unwrap_or_else(|_| Value::Array(Vec::new()))
}

async fn seed_agents(store: &AppStore) -> Result<(), String> {
    for (agent_key, display_name, service_name, description, include_user_resources) in
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
            created_at: now.clone(),
            updated_at: now,
        };
        store.replace_agent(&record).await?;
    }
    Ok(())
}

fn system_agent_specs() -> Vec<(&'static str, &'static str, &'static str, &'static str, bool)> {
    chatos_agent::system_agent_catalog()
        .iter()
        .map(|descriptor| {
            (
                descriptor.key.as_str(),
                descriptor.display_name,
                descriptor.service_name,
                descriptor.description,
                descriptor.include_user_resources,
            )
        })
        .collect()
}

async fn seed_agent_bindings(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    for agent_key in ["chatos_conversation_agent", "chatos_planning_agent"] {
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            agent_key,
            CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
            true,
            10,
        )
        .await?;
    }
    seed_agent_mcp_binding_with_conditions(
        store,
        admin_user_id,
        "project_requirement_execution_planner_agent",
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
        cloud_runtime_binding_conditions(),
    )
    .await?;
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        "project_requirement_execution_planner_agent",
        builtin_resource_id(BuiltinMcpKind::ProjectManagement).as_str(),
        true,
        20,
    )
    .await?;
    for (index, kind) in task_runner_plan_phase_builtin_kinds()
        .into_iter()
        .enumerate()
    {
        let required = matches!(kind, BuiltinMcpKind::TaskManager | BuiltinMcpKind::AskUser);
        let resource_id = builtin_resource_id(kind);
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            "task_runner_plan_phase",
            resource_id.as_str(),
            required,
            10 + index as i64 * 10,
        )
        .await?;
    }
    for (agent_key, kind, required, priority) in [
        (
            "task_runner_run_phase",
            BuiltinMcpKind::TaskManager,
            true,
            10,
        ),
        ("task_runner_run_phase", BuiltinMcpKind::AskUser, true, 20),
    ] {
        let resource_id = builtin_resource_id(kind);
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            agent_key,
            resource_id.as_str(),
            required,
            priority,
        )
        .await?;
    }
    for (kind, priority) in task_runner_run_phase_optional_builtin_kinds() {
        let resource_id = builtin_resource_id(kind);
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            "task_runner_run_phase",
            resource_id.as_str(),
            false,
            priority,
        )
        .await?;
    }
    for agent_key in ["task_runner_plan_phase", "task_runner_run_phase"] {
        seed_agent_mcp_binding_with_conditions(
            store,
            admin_user_id,
            agent_key,
            PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
            true,
            30,
            BindingConditions {
                project_source_type: Some("cloud".to_string()),
                runtime_provider: Some("cloud".to_string()),
                ..BindingConditions::default()
            },
        )
        .await?;
    }
    let catalog = internal_skill_catalog()?;
    for agent_key in ["task_runner_plan_phase", "task_runner_run_phase"] {
        for (index, item) in catalog.skills.iter().enumerate() {
            seed_agent_resource_binding(
                store,
                admin_user_id,
                agent_key,
                RESOURCE_KIND_SKILL,
                item.skill_id.as_str(),
                false,
                300 + index as i64,
            )
            .await?;
        }
    }
    remove_seed_binding(
        store,
        "project_management_agent",
        builtin_resource_id(BuiltinMcpKind::ProjectManagement).as_str(),
    )
    .await?;
    // These bindings mirror fixed tool executors in the current service code.
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        "project_management_agent",
        builtin_resource_id(BuiltinMcpKind::CodeMaintainerRead).as_str(),
        true,
        10,
    )
    .await?;
    for (resource_id, priority) in [
        (PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, 20),
        (SANDBOX_IMAGES_MCP_RESOURCE_ID, 30),
    ] {
        seed_agent_mcp_binding_with_conditions(
            store,
            admin_user_id,
            "project_management_agent",
            resource_id,
            true,
            priority,
            cloud_runtime_binding_conditions(),
        )
        .await?;
    }
    for (resource_id, priority) in [
        (builtin_resource_id(BuiltinMcpKind::CodeMaintainerRead), 10),
        (LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID.to_string(), 20),
    ] {
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            "local_connector_command_approval_agent",
            resource_id.as_str(),
            true,
            priority,
        )
        .await?;
    }
    Ok(())
}

fn cloud_runtime_binding_conditions() -> BindingConditions {
    BindingConditions {
        runtime_provider: Some("cloud".to_string()),
        ..BindingConditions::default()
    }
}

async fn remove_seed_binding(
    store: &AppStore,
    agent_key: &str,
    resource_id: &str,
) -> Result<(), String> {
    let id = format!(
        "{agent_key}__{}__{resource_id}",
        BINDING_SCOPE_SYSTEM_REQUIRED
    );
    store.delete_binding(id.as_str()).await
}

async fn seed_agent_mcp_binding(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
) -> Result<(), String> {
    seed_agent_resource_binding_with_conditions(
        store,
        admin_user_id,
        agent_key,
        RESOURCE_KIND_MCP,
        resource_id,
        required,
        priority,
        BindingConditions::default(),
    )
    .await
}

async fn seed_agent_mcp_binding_with_conditions(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
    conditions: BindingConditions,
) -> Result<(), String> {
    seed_agent_resource_binding_with_conditions(
        store,
        admin_user_id,
        agent_key,
        RESOURCE_KIND_MCP,
        resource_id,
        required,
        priority,
        conditions,
    )
    .await
}

async fn seed_agent_resource_binding(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_kind: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
) -> Result<(), String> {
    seed_agent_resource_binding_with_conditions(
        store,
        admin_user_id,
        agent_key,
        resource_kind,
        resource_id,
        required,
        priority,
        BindingConditions::default(),
    )
    .await
}

async fn seed_agent_resource_binding_with_conditions(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_kind: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
    conditions: BindingConditions,
) -> Result<(), String> {
    let existing = store
        .list_bindings(agent_key, &ListBindingsQuery::default())
        .await?;
    let binding_scope = if required {
        BINDING_SCOPE_SYSTEM_REQUIRED
    } else {
        BINDING_SCOPE_GLOBAL_DEFAULT
    };
    let desired_id = format!("{agent_key}__{binding_scope}__{resource_id}");
    let matching = existing
        .into_iter()
        .filter(|binding| {
            binding.resource_kind == resource_kind
                && binding.resource_id == resource_id
                && binding.owner_user_id.is_none()
                && matches!(
                    binding.binding_scope.as_str(),
                    BINDING_SCOPE_SYSTEM_REQUIRED | BINDING_SCOPE_GLOBAL_DEFAULT
                )
        })
        .collect::<Vec<_>>();
    let now = now_rfc3339();
    let existing_desired = matching
        .iter()
        .find(|binding| binding.id == desired_id)
        .cloned();
    let created_at = existing_desired
        .as_ref()
        .or_else(|| matching.first())
        .map(|binding| binding.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let desired = AgentBindingRecord {
        id: desired_id.clone(),
        agent_key: agent_key.to_string(),
        binding_scope: binding_scope.to_string(),
        owner_user_id: None,
        resource_kind: resource_kind.to_string(),
        resource_id: resource_id.to_string(),
        enabled: true,
        required,
        priority,
        conditions,
        created_by: admin_user_id.to_string(),
        updated_by: admin_user_id.to_string(),
        created_at,
        updated_at: now,
    };
    let already_current = existing_desired.as_ref().is_some_and(|binding| {
        binding.agent_key == desired.agent_key
            && binding.binding_scope == desired.binding_scope
            && binding.owner_user_id == desired.owner_user_id
            && binding.resource_kind == desired.resource_kind
            && binding.resource_id == desired.resource_id
            && binding.enabled == desired.enabled
            && binding.required == desired.required
            && binding.priority == desired.priority
            && binding.conditions == desired.conditions
    });
    for binding in matching {
        if binding.id != desired_id {
            store.delete_binding(binding.id.as_str()).await?;
        }
    }
    if already_current {
        return Ok(());
    }
    store.replace_binding(&desired).await
}

fn task_runner_run_phase_optional_builtin_kinds() -> Vec<(BuiltinMcpKind, i64)> {
    use BuiltinMcpKind::*;
    vec![
        (CodeMaintainerRead, 100),
        (CodeMaintainerWrite, 110),
        (TerminalController, 120),
        (ProjectManagement, 130),
        (Notepad, 140),
        (RemoteConnectionController, 150),
        (WebTools, 160),
        (BrowserTools, 170),
    ]
}

fn task_runner_plan_phase_builtin_kinds() -> Vec<BuiltinMcpKind> {
    use BuiltinMcpKind::*;
    vec![
        CodeMaintainerRead,
        TaskManager,
        ProjectManagement,
        Notepad,
        AskUser,
        WebTools,
        BrowserTools,
        MemorySkillReader,
        MemoryCommandReader,
        MemoryPluginReader,
    ]
}

#[cfg(test)]
fn builtin_kinds() -> Vec<BuiltinMcpKind> {
    system_mcp_catalog()
        .iter()
        .filter_map(|descriptor| descriptor.embedded_kind)
        .collect()
}

pub fn builtin_resource_id(kind: BuiltinMcpKind) -> String {
    system_mcp_catalog()
        .iter()
        .find(|descriptor| descriptor.embedded_kind == Some(kind))
        .map(|descriptor| descriptor.resource_id.to_string())
        .expect("embedded MCP resource id")
}

#[cfg(test)]
mod tests;
