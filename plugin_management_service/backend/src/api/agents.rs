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
        let mcp = state
            .store
            .get_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("MCP not found: {mcp_id}")))?;
        if mcp.visibility != VISIBILITY_SYSTEM_PRIVATE {
            return Err(ApiError::bad_request(
                "system agent bindings only accept system-private MCPs",
            ));
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
        .list_system_mcps()
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
    let items = mcps
        .into_iter()
        .map(|mcp| AgentMcpBindingView {
            mode: modes
                .get(mcp.id.as_str())
                .copied()
                .unwrap_or(MCP_BINDING_MODE_DISABLED)
                .to_string(),
            mcp,
        })
        .collect();
    Ok(AgentMcpBindingsResponse { agent, items })
}
