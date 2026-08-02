// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{
    CloseRuntimeSessionResponse, CreateRuntimeSessionRequest, McpProviderKind, ResolvedMcpRoute,
    RuntimeSessionResponse, RuntimeSessionRoutesResponse, SandboxExecutionTarget,
    SandboxProviderKind, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{ResolveAgentCapabilitiesRequest, SystemAgentKey};
use uuid::Uuid;

use crate::auth::require_internal_request;
use crate::capabilities::{
    materialize_mcp_candidates, materialize_runtime_tools_with_plugin_components,
    runtime_route_revision,
};
use crate::error::ApiError;
use crate::runtime::{RuntimeGrantClaims, RuntimeSessionSnapshot};
use crate::state::AppState;

use super::runtime_session_metadata::resolve_runtime_session_prompt_metadata;

mod routing;
use routing::*;

pub(super) async fn resolve_runtime_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRuntimeSessionRequest>,
) -> Result<Json<RuntimeSessionResponse>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.sessions.resolve")?;
    validate_session_request(&request)?;
    let sandbox_target = normalize_sandbox_target(request.sandbox_target.clone())?;
    let agent_key = parse_agent_key(request.agent_key.as_str())?;
    let contact_agent_id = normalized(request.contact_agent_id.clone());
    let expected_project_task_ids = normalized_unique_items(
        request.expected_project_task_ids.clone(),
        "expected_project_task_ids",
        200,
    )?;
    let mut project_context = state
        .project_context_client
        .resolve(request.project_id.as_str(), request.owner_user_id.as_str())
        .await
        .map_err(ApiError::bad_gateway)?;
    if project_context.sandbox_provider == SandboxProviderKind::LocalConnector {
        let pairing_id = state
            .providers
            .resolve_local_sandbox_pairing(&project_context)
            .await
            .map_err(|error| ApiError::conflict(error.message))?;
        if pairing_id.is_none() {
            return Err(ApiError::conflict(
                "Local Connector sandbox is configured, but the Project Context device/workspace has no active enabled and ready sandbox pairing",
            ));
        }
        project_context.sandbox_pairing_id = pairing_id;
    }
    validate_context_overrides(&request, &project_context)?;
    let device_id = project_context
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.device_id.clone());
    let runtime_provider = capability_runtime_provider(&project_context);
    let capability_request =
        ResolveAgentCapabilitiesRequest::new(agent_key, request.owner_user_id.trim().to_string())
            .with_runtime_context(
                normalized(request.task_profile.clone()),
                project_context.source_type.clone(),
                Some(runtime_provider.to_string()),
                None,
            )
            .with_device_id(device_id);
    let mut capabilities = state
        .plugin_management_client
        .resolve_for_service(&capability_request)
        .await
        .map_err(|err| {
            ApiError::bad_gateway(format!("resolve Agent capabilities failed: {err}"))
        })?;
    validate_capability_identity(
        &capabilities,
        agent_key.as_str(),
        request.owner_user_id.trim(),
    )?;
    if !capabilities.agent_enabled {
        return Err(ApiError::conflict("configured Agent is disabled"));
    }
    let session_id = format!("mcp_session_{}", Uuid::new_v4().simple());
    let expires_at_unix = state
        .runtime_grants
        .next_expires_at_unix()
        .map_err(ApiError::internal)?;
    let materialized = materialize_mcp_candidates(&capabilities).map_err(ApiError::conflict)?;
    let mut route_response =
        state
            .routing
            .resolve(chatos_mcp_management_sdk::ResolveMcpRoutesRequest {
                context: project_context.clone(),
                resources: materialized.resources,
            });
    bind_agent_callback_routes(route_response.routes.as_mut_slice(), agent_key);
    bind_chatos_memory_routes(
        route_response.routes.as_mut_slice(),
        agent_key,
        contact_agent_id.as_deref(),
        request.source_session_id.as_deref(),
    );
    bind_runtime_sandbox_routes(
        route_response.routes.as_mut_slice(),
        sandbox_target.as_ref(),
    );
    let cloud_sandbox_target = sandbox_target
        .as_ref()
        .filter(|target| target.provider == SandboxProviderKind::Cloud);
    bind_cloud_stdio_routes(route_response.routes.as_mut_slice(), cloud_sandbox_target);
    bind_sandbox_image_routes(route_response.routes.as_mut_slice(), &project_context);
    let plugin_cloud_requires_sandbox = route_response.routes.iter().any(|route| {
        route.provider_kind == McpProviderKind::PluginCloud
            && materialized
                .plugin_bindings
                .get(route.resource_id.as_str())
                .is_some_and(|binding| {
                    matches!(
                        &binding.runtime,
                        chatos_plugin_management_sdk::PluginMcpServer::Stdio { .. }
                    )
                })
    });
    let routed_sandbox_target_required = route_response
        .routes
        .iter()
        .any(|route| state.providers.requires_sandbox_target(route));
    if routed_sandbox_target_required || (plugin_cloud_requires_sandbox && sandbox_target.is_some())
    {
        let target = sandbox_target.as_ref().ok_or_else(|| {
            ApiError::conflict("Cloud sandbox-backed route requires a bound sandbox target")
        })?;
        state
            .providers
            .validate_sandbox_target(
                target,
                request.owner_user_id.trim(),
                request.project_id.trim(),
                request.run_id.as_deref(),
            )
            .await
            .map_err(|error| ApiError::conflict(error.message))?;
    }
    let chatos_tool_snapshots = state
        .providers
        .prepare_chatos_routes(
            route_response.routes.as_mut_slice(),
            session_id.as_str(),
            request.owner_user_id.trim(),
            agent_key,
            request.project_id.trim(),
            request.source_session_id.as_deref(),
            expires_at_unix,
        )
        .await;
    let task_runner_tool_snapshots = state
        .providers
        .prepare_task_runner_routes(
            route_response.routes.as_mut_slice(),
            session_id.as_str(),
            request.owner_user_id.trim(),
            agent_key,
            request.project_id.trim(),
            request.run_id.as_deref(),
            request.turn_id.as_deref(),
            request.task_id.as_deref(),
            request.source_session_id.as_deref(),
            request.source_user_message_id.as_deref(),
            request.default_model_config_id.as_deref(),
            request.task_profile.as_deref(),
            expected_project_task_ids.as_slice(),
            expires_at_unix,
        )
        .await;
    let (mut cloud_stdio_bindings, cloud_stdio_tool_snapshots) = state
        .providers
        .prepare_cloud_stdio_routes(
            &capabilities,
            route_response.routes.as_mut_slice(),
            cloud_sandbox_target,
            session_id.as_str(),
            request.owner_user_id.trim(),
            request.project_id.trim(),
            request.run_id.as_deref(),
            expires_at_unix,
        )
        .await;
    let (plugin_local_bindings, mut plugin_tool_snapshots) = state
        .providers
        .prepare_plugin_local_routes(
            &materialized.plugin_bindings,
            route_response.routes.as_mut_slice(),
            &project_context,
            session_id.as_str(),
            request.owner_user_id.trim(),
            expires_at_unix,
        )
        .await;
    let (plugin_cloud_stdio_bindings, plugin_cloud_http_bindings, plugin_cloud_tool_snapshots) =
        state
            .providers
            .prepare_plugin_cloud_routes(
                &state.plugin_management_client,
                &materialized.plugin_bindings,
                route_response.routes.as_mut_slice(),
                &project_context,
                cloud_sandbox_target,
                session_id.as_str(),
                request.owner_user_id.trim(),
                request.project_id.trim(),
                request.run_id.as_deref(),
                expires_at_unix,
            )
            .await;
    cloud_stdio_bindings.extend(plugin_cloud_stdio_bindings);
    plugin_tool_snapshots.extend(plugin_cloud_tool_snapshots);
    let (
        plugin_local_tool_component_bindings,
        plugin_cloud_tool_component_bindings,
        plugin_component_tool_snapshots,
    ) = state
        .providers
        .prepare_plugin_tool_component_routes(
            &state.plugin_management_client,
            &materialized.plugin_tool_component_bindings,
            route_response.routes.as_mut_slice(),
            &project_context,
            session_id.as_str(),
            request.owner_user_id.trim(),
            expires_at_unix,
        )
        .await;
    let cleanup_owner_user_id = request.owner_user_id.trim().to_string();
    let cleanup_session_id = session_id.clone();
    let cleanup_plugin_local_bindings = plugin_local_bindings.clone();
    let cleanup_plugin_local_tool_component_bindings = plugin_local_tool_component_bindings.clone();
    let cleanup_cloud_stdio_bindings = cloud_stdio_bindings.clone();
    let cleanup_sandbox_target = cloud_sandbox_target.cloned();
    let cleanup_project_id = request.project_id.trim().to_string();
    let cleanup_run_id = request.run_id.clone();
    let result = async {
        apply_live_tool_snapshots(&mut capabilities, chatos_tool_snapshots);
        apply_live_tool_snapshots(&mut capabilities, task_runner_tool_snapshots);
        apply_live_tool_snapshots(&mut capabilities, cloud_stdio_tool_snapshots);
        let mut external_http_bindings = state
            .providers
            .prepare_external_http_routes(&capabilities, route_response.routes.as_mut_slice())
            .await;
        external_http_bindings.extend(plugin_cloud_http_bindings);
        for route in &mut route_response.routes {
            route.cancel_supported &= state.providers.supports_cancellation(route);
        }
        let tool_result = materialize_runtime_tools_with_plugin_components(
            &capabilities,
            route_response.routes.as_slice(),
            &materialized.plugin_bindings,
            &plugin_tool_snapshots,
            &materialized.plugin_tool_component_bindings,
            &plugin_component_tool_snapshots,
        )
        .map_err(ApiError::conflict)?;
        let route_revision = runtime_route_revision(
            route_response.route_revision.as_str(),
            capabilities.policy_revision.as_str(),
            route_response.routes.as_slice(),
            tool_result.tools.as_slice(),
        )
        .map_err(ApiError::internal)?;
        validate_task_runner_provider_context(
            agent_key,
            &request,
            expected_project_task_ids.as_slice(),
            route_response.routes.as_slice(),
        )?;
        let mut unavailable_required_mcps = materialized.unavailable_required_resources;
        unavailable_required_mcps.extend(route_response.unavailable_required_mcps);
        unavailable_required_mcps.extend(tool_result.missing_required_tool_schemas);
        let mut required_resource_ids = capabilities
            .mcps
            .iter()
            .filter(|resolved| {
                resolved.binding.enabled && resolved.binding.required && resolved.resource.enabled
            })
            .map(|resolved| resolved.resource.id.clone())
            .collect::<HashSet<_>>();
        required_resource_ids.extend(
            materialized
                .plugin_bindings
                .values()
                .filter(|binding| binding.required)
                .map(|binding| binding.resource_id.clone()),
        );
        required_resource_ids.extend(
            materialized
                .plugin_tool_component_bindings
                .values()
                .filter(|binding| binding.required)
                .map(|binding| binding.resource_id.clone()),
        );
        unavailable_required_mcps.extend(required_routes_without_provider_adapter(
            &required_resource_ids,
            route_response.routes.as_slice(),
            |route| state.providers.supports(route),
        ));
        unavailable_required_mcps.sort();
        unavailable_required_mcps.dedup();
        if !unavailable_required_mcps.is_empty() {
            return Err(ApiError::conflict(format!(
                "required MCPs cannot be materialized: {}",
                unavailable_required_mcps.join(", ")
            )));
        }
        let mut allowed_resource_ids = route_response
            .routes
            .iter()
            .map(|route| route.resource_id.clone())
            .collect::<Vec<_>>();
        allowed_resource_ids.sort();
        allowed_resource_ids.dedup();
        let claims = RuntimeGrantClaims {
            iss: String::new(),
            sub: caller_service.clone(),
            aud: String::new(),
            session_id: session_id.clone(),
            owner_user_id: request.owner_user_id.trim().to_string(),
            agent_key: agent_key.as_str().to_string(),
            task_profile: normalized(request.task_profile.clone()),
            project_id: request.project_id.trim().to_string(),
            run_id: normalized(request.run_id.clone()),
            turn_id: normalized(request.turn_id.clone()),
            task_id: normalized(request.task_id.clone()),
            source_session_id: normalized(request.source_session_id.clone()),
            source_user_message_id: normalized(request.source_user_message_id.clone()),
            contact_agent_id: contact_agent_id.clone(),
            default_model_config_id: normalized(request.default_model_config_id.clone()),
            expected_project_task_ids: expected_project_task_ids.clone(),
            policy_revision: capabilities.policy_revision.clone(),
            route_revision: route_revision.clone(),
            allowed_resource_ids,
            iat: 0,
            exp: 0,
        };
        let grant = state
            .runtime_grants
            .issue_with_expires_at(claims, expires_at_unix)
            .map_err(ApiError::internal)?;
        let configured_mcp_count = route_response.routes.len();
        let exposed_tool_count = tool_result.tools.len();
        let prompt_metadata = resolve_runtime_session_prompt_metadata(
            &capabilities,
            tool_result.tools.as_slice(),
            request.locale.as_deref(),
        );
        let snapshot = RuntimeSessionSnapshot {
            session_id: session_id.clone(),
            caller_service,
            owner_user_id: request.owner_user_id.trim().to_string(),
            agent_key: agent_key.as_str().to_string(),
            task_profile: normalized(request.task_profile),
            project_id: request.project_id.trim().to_string(),
            run_id: normalized(request.run_id),
            turn_id: normalized(request.turn_id),
            task_id: normalized(request.task_id),
            source_session_id: normalized(request.source_session_id),
            source_user_message_id: normalized(request.source_user_message_id),
            contact_agent_id,
            default_model_config_id: normalized(request.default_model_config_id),
            expected_project_task_ids,
            sandbox_target,
            project_context,
            policy_revision: capabilities.policy_revision.clone(),
            route_revision: route_revision.clone(),
            routes: route_response.routes,
            tools: tool_result.tools,
            plugin_mcp_bindings: materialized.plugin_bindings,
            plugin_local_bindings,
            plugin_tool_component_bindings: materialized.plugin_tool_component_bindings,
            plugin_local_tool_component_bindings,
            plugin_cloud_tool_component_bindings,
            external_http_bindings,
            cloud_stdio_bindings,
            expires_at: grant.expires_at.clone(),
            expires_at_unix: grant.expires_at_unix,
        };
        state
            .runtime_sessions
            .insert(snapshot)
            .await
            .map_err(ApiError::internal)?;
        Ok(Json(RuntimeSessionResponse {
            session_id,
            policy_revision: capabilities.policy_revision,
            route_revision,
            expires_at: grant.expires_at,
            mcp_server_url: format!("{}/mcp", state.config.public_base_url),
            runtime_token: grant.token,
            configured_mcp_count,
            exposed_tool_count,
            effective_mcp_ids: prompt_metadata.effective_mcp_ids,
            provider_skills_prompt: prompt_metadata.provider_skills_prompt,
            unavailable_required_mcps,
        }))
    }
    .await;
    if result.is_err() && !cleanup_plugin_local_bindings.is_empty() {
        state
            .providers
            .close_prepared_plugin_local_bindings(
                cleanup_owner_user_id.as_str(),
                cleanup_session_id.as_str(),
                &cleanup_plugin_local_bindings,
            )
            .await;
    }
    if result.is_err() && !cleanup_plugin_local_tool_component_bindings.is_empty() {
        state
            .providers
            .close_prepared_plugin_tool_component_bindings(
                cleanup_owner_user_id.as_str(),
                cleanup_session_id.as_str(),
                &cleanup_plugin_local_tool_component_bindings,
            )
            .await;
    }
    if result.is_err() && !cleanup_cloud_stdio_bindings.is_empty() {
        if let Some(target) = cleanup_sandbox_target.as_ref() {
            state
                .providers
                .close_prepared_cloud_stdio_bindings(
                    target,
                    cleanup_session_id.as_str(),
                    cleanup_owner_user_id.as_str(),
                    cleanup_project_id.as_str(),
                    cleanup_run_id.as_deref(),
                    expires_at_unix,
                    &cleanup_cloud_stdio_bindings,
                )
                .await;
        }
    }
    result
}

pub(super) async fn runtime_session_routes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<RuntimeSessionRoutesResponse>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.sessions.read")?;
    let snapshot = state
        .runtime_sessions
        .get(session_id.trim())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime session was not found or has expired"))?;
    if snapshot.caller_service != caller_service {
        return Err(ApiError::forbidden(
            "runtime session belongs to another caller service",
        ));
    }
    Ok(Json(snapshot.routes_response()))
}

pub(super) async fn close_runtime_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<CloseRuntimeSessionResponse>, ApiError> {
    let caller_service =
        require_internal_request(&state.config, &headers, "runtime.sessions.close")?;
    let session_id = session_id.trim();
    let snapshot = state
        .runtime_sessions
        .get(session_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime session was not found or has expired"))?;
    if snapshot.caller_service != caller_service {
        return Err(ApiError::forbidden(
            "runtime session belongs to another caller service",
        ));
    }
    let Some(snapshot) = state
        .runtime_sessions
        .remove(session_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found(
            "runtime session was already closed or expired",
        ));
    };
    state.providers.close_session(&snapshot).await;
    Ok(Json(CloseRuntimeSessionResponse {
        session_id: snapshot.session_id,
        closed: true,
    }))
}

fn apply_live_tool_snapshots(
    capabilities: &mut chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    mut snapshots: HashMap<String, Vec<serde_json::Value>>,
) {
    for resolved in &mut capabilities.mcps {
        if let Some(tools) = snapshots.remove(resolved.resource.id.as_str()) {
            resolved.tool_snapshot = tools;
            resolved.available = true;
            resolved.status = "ready".to_string();
            resolved.reason = None;
        }
    }
}

fn capability_runtime_provider(
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> &'static str {
    // Plugin Management's runtime_provider condition expresses execution
    // locality (cloud or Local Connector), not the concrete workspace backend.
    // Harness, Cloud Sandbox and Cloud Storage are all cloud execution
    // locations. Keeping this translation here lets capability policy remain
    // stable while MCP Management independently chooses the concrete provider
    // from the authoritative Project Execution Context.
    match context.workspace_provider {
        WorkspaceProviderKind::LocalConnector => WorkspaceProviderKind::LocalConnector.as_str(),
        WorkspaceProviderKind::Harness
        | WorkspaceProviderKind::CloudSandbox
        | WorkspaceProviderKind::CloudStorage
        | WorkspaceProviderKind::None => "cloud",
    }
}

fn parse_agent_key(value: &str) -> Result<SystemAgentKey, ApiError> {
    let value = value.trim();
    let agent_key = SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == value)
        .ok_or_else(|| ApiError::bad_request(format!("unknown system Agent key: {value}")))?;
    let tool_plane = chatos_agent::agent_descriptor(agent_key).tool_plane;
    if !tool_plane.uses_managed_gateway() {
        return Err(ApiError::conflict(format!(
            "system Agent {value} does not use the managed MCP Tool Plane"
        )));
    }
    Ok(agent_key)
}

fn validate_session_request(request: &CreateRuntimeSessionRequest) -> Result<(), ApiError> {
    for (field, value) in [
        ("owner_user_id", request.owner_user_id.as_str()),
        ("agent_key", request.agent_key.as_str()),
        ("project_id", request.project_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(format!("{field} is required")));
        }
    }
    Ok(())
}

fn normalized_unique_items(
    values: Vec<String>,
    field: &str,
    max_items: usize,
) -> Result<Vec<String>, ApiError> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.len() > max_items {
        return Err(ApiError::bad_request(format!(
            "{field} exceeds {max_items} items"
        )));
    }
    if values.iter().any(|value| value.len() > 256) {
        return Err(ApiError::bad_request(format!(
            "{field} contains an item longer than 256 bytes"
        )));
    }
    let encoded_bytes = values
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(values.len().saturating_sub(1));
    if encoded_bytes > 12 * 1024 {
        return Err(ApiError::bad_request(format!(
            "{field} exceeds 12288 encoded bytes"
        )));
    }
    Ok(values)
}

fn validate_task_runner_provider_context(
    agent_key: SystemAgentKey,
    request: &CreateRuntimeSessionRequest,
    expected_project_task_ids: &[String],
    routes: &[ResolvedMcpRoute],
) -> Result<(), ApiError> {
    let has_route = |system_key| {
        let resource_id = chatos_mcp::system_mcp_descriptor(system_key).resource_id;
        routes.iter().any(|route| route.resource_id == resource_id)
    };
    let has_task_runner_ask_user_route = routes.iter().any(|route| {
        route.resource_id == chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
            && route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some("task-runner")
    });
    let has_chatos_ask_user_route = routes.iter().any(|route| {
        route.resource_id == chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
            && route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some("chatos")
    });
    if has_route(SystemMcpKey::TaskRunnerService) {
        if !matches!(
            agent_key,
            SystemAgentKey::ChatosConversationAgent
                | SystemAgentKey::ChatosPlanningAgent
                | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        ) {
            return Err(ApiError::conflict(
                "Task Runner Service MCP is only valid for ChatOS task planning Agents",
            ));
        }
        for (field, value) in [
            ("source_session_id", request.source_session_id.as_deref()),
            (
                "source_user_message_id",
                request.source_user_message_id.as_deref(),
            ),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Runner Service MCP requires {field}"
                )));
            }
        }
        if agent_key == SystemAgentKey::ProjectRequirementExecutionPlannerAgent
            && expected_project_task_ids.is_empty()
        {
            return Err(ApiError::conflict(
                "project requirement execution planner requires expected_project_task_ids",
            ));
        }
    }
    if has_route(SystemMcpKey::TaskProcessLog) {
        if !matches!(
            agent_key,
            SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase
        ) {
            return Err(ApiError::conflict(
                "Task Process Log MCP is only valid for Task Runner phase Agents",
            ));
        }
        for (field, value) in [
            ("run_id", request.run_id.as_deref()),
            ("task_id", request.task_id.as_deref()),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Process Log MCP requires {field}"
                )));
            }
        }
    }
    if has_task_runner_ask_user_route {
        if !matches!(
            agent_key,
            SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase
        ) {
            return Err(ApiError::conflict(
                "Task Runner Ask User MCP is only valid for Task Runner phase Agents",
            ));
        }
        for (field, value) in [
            ("run_id", request.run_id.as_deref()),
            ("task_id", request.task_id.as_deref()),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Runner Ask User MCP requires {field}"
                )));
            }
        }
    }
    if has_chatos_ask_user_route {
        if !matches!(
            agent_key,
            SystemAgentKey::ChatosConversationAgent
                | SystemAgentKey::ChatosPlanningAgent
                | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        ) {
            return Err(ApiError::conflict(
                "ChatOS Ask User MCP is only valid for ChatOS conversation Agents",
            ));
        }
        for (field, value) in [
            ("turn_id", request.turn_id.as_deref()),
            ("source_session_id", request.source_session_id.as_deref()),
            (
                "source_user_message_id",
                request.source_user_message_id.as_deref(),
            ),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "ChatOS Ask User MCP requires {field}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
