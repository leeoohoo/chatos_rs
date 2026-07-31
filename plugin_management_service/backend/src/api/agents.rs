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
    let agent = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    if !agent_supports_mcp(&agent) && !payload.bindings.is_empty() {
        return Err(ApiError::bad_request(
            "This system agent does not support MCP bindings",
        ));
    }

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

        if let Some(reason) = agent_mcp_unavailable_reason(&agent, &mcp) {
            return Err(ApiError::bad_request(format!(
                "MCP {} cannot be bound to system agent {}: {}",
                mcp.id,
                agent.agent_key,
                agent_mcp_unavailable_message(reason)
            )));
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
    if !agent_supports_mcp(&agent) {
        return Ok(AgentMcpBindingsResponse {
            agent,
            items: Vec::new(),
        });
    }
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
        .map(|mcp| {
            let unavailable_reason = agent_mcp_unavailable_reason(&agent, &mcp);
            AgentMcpBindingView {
                mode: modes
                    .get(mcp.id.as_str())
                    .copied()
                    .unwrap_or(MCP_BINDING_MODE_DISABLED)
                    .to_string(),
                bindable: unavailable_reason.is_none(),
                unavailable_reason: unavailable_reason.map(str::to_string),
                mcp,
            }
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

fn agent_supports_mcp(agent: &SystemAgentRecord) -> bool {
    agent.service_name != "memory-engine"
}

const AGENT_MCP_UNAVAILABLE_DISABLED: &str = "agentMcpUnavailable.disabled";
const AGENT_MCP_UNAVAILABLE_PRIVATE_SCOPE: &str = "agentMcpUnavailable.privateScope";
const AGENT_MCP_UNAVAILABLE_LOCAL_CONNECTOR_SCOPE: &str = "agentMcpUnavailable.localConnectorScope";
const AGENT_MCP_UNAVAILABLE_HOST_UNSUPPORTED: &str = "agentMcpUnavailable.hostUnsupported";
const AGENT_MCP_UNAVAILABLE_PHASE_FORBIDS_BUILTIN: &str = "agentMcpUnavailable.phaseForbidsBuiltin";
const AGENT_MCP_UNAVAILABLE_CLOUD_RUNTIME: &str = "agentMcpUnavailable.cloudRuntime";
const AGENT_MCP_UNAVAILABLE_PLANNING_WRITES: &str = "agentMcpUnavailable.planningWrites";
const AGENT_MCP_UNAVAILABLE_LOCAL_EXTERNAL_GLOBAL: &str = "agentMcpUnavailable.localExternalGlobal";

fn agent_mcp_unavailable_reason(
    agent: &SystemAgentRecord,
    mcp: &McpRecord,
) -> Option<&'static str> {
    if !mcp.enabled {
        return Some(AGENT_MCP_UNAVAILABLE_DISABLED);
    }
    if mcp_is_local_connector_scoped(mcp) {
        return Some(AGENT_MCP_UNAVAILABLE_LOCAL_CONNECTOR_SCOPE);
    }
    if mcp.visibility == VISIBILITY_PRIVATE {
        return Some(AGENT_MCP_UNAVAILABLE_PRIVATE_SCOPE);
    }
    if let Some(descriptor) = chatos_mcp::system_mcp_descriptor_for_record(mcp) {
        let Some(host) = agent_mcp_host(agent) else {
            if descriptor.key == chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog {
                return Some(AGENT_MCP_UNAVAILABLE_HOST_UNSUPPORTED);
            }
            return None;
        };
        if !descriptor.supports_host(host) {
            return Some(AGENT_MCP_UNAVAILABLE_HOST_UNSUPPORTED);
        }
        if descriptor
            .embedded_kind
            .is_some_and(|kind| !task_runner_agent_allows_builtin(agent.agent_key.as_str(), kind))
        {
            return Some(AGENT_MCP_UNAVAILABLE_PHASE_FORBIDS_BUILTIN);
        }
        return None;
    }
    let host = agent_mcp_host(agent)?;

    if host == chatos_mcp::SystemMcpHost::LocalConnector {
        return Some(AGENT_MCP_UNAVAILABLE_LOCAL_EXTERNAL_GLOBAL);
    }
    if host == chatos_mcp::SystemMcpHost::TaskRunner {
        return cloud_task_runner_external_mcp_unavailable_reason(agent, mcp);
    }
    None
}

fn mcp_binding_sort_rank(item: &AgentMcpBindingView) -> (u8, u8, u8) {
    let bound_rank = if item.mode == MCP_BINDING_MODE_DISABLED {
        1
    } else {
        0
    };
    let bindable_rank = if item.bindable { 0 } else { 1 };
    let visibility_rank = match item.mcp.visibility.as_str() {
        VISIBILITY_SYSTEM_PRIVATE => 0,
        VISIBILITY_PUBLIC => 1,
        _ => 2,
    };
    (bound_rank, bindable_rank, visibility_rank)
}

fn mcp_is_local_connector_scoped(mcp: &McpRecord) -> bool {
    mcp.source_kind == SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED
        || matches!(
            mcp.runtime.kind.as_str(),
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
                | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
                | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
        )
        || mcp.runtime.local_connector.is_some()
}

fn cloud_task_runner_external_mcp_unavailable_reason(
    agent: &SystemAgentRecord,
    mcp: &McpRecord,
) -> Option<&'static str> {
    if !matches!(
        mcp.runtime.kind.as_str(),
        RUNTIME_KIND_HTTP | RUNTIME_KIND_STDIO_CLOUD
    ) {
        return Some(AGENT_MCP_UNAVAILABLE_CLOUD_RUNTIME);
    }
    if task_runner_agent_is_planning_phase(agent.agent_key.as_str())
        && mcp.security.allow_writes != Some(false)
    {
        return Some(AGENT_MCP_UNAVAILABLE_PLANNING_WRITES);
    }
    None
}

fn task_runner_agent_is_planning_phase(agent_key: &str) -> bool {
    matches!(
        agent_key,
        "task_runner_plan_phase" | "task_runner_local_plan_phase"
    )
}

fn agent_mcp_unavailable_message(reason: &str) -> &'static str {
    match reason {
        AGENT_MCP_UNAVAILABLE_DISABLED => "MCP is disabled",
        AGENT_MCP_UNAVAILABLE_PRIVATE_SCOPE => {
            "private MCPs are user scoped and cannot be saved as a global system-agent binding"
        }
        AGENT_MCP_UNAVAILABLE_LOCAL_CONNECTOR_SCOPE => {
            "Local Connector MCPs are owner/device scoped and are merged at runtime"
        }
        AGENT_MCP_UNAVAILABLE_HOST_UNSUPPORTED => {
            "the MCP does not support this agent runtime host"
        }
        AGENT_MCP_UNAVAILABLE_PHASE_FORBIDS_BUILTIN => {
            "this Task Runner phase does not allow that builtin MCP"
        }
        AGENT_MCP_UNAVAILABLE_CLOUD_RUNTIME => {
            "cloud Task Runner only supports HTTP or cloud stdio external MCPs"
        }
        AGENT_MCP_UNAVAILABLE_PLANNING_WRITES => {
            "planning agents only allow external MCPs that explicitly disallow writes"
        }
        AGENT_MCP_UNAVAILABLE_LOCAL_EXTERNAL_GLOBAL => {
            "Local Connector external MCPs cannot be saved as global bindings"
        }
        _ => "MCP is not bindable",
    }
}

fn task_runner_agent_allows_builtin(
    agent_key: &str,
    kind: chatos_mcp_runtime::BuiltinMcpKind,
) -> bool {
    if matches!(
        agent_key,
        "task_runner_plan_phase" | "task_runner_local_plan_phase"
    ) {
        return !matches!(
            kind,
            chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
                | chatos_mcp_runtime::BuiltinMcpKind::TerminalController
                | chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController
        );
    }
    if matches!(
        agent_key,
        "task_runner_run_phase" | "task_runner_local_run_phase"
    ) {
        return !matches!(
            kind,
            chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController
        );
    }
    true
}

fn agent_mcp_host(agent: &SystemAgentRecord) -> Option<chatos_mcp::SystemMcpHost> {
    match agent.agent_key.as_str() {
        "task_runner_plan_phase" | "task_runner_run_phase" => {
            Some(chatos_mcp::SystemMcpHost::TaskRunner)
        }
        "task_runner_local_plan_phase" | "task_runner_local_run_phase" => {
            Some(chatos_mcp::SystemMcpHost::LocalConnector)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
