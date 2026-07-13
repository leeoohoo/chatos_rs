// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpKind;

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};

mod internal_skills;

use internal_skills::{internal_skill_catalog, seed_internal_skills};

pub const SANDBOX_IMAGES_MCP_RESOURCE_ID: &str = "system_mcp_sandbox_images";
const SANDBOX_IMAGES_MCP_SERVER_NAME: &str = "sandbox_images";
pub const PROJECT_ENVIRONMENT_MCP_RESOURCE_ID: &str = "system_mcp_project_environment";
const PROJECT_ENVIRONMENT_MCP_SERVER_NAME: &str = "project_environment";
pub const LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID: &str = "system_mcp_local_connector_approval";
const LOCAL_CONNECTOR_APPROVAL_MCP_SERVER_NAME: &str = "local_connector_approval";
pub const CHATOS_TASK_RUNNER_MCP_RESOURCE_ID: &str = "system_mcp_chatos_task_runner";
const CHATOS_TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";
const RETIRED_SYSTEM_AGENT_KEYS: &[&str] = &[
    "chatos_plan_agent",
    "chatos_async_planner",
    "chatos_chat_runtime",
    "task_runner_plan_phase",
    "project_environment_agent",
    "local_connector_client_agent",
    "memory_engine_context_agent",
];

pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    remove_retired_system_agents(store).await?;
    seed_builtin_mcps(store, admin_user_id).await?;
    seed_system_routed_mcps(store, admin_user_id).await?;
    seed_internal_skills(store, admin_user_id).await?;
    seed_agents(store).await?;
    seed_agent_bindings(store, admin_user_id).await?;
    Ok(())
}

async fn remove_retired_system_agents(store: &AppStore) -> Result<(), String> {
    for agent_key in RETIRED_SYSTEM_AGENT_KEYS {
        store.delete_bindings_for_agent(agent_key).await?;
        store.delete_agent(agent_key).await?;
    }
    Ok(())
}

async fn seed_builtin_mcps(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    for kind in builtin_kinds() {
        let now = now_rfc3339();
        let id = builtin_resource_id(kind);
        if store.get_mcp(id.as_str()).await?.is_some() {
            continue;
        }
        let display_name = builtin_display_name(kind);
        let record = McpRecord {
            id,
            owner_user_id: admin_user_id.to_string(),
            owner_kind: OWNER_KIND_SYSTEM.to_string(),
            visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
            source_kind: SOURCE_KIND_SYSTEM_SEED.to_string(),
            name: kind.server_name().to_string(),
            display_name,
            description: Some(format!("System builtin MCP: {}", kind.kind_name())),
            enabled: true,
            runtime: McpRuntime {
                kind: RUNTIME_KIND_BUILTIN.to_string(),
                builtin_kind: Some(kind.kind_name().to_string()),
                server_name: Some(kind.server_name().to_string()),
                command: kind.command().map(ToOwned::to_owned),
                ..McpRuntime::default()
            },
            security: ResourceSecurity {
                allow_writes: Some(kind.default_allow_writes()),
                ..ResourceSecurity::default()
            },
            metadata: ResourceMetadata {
                tags: vec!["system".to_string(), "builtin".to_string()],
                ..ResourceMetadata::default()
            },
            created_by: admin_user_id.to_string(),
            updated_by: admin_user_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        store.replace_mcp(&record).await?;
    }
    Ok(())
}

async fn seed_system_routed_mcps(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    seed_system_routed_mcp(
        store,
        admin_user_id,
        SANDBOX_IMAGES_MCP_RESOURCE_ID,
        SANDBOX_IMAGES_MCP_SERVER_NAME,
        "Sandbox Images",
        "System-routed sandbox image MCP for project environment initialization.",
        true,
        &["system", "sandbox", "images"],
        "project_environment",
    )
    .await?;
    seed_system_routed_mcp(
        store,
        admin_user_id,
        PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
        PROJECT_ENVIRONMENT_MCP_SERVER_NAME,
        "Project Environment",
        "Project environment state tools used by the Project Management Agent.",
        true,
        &["system", "project", "environment"],
        "project_environment",
    )
    .await?;
    seed_system_routed_mcp(
        store,
        admin_user_id,
        LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
        LOCAL_CONNECTOR_APPROVAL_MCP_SERVER_NAME,
        "Local Command Approval",
        "Final decision tools used by the Local Connector command approval agent.",
        true,
        &["system", "local_connector", "approval"],
        "local_connector",
    )
    .await?;
    seed_system_routed_mcp(
        store,
        admin_user_id,
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        CHATOS_TASK_RUNNER_MCP_SERVER_NAME,
        "Task Runner Service",
        "Task Runner MCP entry used by Chat OS to create and manage asynchronous tasks.",
        true,
        &["system", "chatos", "task_runner"],
        "chatos",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn seed_system_routed_mcp(
    store: &AppStore,
    admin_user_id: &str,
    resource_id: &str,
    server_name: &str,
    display_name: &str,
    description: &str,
    allow_writes: bool,
    tags: &[&str],
    category: &str,
) -> Result<(), String> {
    if store.get_mcp(resource_id).await?.is_some() {
        return Ok(());
    }
    let now = now_rfc3339();
    let record = McpRecord {
        id: resource_id.to_string(),
        owner_user_id: admin_user_id.to_string(),
        owner_kind: OWNER_KIND_SYSTEM.to_string(),
        visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_SYSTEM_SEED.to_string(),
        name: server_name.to_string(),
        display_name: display_name.to_string(),
        description: Some(description.to_string()),
        enabled: true,
        runtime: McpRuntime {
            kind: RUNTIME_KIND_SYSTEM_ROUTED.to_string(),
            server_name: Some(server_name.to_string()),
            ..McpRuntime::default()
        },
        security: ResourceSecurity {
            allow_writes: Some(allow_writes),
            ..ResourceSecurity::default()
        },
        metadata: ResourceMetadata {
            tags: tags.iter().map(|value| (*value).to_string()).collect(),
            category: Some(category.to_string()),
            ..ResourceMetadata::default()
        },
        created_by: admin_user_id.to_string(),
        updated_by: admin_user_id.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    store.replace_mcp(&record).await
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
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        "chatos_conversation_agent",
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
    )
    .await?;
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        "chatos_planning_agent",
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
    )
    .await?;
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        "project_requirement_execution_planner_agent",
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
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
    let catalog = internal_skill_catalog()?;
    for (index, item) in catalog.skills.iter().enumerate() {
        seed_agent_resource_binding(
            store,
            admin_user_id,
            "task_runner_run_phase",
            RESOURCE_KIND_SKILL,
            item.skill_id.as_str(),
            false,
            300 + index as i64,
        )
        .await?;
    }
    remove_seed_binding(
        store,
        "project_management_agent",
        builtin_resource_id(BuiltinMcpKind::ProjectManagement).as_str(),
    )
    .await?;
    // These bindings mirror fixed tool executors in the current service code.
    for (resource_id, priority) in [
        (builtin_resource_id(BuiltinMcpKind::CodeMaintainerRead), 10),
        (PROJECT_ENVIRONMENT_MCP_RESOURCE_ID.to_string(), 20),
        (SANDBOX_IMAGES_MCP_RESOURCE_ID.to_string(), 30),
    ] {
        seed_agent_mcp_binding(
            store,
            admin_user_id,
            "project_management_agent",
            resource_id.as_str(),
            true,
            priority,
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
    seed_agent_resource_binding(
        store,
        admin_user_id,
        agent_key,
        RESOURCE_KIND_MCP,
        resource_id,
        required,
        priority,
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
        conditions: BindingConditions::default(),
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
            && binding.conditions.task_profile.is_none()
            && binding.conditions.project_source_type.is_none()
            && binding.conditions.runtime_provider.is_none()
            && binding.conditions.schedule_mode.is_none()
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

fn builtin_kinds() -> Vec<BuiltinMcpKind> {
    use BuiltinMcpKind::*;
    vec![
        CodeMaintainerRead,
        CodeMaintainerWrite,
        TerminalController,
        TaskManager,
        ProjectManagement,
        Notepad,
        AgentBuilder,
        AskUser,
        RemoteConnectionController,
        WebTools,
        BrowserTools,
        MemorySkillReader,
        MemoryCommandReader,
        MemoryPluginReader,
    ]
}

pub fn builtin_resource_id(kind: BuiltinMcpKind) -> String {
    kind.config_id()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("system_builtin_{}", snake_case(kind.kind_name())))
}

fn builtin_display_name(kind: BuiltinMcpKind) -> String {
    let mut out = String::new();
    for (idx, ch) in kind.kind_name().chars().enumerate() {
        if idx > 0 && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    format!("{out} (Builtin)")
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx > 0 && ch.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_runner_run_phase_defaults_match_callable_task_runner_providers() {
        let kinds = task_runner_run_phase_optional_builtin_kinds()
            .into_iter()
            .map(|(kind, _)| kind)
            .collect::<Vec<_>>();

        assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(kinds.contains(&BuiltinMcpKind::TerminalController));
        assert!(kinds.contains(&BuiltinMcpKind::ProjectManagement));
        assert!(kinds.contains(&BuiltinMcpKind::Notepad));
        assert!(kinds.contains(&BuiltinMcpKind::RemoteConnectionController));
        assert!(kinds.contains(&BuiltinMcpKind::WebTools));
        assert!(kinds.contains(&BuiltinMcpKind::BrowserTools));
        assert!(!kinds.contains(&BuiltinMcpKind::AgentBuilder));
        assert!(!kinds.contains(&BuiltinMcpKind::MemorySkillReader));
    }

    #[test]
    fn legacy_chatos_plan_key_is_replaced_by_the_explicit_planning_role() {
        assert!(RETIRED_SYSTEM_AGENT_KEYS.contains(&"chatos_plan_agent"));
        assert!(system_agent_specs()
            .iter()
            .any(|(agent_key, _, _, _, _)| *agent_key == "chatos_planning_agent"));
    }

    #[test]
    fn system_agent_registry_contains_all_six_capability_roles() {
        let keys = system_agent_specs()
            .into_iter()
            .map(|(agent_key, _, _, _, _)| agent_key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                "chatos_conversation_agent",
                "chatos_planning_agent",
                "project_requirement_execution_planner_agent",
                "task_runner_run_phase",
                "project_management_agent",
                "local_connector_command_approval_agent",
            ]
        );
    }

    #[test]
    fn chatos_uses_the_task_runner_service_mcp_entry() {
        assert_eq!(CHATOS_TASK_RUNNER_MCP_SERVER_NAME, "task_runner_service");
    }

    #[test]
    fn chatos_conversation_requires_task_runner_service() {
        let spec = (
            "chatos_conversation_agent",
            CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
            true,
        );
        assert_eq!(spec.0, "chatos_conversation_agent");
        assert_eq!(spec.1, "system_mcp_chatos_task_runner");
        assert!(spec.2);
    }
}
