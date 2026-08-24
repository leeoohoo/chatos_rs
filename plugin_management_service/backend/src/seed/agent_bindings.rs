// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::hash::{DefaultHasher, Hash, Hasher};

use super::*;

pub(super) async fn seed_agent_bindings(
    store: &AppStore,
    admin_user_id: &str,
) -> Result<(), String> {
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
    seed_agent_mcp_binding(
        store,
        admin_user_id,
        PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY,
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
        10,
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
    for agent_key in [TASK_RUNNER_PLAN_AGENT_KEY] {
        let kinds = task_runner_plan_phase_builtin_kinds();
        for (index, kind) in kinds.into_iter().enumerate() {
            let required = task_runner_plan_phase_required(kind);
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
    for agent_key in [TASK_RUNNER_RUN_AGENT_KEY] {
        for (kind, priority) in task_runner_run_phase_optional_builtin_kinds() {
            let resource_id = builtin_resource_id(kind);
            seed_agent_mcp_binding(
                store,
                admin_user_id,
                agent_key,
                resource_id.as_str(),
                false,
                priority,
            )
            .await?;
        }
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
    if existing
        .iter()
        .any(|binding| binding_matches_admin_override(binding, resource_kind, resource_id))
    {
        return Ok(());
    }
    let matching = existing
        .iter()
        .filter(|binding| {
            binding_matches_seed_variant(
                binding,
                binding_scope,
                resource_kind,
                resource_id,
                &conditions,
            )
        })
        .cloned()
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

pub(super) fn binding_matches_seed_variant(
    binding: &AgentBindingRecord,
    binding_scope: &str,
    resource_kind: &str,
    resource_id: &str,
    conditions: &BindingConditions,
) -> bool {
    binding.binding_scope == binding_scope
        && binding.resource_kind == resource_kind
        && binding.resource_id == resource_id
        && binding.owner_user_id.is_none()
        && binding.conditions == *conditions
}

fn binding_matches_admin_override(
    binding: &AgentBindingRecord,
    resource_kind: &str,
    resource_id: &str,
) -> bool {
    binding.binding_scope == BINDING_SCOPE_ADMIN_OVERRIDE
        && binding.resource_kind == resource_kind
        && binding.resource_id == resource_id
        && binding.owner_user_id.is_none()
}

pub(super) fn task_runner_run_phase_optional_builtin_kinds() -> Vec<(BuiltinMcpKind, i64)> {
    use BuiltinMcpKind::*;
    vec![
        (CodeMaintainerRead, 100),
        (CodeMaintainerWrite, 110),
        (TerminalController, 120),
        (ProjectManagement, 130),
        (Notepad, 140),
    ]
}

pub(super) fn task_runner_plan_phase_builtin_kinds() -> Vec<BuiltinMcpKind> {
    use BuiltinMcpKind::*;
    vec![
        CodeMaintainerRead,
        ProjectManagement,
        Notepad,
        AskUser,
        MemorySkillReader,
        MemoryCommandReader,
        MemoryPluginReader,
    ]
}

pub(super) fn task_runner_plan_phase_required(kind: BuiltinMcpKind) -> bool {
    matches!(
        kind,
        BuiltinMcpKind::CodeMaintainerRead
            | BuiltinMcpKind::ProjectManagement
            | BuiltinMcpKind::AskUser
    )
}
