// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use chatos_agent::CHATOS_PLAN_TASK_PROFILE;
#[cfg(test)]
use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp::{
    system_mcp_catalog, system_mcp_provider_skills, system_mcp_tool_catalog, SystemMcpDescriptor,
    SystemMcpToolCatalog,
};
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};

mod agent_prompts;
mod internal_skills;
mod plugins;

use agent_prompts::{backfill_agent_prompt_versions, seed_agent_prompts};
use internal_skills::{internal_skill_catalog, seed_internal_skills};
use plugins::{seed_bundled_plugins, BUNDLED_PONYTAIL_AGENT_KEYS, BUNDLED_PONYTAIL_PLUGIN_ID};

pub use chatos_plugin_management_sdk::{
    CHATOS_TASK_RUNNER_MCP_RESOURCE_ID, LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
    SANDBOX_IMAGES_MCP_RESOURCE_ID, TASK_PROCESS_LOG_MCP_RESOURCE_ID,
};
const CHATOS_TASK_RUNNER_DEFAULT_TOOL_ALLOWLIST: &[&str] = &[
    "list_tasks",
    "get_task",
    "get_task_stats",
    "create_task",
    "wait_for_task_completion",
];
const CHATOS_TASK_RUNNER_PLAN_TOOL_ALLOWLIST: &[&str] = &[
    "list_tasks",
    "get_task",
    "get_task_stats",
    "create_task",
    "create_tasks_with_prerequisites",
    "wait_for_task_completion",
];
const PROJECT_MANAGEMENT_AGENT_SANDBOX_TOOL_ALLOWLIST: &[&str] =
    &["get_image_catalog", "search_images"];
const CHATOS_CONVERSATION_AGENT_KEY: &str = SystemAgentKey::ChatosConversationAgent.as_str();
const PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY: &str =
    SystemAgentKey::ProjectRequirementExecutionPlannerAgent.as_str();
const TASK_RUNNER_PLAN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerPlanPhase.as_str();
const TASK_RUNNER_RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const PROJECT_MANAGEMENT_AGENT_KEY: &str = SystemAgentKey::ProjectManagementAgent.as_str();
const LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY: &str =
    SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str();
const RETIRED_SYSTEM_AGENT_KEYS: &[&str] = &[
    "chatos_plan_agent",
    "chatos_planning_agent",
    "chatos_async_planner",
    "chatos_chat_runtime",
    "project_environment_agent",
    "local_connector_client_agent",
    SystemAgentKey::TaskRunnerLocalPlanPhase.as_str(),
    SystemAgentKey::TaskRunnerLocalRunPhase.as_str(),
    "memory_engine_context_agent",
];
const CHATOS_NOTEPAD_AGENT_KEYS: &[&str] = &[
    CHATOS_CONVERSATION_AGENT_KEY,
    PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY,
];
const CHATOS_TASK_RUNNER_AGENT_KEYS: &[&str] = &[CHATOS_CONVERSATION_AGENT_KEY];
const TASK_RUNNER_PHASE_AGENT_KEYS: &[&str] =
    &[TASK_RUNNER_PLAN_AGENT_KEY, TASK_RUNNER_RUN_AGENT_KEY];
const PROJECT_MANAGEMENT_AGENT_REQUIRED_MCPS: &[(&str, i64)] = &[
    (PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, 20),
    (SANDBOX_IMAGES_MCP_RESOURCE_ID, 30),
];

pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    remove_retired_system_agents(store).await?;
    remove_retired_system_mcps(store).await?;
    seed_system_mcps(store, admin_user_id).await?;
    seed_internal_skills(store, admin_user_id).await?;
    seed_bundled_plugins(store, admin_user_id).await?;
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

async fn remove_retired_system_mcps(store: &AppStore) -> Result<(), String> {
    store.delete_retired_task_manager_mcp().await
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

fn system_agent_specs() -> Vec<(
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

async fn seed_agent_bindings(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    for descriptor in chatos_agent::system_agent_catalog()
        .iter()
        .filter(|descriptor| !descriptor.tool_plane.uses_managed_gateway())
    {
        store
            .delete_bindings_for_agent(descriptor.key.as_str())
            .await?;
    }
    for agent_key in CHATOS_TASK_RUNNER_AGENT_KEYS {
        seed_agent_mcp_binding_with_tool_policy(
            store,
            admin_user_id,
            agent_key,
            CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
            true,
            10,
            BindingConditions::default(),
            CHATOS_TASK_RUNNER_DEFAULT_TOOL_ALLOWLIST,
            &[],
        )
        .await?;
    }
    seed_agent_mcp_binding_with_tool_policy(
        store,
        admin_user_id,
        CHATOS_CONVERSATION_AGENT_KEY,
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        11,
        BindingConditions {
            task_profile: Some(CHATOS_PLAN_TASK_PROFILE.to_string()),
            ..BindingConditions::default()
        },
        CHATOS_TASK_RUNNER_PLAN_TOOL_ALLOWLIST,
        &[],
    )
    .await?;
    seed_agent_mcp_binding_with_conditions(
        store,
        admin_user_id,
        PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY,
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
        cloud_runtime_binding_conditions(),
    )
    .await?;
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY,
        builtin_resource_id(BuiltinMcpKind::ProjectManagement).as_str(),
        true,
        20,
    )
    .await?;
    for agent_key in CHATOS_NOTEPAD_AGENT_KEYS {
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            agent_key,
            builtin_resource_id(BuiltinMcpKind::Notepad).as_str(),
            false,
            30,
        )
        .await?;
    }
    for (agent_key, kinds) in [(
        TASK_RUNNER_PLAN_AGENT_KEY,
        task_runner_cloud_plan_phase_builtin_kinds(),
    )] {
        for (index, kind) in kinds.into_iter().enumerate() {
            let required = task_runner_cloud_plan_phase_required(kind);
            let resource_id = builtin_resource_id(kind);
            seed_agent_mcp_binding(
                store,
                admin_user_id,
                agent_key,
                resource_id.as_str(),
                required,
                10 + index as i64 * 10,
            )
            .await?;
        }
    }
    for (agent_key, kind, required, priority) in
        [(TASK_RUNNER_RUN_AGENT_KEY, BuiltinMcpKind::AskUser, true, 20)]
    {
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
    for agent_key in TASK_RUNNER_PHASE_AGENT_KEYS {
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            agent_key,
            TASK_PROCESS_LOG_MCP_RESOURCE_ID,
            true,
            15,
        )
        .await?;
    }
    remove_seed_binding_for_all_system_scopes(
        store,
        TASK_RUNNER_RUN_AGENT_KEY,
        builtin_resource_id(BuiltinMcpKind::RemoteConnectionController).as_str(),
    )
    .await?;
    for (kind, priority) in task_runner_cloud_run_phase_optional_builtin_kinds() {
        let resource_id = builtin_resource_id(kind);
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            TASK_RUNNER_RUN_AGENT_KEY,
            resource_id.as_str(),
            false,
            priority,
        )
        .await?;
    }
    for agent_key in BUNDLED_PONYTAIL_AGENT_KEYS {
        seed_agent_resource_binding(
            store,
            admin_user_id,
            agent_key,
            RESOURCE_KIND_PLUGIN,
            BUNDLED_PONYTAIL_PLUGIN_ID,
            false,
            800,
        )
        .await?;
    }
    for agent_key in TASK_RUNNER_PHASE_AGENT_KEYS {
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
    for agent_key in TASK_RUNNER_PHASE_AGENT_KEYS {
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
    // These bindings mirror fixed tool executors in the current service code.
    for (resource_id, priority) in [
        (builtin_resource_id(BuiltinMcpKind::CodeMaintainerRead), 10),
        (builtin_resource_id(BuiltinMcpKind::ProjectManagement), 15),
    ] {
        let tool_allowlist = if resource_id
            == builtin_resource_id(BuiltinMcpKind::ProjectManagement)
        {
            chatos_mcp::project_management_contract::tools::PROJECT_MANAGEMENT_READ_ONLY_TOOL_NAMES
        } else {
            &[]
        };
        seed_agent_mcp_binding_with_tool_policy(
            store,
            admin_user_id,
            PROJECT_MANAGEMENT_AGENT_KEY,
            resource_id.as_str(),
            true,
            priority,
            BindingConditions::default(),
            tool_allowlist,
            &[],
        )
        .await?;
    }
    // Capability selection only decides which tools this Agent owns. MCP Management
    // resolves the actual Project Service, Local Connector, or cloud Sandbox provider
    // from the authoritative Project Execution Context for each Runtime Session.
    for (resource_id, priority) in PROJECT_MANAGEMENT_AGENT_REQUIRED_MCPS {
        let tool_allowlist = if *resource_id == SANDBOX_IMAGES_MCP_RESOURCE_ID {
            PROJECT_MANAGEMENT_AGENT_SANDBOX_TOOL_ALLOWLIST
        } else {
            &[]
        };
        seed_agent_mcp_binding_with_tool_policy(
            store,
            admin_user_id,
            PROJECT_MANAGEMENT_AGENT_KEY,
            resource_id,
            true,
            *priority,
            BindingConditions::default(),
            tool_allowlist,
            &[],
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
            LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY,
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

async fn remove_seed_binding_for_all_system_scopes(
    store: &AppStore,
    agent_key: &str,
    resource_id: &str,
) -> Result<(), String> {
    for scope in [BINDING_SCOPE_SYSTEM_REQUIRED, BINDING_SCOPE_GLOBAL_DEFAULT] {
        let id = format!("{agent_key}__{scope}__{resource_id}");
        store.delete_binding(id.as_str()).await?;
    }
    Ok(())
}

async fn seed_agent_mcp_binding(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
) -> Result<(), String> {
    seed_agent_mcp_binding_with_tool_policy(
        store,
        admin_user_id,
        agent_key,
        resource_id,
        required,
        priority,
        BindingConditions::default(),
        &[],
        &[],
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
    seed_agent_mcp_binding_with_tool_policy(
        store,
        admin_user_id,
        agent_key,
        resource_id,
        required,
        priority,
        conditions,
        &[],
        &[],
    )
    .await
}

async fn seed_agent_mcp_binding_with_tool_policy(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
    conditions: BindingConditions,
    tool_allowlist: &[&str],
    tool_blocklist: &[&str],
) -> Result<(), String> {
    seed_agent_resource_binding_with_policy(
        store,
        admin_user_id,
        agent_key,
        RESOURCE_KIND_MCP,
        resource_id,
        required,
        priority,
        conditions,
        Vec::new(),
        tool_allowlist
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        tool_blocklist
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
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
    seed_agent_resource_binding_with_policy(
        store,
        admin_user_id,
        agent_key,
        resource_kind,
        resource_id,
        required,
        priority,
        BindingConditions::default(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .await
}

async fn seed_agent_resource_binding_with_policy(
    store: &AppStore,
    admin_user_id: &str,
    agent_key: &str,
    resource_kind: &str,
    resource_id: &str,
    required: bool,
    priority: i64,
    conditions: BindingConditions,
    component_allowlist: Vec<String>,
    tool_allowlist: Vec<String>,
    tool_blocklist: Vec<String>,
) -> Result<(), String> {
    let existing = store
        .list_bindings(agent_key, &ListBindingsQuery::default())
        .await?;
    let binding_scope = if required {
        BINDING_SCOPE_SYSTEM_REQUIRED
    } else {
        BINDING_SCOPE_GLOBAL_DEFAULT
    };
    let desired_id = seed_binding_id(
        agent_key,
        binding_scope,
        resource_kind,
        resource_id,
        &conditions,
    );
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
        component_allowlist,
        tool_allowlist,
        tool_blocklist,
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
            && binding.component_allowlist == desired.component_allowlist
            && binding.tool_allowlist == desired.tool_allowlist
            && binding.tool_blocklist == desired.tool_blocklist
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

fn seed_binding_id(
    agent_key: &str,
    binding_scope: &str,
    resource_kind: &str,
    resource_id: &str,
    conditions: &BindingConditions,
) -> String {
    let condition_key = [
        ("task_profile", conditions.task_profile.as_deref()),
        (
            "project_source_type",
            conditions.project_source_type.as_deref(),
        ),
        ("runtime_provider", conditions.runtime_provider.as_deref()),
        ("schedule_mode", conditions.schedule_mode.as_deref()),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|value| format!("{label}={value}")))
    .collect::<Vec<_>>()
    .join("|");
    if condition_key.is_empty() {
        return format!("{agent_key}__{binding_scope}__{resource_id}");
    }
    let mut hasher = DefaultHasher::new();
    resource_kind.hash(&mut hasher);
    condition_key.hash(&mut hasher);
    format!(
        "{agent_key}__{binding_scope}__{resource_id}__{:016x}",
        hasher.finish()
    )
}

fn task_runner_cloud_run_phase_optional_builtin_kinds() -> Vec<(BuiltinMcpKind, i64)> {
    use BuiltinMcpKind::*;
    vec![
        (CodeMaintainerRead, 100),
        (CodeMaintainerWrite, 110),
        (TerminalController, 120),
        (ProjectManagement, 130),
        (Notepad, 140),
        (WebTools, 160),
        (BrowserTools, 170),
    ]
}

fn task_runner_cloud_plan_phase_builtin_kinds() -> Vec<BuiltinMcpKind> {
    use BuiltinMcpKind::*;
    vec![
        CodeMaintainerRead,
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

fn task_runner_cloud_plan_phase_required(kind: BuiltinMcpKind) -> bool {
    matches!(
        kind,
        BuiltinMcpKind::CodeMaintainerRead
            | BuiltinMcpKind::ProjectManagement
            | BuiltinMcpKind::AskUser
    )
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
