// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for selection in payload.bindings {
        let mcp_id = required_text(Some(selection.mcp_id.as_str()), "mcp_id")?;
        if !seen.insert(mcp_id.clone()) {
            return Err(ApiError::bad_request("duplicate mcp_id in bindings"));
        }
        validate_mcp_binding_mode(selection.mode.as_str())?;
        if selection.mode == MCP_BINDING_MODE_DISABLED {
            continue;
        }
        let mcp = state
            .store
            .get_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("MCP not found: {mcp_id}")))?;
        if !mcp.enabled {
            return Err(ApiError::bad_request(format!("MCP is disabled: {mcp_id}")));
        }
        selected.push((mcp_id, selection.mode));
    }

    state
        .store
        .delete_mcp_bindings_for_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?;

    for (index, (mcp_id, mode)) in selected.into_iter().enumerate() {
        let (enabled, required, binding_scope) = mcp_binding_state(mode.as_str())?;
        let now = now_rfc3339();
        let record = AgentBindingRecord {
            id: format!("{agent_key}__mcp__{mcp_id}"),
            agent_key: agent_key.clone(),
            binding_scope: binding_scope.to_string(),
            owner_user_id: None,
            resource_kind: RESOURCE_KIND_MCP.to_string(),
            resource_id: mcp_id,
            enabled,
            required,
            priority: 100 + index as i64,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
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
    state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;

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
    let mut modes = HashMap::new();
    for binding in bindings
        .into_iter()
        .filter(|binding| binding.enabled && binding.resource_kind == RESOURCE_KIND_MCP)
    {
        let mode = if binding.required {
            MCP_BINDING_MODE_REQUIRED
        } else {
            MCP_BINDING_MODE_OPTIONAL
        };
        modes
            .entry(binding.resource_id)
            .and_modify(|current: &mut &str| {
                if mode == MCP_BINDING_MODE_REQUIRED {
                    *current = mode;
                }
            })
            .or_insert(mode);
    }
    let mut items = mcps
        .into_iter()
        .map(|mcp| AgentMcpBindingView {
            mode: modes
                .get(mcp.id.as_str())
                .copied()
                .unwrap_or(MCP_BINDING_MODE_DISABLED)
                .to_string(),
            bindable: true,
            unavailable_reason: None,
            mcp,
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

#[cfg(test)]
mod tests;
