// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::hash::{DefaultHasher, Hash, Hasher};

use super::*;

pub(super) async fn list_system_agents(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<SystemAgentRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    state
        .store
        .list_agents()
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_system_agent(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<SystemAgentPayload>,
) -> Result<Json<SystemAgentRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let agent_key = required_text(payload.agent_key.as_deref(), "agent_key")?;
    if state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        return Err(ApiError::conflict("System agent already exists"));
    }
    let display_name = required_text(payload.display_name.as_deref(), "display_name")?;
    let service_name = required_text(payload.service_name.as_deref(), "service_name")?;
    let now = now_rfc3339();
    let record = SystemAgentRecord {
        id: format!("system_agent_{agent_key}"),
        agent_key,
        display_name,
        service_name,
        scope: "system_internal".to_string(),
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        managed_by: payload.managed_by.unwrap_or_else(|| "admin".to_string()),
        include_user_resources: false,
        tool_plane: AgentToolPlane::Managed,
        plugin_component: PluginComponentOwnership::default(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_agent(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn update_system_agent(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
    Json(payload): Json<SystemAgentPayload>,
) -> Result<Json<SystemAgentRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let mut record = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    validate_release_managed_agent_update(&record.plugin_component, &payload)?;
    if let Some(display_name) = payload.display_name {
        record.display_name = required_text(Some(&display_name), "display_name")?;
    }
    if let Some(service_name) = payload.service_name {
        record.service_name = required_text(Some(&service_name), "service_name")?;
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(managed_by) = payload.managed_by {
        record.managed_by = managed_by;
    }
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_agent(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn get_agent_mcp_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<AgentMcpBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    build_agent_mcp_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

pub(super) async fn update_agent_mcp_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
    Json(payload): Json<UpdateAgentMcpBindingsRequest>,
) -> Result<Json<AgentMcpBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    let agent = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    ensure_managed_tool_plane(&agent)?;
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for mut selection in payload.bindings {
        let mcp_id = required_text(Some(selection.mcp_id.as_str()), "mcp_id")?;
        let conditions = normalized_binding_conditions(selection.conditions);
        let duplicate_key = (mcp_id.clone(), binding_conditions_key(&conditions));
        if !seen.insert(duplicate_key) {
            return Err(ApiError::bad_request(
                "duplicate MCP binding conditions in bindings",
            ));
        }
        validate_mcp_binding_mode(selection.mode.as_str())?;
        selection.tool_allowlist =
            normalize_string_list(std::mem::take(&mut selection.tool_allowlist));
        selection.tool_blocklist =
            normalize_string_list(std::mem::take(&mut selection.tool_blocklist));
        let mcp = state
            .store
            .get_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("MCP not found: {mcp_id}")))?;
        if !mcp.enabled {
            return Err(ApiError::bad_request(format!("MCP is disabled: {mcp_id}")));
        }
        selected.push((
            mcp_id,
            selection.mode,
            conditions,
            selection.tool_allowlist,
            selection.tool_blocklist,
        ));
    }

    state
        .store
        .delete_mcp_bindings_for_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?;

    for (index, (mcp_id, mode, conditions, tool_allowlist, tool_blocklist)) in
        selected.into_iter().enumerate()
    {
        let (enabled, required) = match mode.as_str() {
            MCP_BINDING_MODE_DISABLED => (false, false),
            MCP_BINDING_MODE_OPTIONAL => (true, false),
            MCP_BINDING_MODE_REQUIRED => (true, true),
            _ => unreachable!("validated MCP binding mode"),
        };
        let now = now_rfc3339();
        let record = AgentBindingRecord {
            id: agent_resource_binding_id(
                agent_key.as_str(),
                BINDING_SCOPE_ADMIN_OVERRIDE,
                RESOURCE_KIND_MCP,
                mcp_id.as_str(),
                &conditions,
            ),
            agent_key: agent_key.clone(),
            binding_scope: BINDING_SCOPE_ADMIN_OVERRIDE.to_string(),
            owner_user_id: None,
            resource_kind: RESOURCE_KIND_MCP.to_string(),
            resource_id: mcp_id,
            enabled,
            required,
            priority: 100 + index as i64,
            conditions,
            component_allowlist: Vec::new(),
            tool_allowlist,
            tool_blocklist,
            created_by: user.user_id.clone(),
            updated_by: user.user_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        state
            .store
            .replace_binding(&record)
            .await
            .map_err(ApiError::internal)?;
    }

    build_agent_mcp_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

pub(super) async fn get_agent_plugin_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<AgentPluginBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    build_agent_plugin_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

pub(super) async fn update_agent_plugin_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
    Json(payload): Json<UpdateAgentPluginBindingsRequest>,
) -> Result<Json<AgentPluginBindingsResponse>, ApiError> {
    ensure_super_admin(&user)?;
    let agent = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    ensure_managed_tool_plane(&agent)?;

    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for mut selection in payload.bindings {
        selection.plugin_id = required_text(Some(selection.plugin_id.as_str()), "plugin_id")?;
        if !seen.insert(selection.plugin_id.clone()) {
            return Err(ApiError::bad_request(
                "duplicate plugin_id in Plugin bindings",
            ));
        }
        validate_mcp_binding_mode(selection.mode.as_str())?;
        selection.component_allowlist =
            normalize_string_list(std::mem::take(&mut selection.component_allowlist));
        let plugin = state
            .store
            .get_plugin_catalog_entry(selection.plugin_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| {
                ApiError::not_found(format!("Plugin not found: {}", selection.plugin_id))
            })?;
        let release = if plugin.latest_release_id.is_empty() {
            None
        } else {
            state
                .store
                .get_plugin_release(plugin.latest_release_id.as_str())
                .await
                .map_err(ApiError::internal)?
        };
        validate_component_selection(&selection.component_allowlist, release.as_ref())?;
        selected.push(selection);
    }

    state
        .store
        .delete_plugin_bindings_for_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?;

    for (index, selection) in selected.into_iter().enumerate() {
        let (enabled, required, binding_scope) = mcp_binding_state(selection.mode.as_str())?;
        let resource_kind = if selection.component_allowlist.is_empty() {
            RESOURCE_KIND_PLUGIN
        } else {
            RESOURCE_KIND_PLUGIN_COMPONENT
        };
        let now = now_rfc3339();
        let record = AgentBindingRecord {
            id: format!("{}__plugin__{}", agent_key, selection.plugin_id),
            agent_key: agent_key.clone(),
            binding_scope: binding_scope.to_string(),
            owner_user_id: None,
            resource_kind: resource_kind.to_string(),
            resource_id: selection.plugin_id,
            enabled,
            required,
            priority: 500 + index as i64,
            conditions: selection.conditions,
            component_allowlist: selection.component_allowlist,
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
            created_by: user.user_id.clone(),
            updated_by: user.user_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        state
            .store
            .replace_binding(&record)
            .await
            .map_err(ApiError::internal)?;
    }

    build_agent_plugin_bindings_response(&state, agent_key.as_str())
        .await
        .map(Json)
}

async fn build_agent_plugin_bindings_response(
    state: &AppState,
    agent_key: &str,
) -> Result<AgentPluginBindingsResponse, ApiError> {
    let agent = state
        .store
        .get_agent(agent_key)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    ensure_managed_tool_plane(&agent)?;
    let plugins = state
        .store
        .list_plugin_catalog(
            &PluginCatalogQuery {
                limit: Some(500),
                ..PluginCatalogQuery::default()
            },
            None,
        )
        .await
        .map_err(ApiError::internal)?
        .items;
    let bindings = state
        .store
        .list_bindings(agent_key, &ListBindingsQuery::default())
        .await
        .map_err(ApiError::internal)?;
    let modes = bindings
        .into_iter()
        .filter(|binding| {
            matches!(
                binding.resource_kind.as_str(),
                RESOURCE_KIND_PLUGIN | RESOURCE_KIND_PLUGIN_COMPONENT
            )
        })
        .map(|binding| {
            let mode = if !binding.enabled {
                MCP_BINDING_MODE_DISABLED
            } else if binding.required {
                MCP_BINDING_MODE_REQUIRED
            } else {
                MCP_BINDING_MODE_OPTIONAL
            };
            (
                binding.resource_id,
                (
                    mode.to_string(),
                    binding.component_allowlist,
                    binding.conditions,
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let items = plugins
        .into_iter()
        .map(|plugin| {
            let (mode, component_allowlist, conditions) =
                modes.get(plugin.id.as_str()).cloned().unwrap_or_else(|| {
                    (
                        MCP_BINDING_MODE_DISABLED.to_string(),
                        Vec::new(),
                        BindingConditions::default(),
                    )
                });
            AgentPluginBindingView {
                plugin,
                mode,
                component_allowlist,
                conditions,
            }
        })
        .collect();
    Ok(AgentPluginBindingsResponse { agent, items })
}

pub(super) async fn build_agent_mcp_bindings_response(
    state: &AppState,
    agent_key: &str,
) -> Result<AgentMcpBindingsResponse, ApiError> {
    let agent = state
        .store
        .get_agent(agent_key)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    ensure_managed_tool_plane(&agent)?;
    let mcps = state
        .store
        .list_all_mcps_for_admin_catalog()
        .await
        .map_err(ApiError::internal)?;
    let bindings = state
        .store
        .list_bindings(agent_key, &ListBindingsQuery::default())
        .await
        .map_err(ApiError::internal)?;
    let mut bindings_by_resource = bindings
        .into_iter()
        .filter(|binding| binding.resource_kind == RESOURCE_KIND_MCP)
        .fold(
            HashMap::<String, Vec<AgentBindingRecord>>::new(),
            |mut acc, binding| {
                acc.entry(binding.resource_id.clone())
                    .or_default()
                    .push(binding);
                acc
            },
        );
    let mut items = mcps
        .into_iter()
        .flat_map(|mcp| {
            let mut binding_items = bindings_by_resource
                .remove(mcp.id.as_str())
                .unwrap_or_default()
                .into_iter()
                .map(|binding| AgentMcpBindingView {
                    mcp: mcp.clone(),
                    binding_id: Some(binding.id.clone()),
                    mode: if binding.required {
                        MCP_BINDING_MODE_REQUIRED
                    } else if binding.enabled {
                        MCP_BINDING_MODE_OPTIONAL
                    } else {
                        MCP_BINDING_MODE_DISABLED
                    }
                    .to_string(),
                    bindable: true,
                    unavailable_reason: None,
                    conditions: binding.conditions,
                    tool_allowlist: binding.tool_allowlist,
                    tool_blocklist: binding.tool_blocklist,
                })
                .collect::<Vec<_>>();
            if binding_items.is_empty() {
                binding_items.push(AgentMcpBindingView {
                    mcp,
                    binding_id: None,
                    mode: MCP_BINDING_MODE_DISABLED.to_string(),
                    bindable: true,
                    unavailable_reason: None,
                    conditions: BindingConditions::default(),
                    tool_allowlist: Vec::new(),
                    tool_blocklist: Vec::new(),
                });
            }
            binding_items
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        mcp_binding_sort_rank(left)
            .cmp(&mcp_binding_sort_rank(right))
            .then_with(|| {
                left.mcp
                    .display_name
                    .to_ascii_lowercase()
                    .cmp(&right.mcp.display_name.to_ascii_lowercase())
            })
            .then_with(|| left.mcp.id.cmp(&right.mcp.id))
    });
    Ok(AgentMcpBindingsResponse { agent, items })
}

fn normalized_binding_conditions(conditions: BindingConditions) -> BindingConditions {
    BindingConditions {
        task_profile: normalized(conditions.task_profile.as_deref()),
        runtime_provider: normalized(conditions.runtime_provider.as_deref()),
        schedule_mode: normalized(conditions.schedule_mode.as_deref()),
    }
}

fn binding_conditions_key(conditions: &BindingConditions) -> String {
    [
        ("task_profile", conditions.task_profile.as_deref()),
        ("runtime_provider", conditions.runtime_provider.as_deref()),
        ("schedule_mode", conditions.schedule_mode.as_deref()),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|value| format!("{label}={value}")))
    .collect::<Vec<_>>()
    .join("|")
}

pub(super) fn agent_resource_binding_id(
    agent_key: &str,
    binding_scope: &str,
    resource_kind: &str,
    resource_id: &str,
    conditions: &BindingConditions,
) -> String {
    let condition_key = binding_conditions_key(conditions);
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

fn mcp_binding_sort_rank(item: &AgentMcpBindingView) -> (u8, u8) {
    let bound_rank = if item.mode == MCP_BINDING_MODE_DISABLED {
        1
    } else {
        0
    };
    let visibility_rank = match item.mcp.visibility.as_str() {
        VISIBILITY_SYSTEM_PRIVATE => 0,
        VISIBILITY_PUBLIC => 1,
        _ => 2,
    };
    (bound_rank, visibility_rank)
}

pub(super) fn ensure_managed_tool_plane(agent: &SystemAgentRecord) -> Result<(), ApiError> {
    if agent.tool_plane.uses_managed_gateway() {
        return Ok(());
    }
    Err(ApiError::conflict(format!(
        "System agent {} does not use the managed MCP Tool Plane",
        agent.agent_key,
    )))
}

#[cfg(test)]
mod tests;
