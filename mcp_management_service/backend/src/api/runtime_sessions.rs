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
    materialize_mcp_candidates, materialize_runtime_tools, runtime_route_revision,
};
use crate::error::ApiError;
use crate::runtime::{RuntimeGrantClaims, RuntimeSessionSnapshot};
use crate::state::AppState;

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
    let project_context = state
        .project_context_client
        .resolve(request.project_id.as_str(), request.owner_user_id.as_str())
        .await
        .map_err(ApiError::bad_gateway)?;
    validate_context_overrides(&request, &project_context)?;
    let device_id = project_context
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.device_id.clone());
    let capability_request =
        ResolveAgentCapabilitiesRequest::new(agent_key, request.owner_user_id.trim().to_string())
            .with_runtime_context(
                normalized(request.task_profile.clone()),
                project_context.source_type.clone(),
                Some(project_context.workspace_provider.as_str().to_string()),
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
    let materialized = materialize_mcp_candidates(&capabilities);
    let mut route_response =
        state
            .routing
            .resolve(chatos_mcp_management_sdk::ResolveMcpRoutesRequest {
                context: project_context.clone(),
                resources: materialized.resources,
            });
    bind_agent_callback_routes(route_response.routes.as_mut_slice(), agent_key);
    bind_cloud_sandbox_routes(
        route_response.routes.as_mut_slice(),
        sandbox_target.as_ref(),
    );
    bind_cloud_stdio_routes(
        route_response.routes.as_mut_slice(),
        sandbox_target.as_ref(),
    );
    bind_sandbox_image_routes(route_response.routes.as_mut_slice(), &project_context);
    if route_response
        .routes
        .iter()
        .any(|route| state.providers.requires_sandbox_target(route))
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
    let (cloud_stdio_bindings, cloud_stdio_tool_snapshots) = state
        .providers
        .prepare_cloud_stdio_routes(
            &capabilities,
            route_response.routes.as_mut_slice(),
            sandbox_target.as_ref(),
            session_id.as_str(),
            request.owner_user_id.trim(),
            request.project_id.trim(),
            request.run_id.as_deref(),
            expires_at_unix,
        )
        .await;
    apply_live_tool_snapshots(&mut capabilities, cloud_stdio_tool_snapshots);
    let external_http_bindings = state
        .providers
        .prepare_external_http_routes(&capabilities, route_response.routes.as_mut_slice())
        .await;
    let tool_result = materialize_runtime_tools(&capabilities, route_response.routes.as_slice())
        .map_err(ApiError::conflict)?;
    let route_revision = runtime_route_revision(
        route_response.route_revision.as_str(),
        capabilities.policy_revision.as_str(),
        route_response.routes.as_slice(),
        tool_result.tools.as_slice(),
    )
    .map_err(ApiError::internal)?;
    let expected_project_task_ids = normalized_unique_items(
        request.expected_project_task_ids.clone(),
        "expected_project_task_ids",
        200,
    )?;
    validate_task_runner_provider_context(
        agent_key,
        &request,
        expected_project_task_ids.as_slice(),
        route_response.routes.as_slice(),
    )?;
    let mut unavailable_required_mcps = route_response.unavailable_required_mcps;
    unavailable_required_mcps.extend(tool_result.missing_required_tool_schemas);
    let required_resource_ids = capabilities
        .mcps
        .iter()
        .filter(|resolved| {
            resolved.binding.enabled && resolved.binding.required && resolved.resource.enabled
        })
        .map(|resolved| resolved.resource.id.as_str())
        .collect::<HashSet<_>>();
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
        project_id: request.project_id.trim().to_string(),
        run_id: normalized(request.run_id.clone()),
        turn_id: normalized(request.turn_id.clone()),
        task_id: normalized(request.task_id.clone()),
        source_session_id: normalized(request.source_session_id.clone()),
        source_user_message_id: normalized(request.source_user_message_id.clone()),
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
    let snapshot = RuntimeSessionSnapshot {
        session_id: session_id.clone(),
        caller_service,
        owner_user_id: request.owner_user_id.trim().to_string(),
        agent_key: agent_key.as_str().to_string(),
        project_id: request.project_id.trim().to_string(),
        run_id: normalized(request.run_id),
        turn_id: normalized(request.turn_id),
        task_id: normalized(request.task_id),
        source_session_id: normalized(request.source_session_id),
        source_user_message_id: normalized(request.source_user_message_id),
        default_model_config_id: normalized(request.default_model_config_id),
        expected_project_task_ids,
        sandbox_target,
        project_context,
        policy_revision: capabilities.policy_revision.clone(),
        route_revision: route_revision.clone(),
        routes: route_response.routes,
        tools: tool_result.tools,
        external_http_bindings,
        cloud_stdio_bindings,
        expires_at: grant.expires_at.clone(),
        expires_at_unix: grant.expires_at_unix,
    };
    state.runtime_sessions.insert(snapshot).await;
    Ok(Json(RuntimeSessionResponse {
        session_id,
        policy_revision: capabilities.policy_revision,
        route_revision,
        expires_at: grant.expires_at,
        mcp_server_url: format!("{}/mcp", state.config.public_base_url),
        runtime_token: grant.token,
        configured_mcp_count,
        exposed_tool_count,
        unavailable_required_mcps,
    }))
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
        .ok_or_else(|| ApiError::not_found("runtime session was not found or has expired"))?;
    if snapshot.caller_service != caller_service {
        return Err(ApiError::forbidden(
            "runtime session belongs to another caller service",
        ));
    }
    let Some(snapshot) = state.runtime_sessions.remove(session_id).await else {
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

fn parse_agent_key(value: &str) -> Result<SystemAgentKey, ApiError> {
    let value = value.trim();
    SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == value)
        .ok_or_else(|| ApiError::bad_request(format!("unknown system Agent key: {value}")))
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
            SystemAgentKey::TaskRunnerPlanPhase
                | SystemAgentKey::TaskRunnerLocalPlanPhase
                | SystemAgentKey::TaskRunnerRunPhase
                | SystemAgentKey::TaskRunnerLocalRunPhase
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
            SystemAgentKey::TaskRunnerPlanPhase
                | SystemAgentKey::TaskRunnerLocalPlanPhase
                | SystemAgentKey::TaskRunnerRunPhase
                | SystemAgentKey::TaskRunnerLocalRunPhase
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

fn bind_agent_callback_routes(routes: &mut [ResolvedMcpRoute], agent_key: SystemAgentKey) {
    let ask_user_resource_id = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id;
    for route in routes
        .iter_mut()
        .filter(|route| route.resource_id == ask_user_resource_id)
    {
        match agent_key {
            SystemAgentKey::TaskRunnerPlanPhase
            | SystemAgentKey::TaskRunnerLocalPlanPhase
            | SystemAgentKey::TaskRunnerRunPhase
            | SystemAgentKey::TaskRunnerLocalRunPhase => {
                route.provider_kind = McpProviderKind::InternalService;
                route.provider_ref = Some("task-runner".to_string());
                route.reason =
                    "Ask User is pinned to the Task Runner Agent callback host".to_string();
            }
            SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent => {
                route.provider_kind = McpProviderKind::InternalService;
                route.provider_ref = Some("chatos".to_string());
                route.reason = "Ask User is pinned to the ChatOS Agent callback host".to_string();
            }
            _ => {
                route.provider_kind = McpProviderKind::Unavailable;
                route.provider_ref = None;
                route.reason =
                    "configured Agent has no registered Ask User callback host".to_string();
            }
        }
    }
}

fn normalize_sandbox_target(
    target: Option<SandboxExecutionTarget>,
) -> Result<Option<SandboxExecutionTarget>, ApiError> {
    let Some(mut target) = target else {
        return Ok(None);
    };
    target.sandbox_id = target.sandbox_id.trim().to_string();
    target.lease_id = target.lease_id.trim().to_string();
    target.service_id = normalized(target.service_id);
    if target.sandbox_id.is_empty() || target.lease_id.is_empty() {
        return Err(ApiError::bad_request(
            "sandbox_target requires sandbox_id and lease_id",
        ));
    }
    if target.is_environment && target.service_id.is_none() {
        return Err(ApiError::bad_request(
            "sandbox environment target requires service_id",
        ));
    }
    if !target.is_environment && target.service_id.is_some() {
        return Err(ApiError::bad_request(
            "sandbox service_id is only valid for an environment target",
        ));
    }
    Ok(Some(target))
}

fn bind_cloud_sandbox_routes(
    routes: &mut [ResolvedMcpRoute],
    target: Option<&SandboxExecutionTarget>,
) {
    for route in routes.iter_mut().filter(|route| {
        route.provider_kind == McpProviderKind::CloudSandbox
            && route.resource_id
                != chatos_mcp::system_mcp_descriptor(SystemMcpKey::SandboxImages).resource_id
    }) {
        if let Some(target) = target {
            route.provider_ref = Some(target.provider_ref());
        } else {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.reason = "Cloud Sandbox route requires a runtime sandbox lease".to_string();
        }
    }
}

fn bind_sandbox_image_routes(
    routes: &mut [ResolvedMcpRoute],
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) {
    let resource_id = chatos_mcp::system_mcp_descriptor(SystemMcpKey::SandboxImages).resource_id;
    for route in routes
        .iter_mut()
        .filter(|route| route.resource_id == resource_id)
    {
        route.cancel_supported = false;
        match (context.sandbox_provider, route.provider_kind) {
            (SandboxProviderKind::Cloud, McpProviderKind::CloudSandbox) => {
                route.provider_ref =
                    Some(crate::providers::sandbox_images_cloud_provider_ref().to_string());
                route.reason = "Sandbox Images is pinned to the cloud Sandbox Manager".to_string();
            }
            (SandboxProviderKind::LocalConnector, McpProviderKind::LocalConnector) => {
                let Some(pairing_id) = context
                    .sandbox_pairing_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.allow_writes = false;
                    route.reason =
                        "local Sandbox Images requires a bound sandbox pairing".to_string();
                    continue;
                };
                route.provider_ref = Some(crate::providers::sandbox_images_local_provider_ref(
                    pairing_id,
                ));
                route.reason =
                    "Sandbox Images is pinned to the Local Connector sandbox pairing".to_string();
            }
            _ => {
                route.provider_kind = McpProviderKind::Unavailable;
                route.provider_ref = None;
                route.allow_writes = false;
                route.reason =
                    "Sandbox Images provider does not match the project sandbox policy".to_string();
            }
        }
    }
}

fn bind_cloud_stdio_routes(
    routes: &mut [ResolvedMcpRoute],
    target: Option<&SandboxExecutionTarget>,
) {
    for route in routes
        .iter_mut()
        .filter(|route| route.provider_kind == McpProviderKind::CloudStdio)
    {
        if let Some(target) = target {
            route.provider_ref = Some(target.provider_ref());
        } else {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.allow_writes = false;
            route.cancel_supported = false;
            route.reason = "Cloud stdio MCP requires a runtime sandbox lease".to_string();
        }
    }
}

fn validate_capability_identity(
    capabilities: &chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    expected_agent_key: &str,
    expected_owner_user_id: &str,
) -> Result<(), ApiError> {
    if capabilities.agent_key.trim() != expected_agent_key
        || capabilities.owner_user_id.trim() != expected_owner_user_id
    {
        return Err(ApiError::bad_gateway(
            "Plugin Management returned capabilities for a different Agent or owner",
        ));
    }
    Ok(())
}

fn validate_context_overrides(
    request: &CreateRuntimeSessionRequest,
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> Result<(), ApiError> {
    if context.project_id != request.project_id.trim()
        || context.owner_user_id != request.owner_user_id.trim()
    {
        return Err(ApiError::forbidden(
            "project execution context identity does not match the request",
        ));
    }
    if let Some(requested_device_id) = normalized(request.requested_device_id.clone()) {
        let context_device_id = context
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.device_id.as_deref());
        if context_device_id != Some(requested_device_id.as_str()) {
            return Err(ApiError::conflict(
                "requested device is not the Project Context device",
            ));
        }
    }
    if let Some(requested) = request.requested_sandbox_provider {
        let workspace_authorizes_cloud = requested == SandboxProviderKind::Cloud
            && context.workspace_provider == WorkspaceProviderKind::CloudSandbox;
        if requested != context.sandbox_provider && !workspace_authorizes_cloud {
            return Err(ApiError::conflict(
                "sandbox provider override is not authorized by Project Context",
            ));
        }
    }
    if request.sandbox_target.is_some()
        && context.sandbox_provider != SandboxProviderKind::Cloud
        && context.workspace_provider != WorkspaceProviderKind::CloudSandbox
    {
        return Err(ApiError::conflict(
            "sandbox target is not authorized by Project Context",
        ));
    }
    Ok(())
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_routes_without_provider_adapter(
    required_resource_ids: &HashSet<&str>,
    routes: &[chatos_mcp_management_sdk::ResolvedMcpRoute],
    mut supports: impl FnMut(&chatos_mcp_management_sdk::ResolvedMcpRoute) -> bool,
) -> Vec<String> {
    routes
        .iter()
        .filter(|route| {
            required_resource_ids.contains(route.resource_id.as_str()) && !supports(route)
        })
        .map(|route| route.resource_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_management_sdk::{
        ExecutionPlane, ProjectExecutionContext, SandboxProviderKind, WorkspaceExecutionTarget,
        WorkspaceProviderKind,
    };

    fn request() -> CreateRuntimeSessionRequest {
        CreateRuntimeSessionRequest {
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            task_profile: Some("implementation".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            requested_device_id: Some("device-1".to_string()),
            requested_sandbox_provider: None,
            sandbox_target: None,
        }
    }

    fn context() -> ProjectExecutionContext {
        ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: None,
            }),
            sandbox_provider: SandboxProviderKind::None,
            sandbox_pairing_id: None,
            source_type: Some("local_connector".to_string()),
            revision: "revision-1".to_string(),
        }
    }

    #[test]
    fn context_override_must_match_authoritative_device() {
        validate_context_overrides(&request(), &context()).unwrap();
        let mut invalid = request();
        invalid.requested_device_id = Some("another-device".to_string());
        assert!(validate_context_overrides(&invalid, &context()).is_err());
    }

    #[test]
    fn cloud_sandbox_workspace_authorizes_runtime_sandbox_target() {
        let mut request = request();
        request.requested_device_id = None;
        request.requested_sandbox_provider = Some(SandboxProviderKind::Cloud);
        request.sandbox_target = Some(SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        });
        let mut context = context();
        context.workspace_provider = WorkspaceProviderKind::CloudSandbox;
        context.workspace = None;
        validate_context_overrides(&request, &context).unwrap();
    }

    #[test]
    fn only_registered_system_agent_keys_are_accepted() {
        assert_eq!(
            parse_agent_key("task_runner_run_phase").unwrap(),
            SystemAgentKey::TaskRunnerRunPhase
        );
        assert!(parse_agent_key("arbitrary-agent").is_err());
    }

    #[test]
    fn task_process_log_session_requires_exact_run_task_and_agent_scope() {
        let route = system_route(SystemMcpKey::TaskProcessLog);
        let request = request();
        validate_task_runner_provider_context(
            SystemAgentKey::TaskRunnerRunPhase,
            &request,
            &[],
            std::slice::from_ref(&route),
        )
        .expect("bound Task Runner run should be accepted");

        let mut missing_run = request.clone();
        missing_run.run_id = None;
        let error = validate_task_runner_provider_context(
            SystemAgentKey::TaskRunnerRunPhase,
            &missing_run,
            &[],
            std::slice::from_ref(&route),
        )
        .expect_err("run binding is required");
        assert!(format!("{error:?}").contains("run_id"));

        assert!(validate_task_runner_provider_context(
            SystemAgentKey::ChatosConversationAgent,
            &request,
            &[],
            &[route],
        )
        .is_err());
    }

    #[test]
    fn ask_user_route_is_pinned_to_the_agent_host_and_requires_task_run_scope() {
        let mut routes = vec![system_route(SystemMcpKey::AskUser)];
        bind_agent_callback_routes(routes.as_mut_slice(), SystemAgentKey::TaskRunnerRunPhase);
        assert_eq!(routes[0].provider_ref.as_deref(), Some("task-runner"));
        let state = AppState::new(crate::config::AppConfig::test()).expect("test state");
        assert!(state.providers.supports(&routes[0]));

        validate_task_runner_provider_context(
            SystemAgentKey::TaskRunnerRunPhase,
            &request(),
            &[],
            routes.as_slice(),
        )
        .expect("bound Task Runner Ask User route should be accepted");

        let mut missing_task = request();
        missing_task.task_id = None;
        let error = validate_task_runner_provider_context(
            SystemAgentKey::TaskRunnerRunPhase,
            &missing_task,
            &[],
            routes.as_slice(),
        )
        .expect_err("task binding is required");
        assert!(format!("{error:?}").contains("task_id"));

        bind_agent_callback_routes(
            routes.as_mut_slice(),
            SystemAgentKey::ChatosConversationAgent,
        );
        assert_eq!(routes[0].provider_ref.as_deref(), Some("chatos"));
        assert!(state.providers.supports(&routes[0]));

        let mut chatos_request = request();
        chatos_request.agent_key = SystemAgentKey::ChatosConversationAgent.as_str().to_string();
        chatos_request.run_id = None;
        chatos_request.task_id = None;
        chatos_request.turn_id = Some("turn-1".to_string());
        chatos_request.source_session_id = Some("conversation-1".to_string());
        chatos_request.source_user_message_id = Some("message-1".to_string());
        validate_task_runner_provider_context(
            SystemAgentKey::ChatosConversationAgent,
            &chatos_request,
            &[],
            routes.as_slice(),
        )
        .expect("bound ChatOS Ask User route should be accepted");

        chatos_request.turn_id = None;
        let error = validate_task_runner_provider_context(
            SystemAgentKey::ChatosConversationAgent,
            &chatos_request,
            &[],
            routes.as_slice(),
        )
        .expect_err("ChatOS turn binding is required");
        assert!(format!("{error:?}").contains("turn_id"));
    }

    #[test]
    fn task_runner_service_session_requires_chatos_source_scope() {
        let route = system_route(SystemMcpKey::TaskRunnerService);
        let mut request = request();
        request.agent_key = SystemAgentKey::ChatosConversationAgent.as_str().to_string();
        assert!(validate_task_runner_provider_context(
            SystemAgentKey::ChatosConversationAgent,
            &request,
            &[],
            std::slice::from_ref(&route),
        )
        .is_err());

        request.source_session_id = Some("conversation-1".to_string());
        request.source_user_message_id = Some("message-1".to_string());
        validate_task_runner_provider_context(
            SystemAgentKey::ChatosConversationAgent,
            &request,
            &[],
            std::slice::from_ref(&route),
        )
        .expect("complete Chatos source binding should be accepted");

        let error = validate_task_runner_provider_context(
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
            &request,
            &[],
            &[route],
        )
        .expect_err("project execution scope is required");
        assert!(format!("{error:?}").contains("expected_project_task_ids"));
    }

    #[test]
    fn capability_response_must_match_the_requested_identity() {
        let capabilities = chatos_plugin_management_sdk::ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        };
        validate_capability_identity(&capabilities, "task_runner_run_phase", "user-1").unwrap();
        assert!(
            validate_capability_identity(&capabilities, "task_runner_plan_phase", "user-1")
                .is_err()
        );
        assert!(validate_capability_identity(
            &capabilities,
            "task_runner_run_phase",
            "another-user"
        )
        .is_err());
    }

    #[test]
    fn required_route_without_registered_provider_adapter_is_blocked() {
        let required_resource_ids = HashSet::from(["required-mcp"]);
        let routes = vec![chatos_mcp_management_sdk::ResolvedMcpRoute {
            resource_id: "required-mcp".to_string(),
            server_name: "required".to_string(),
            provider_kind: chatos_mcp_management_sdk::McpProviderKind::ExternalHttp,
            provider_ref: Some("mcp-resource:required-mcp".to_string()),
            tool_namespace: "required".to_string(),
            allow_writes: false,
            retry_class: chatos_mcp_management_sdk::McpRetryClass::NoRetry,
            cancel_supported: false,
            reason: "test".to_string(),
        }];
        assert_eq!(
            required_routes_without_provider_adapter(&required_resource_ids, &routes, |_| false),
            vec!["required-mcp"]
        );
    }

    #[test]
    fn cloud_sandbox_routes_are_bound_to_opaque_runtime_target() {
        let mut routes = vec![chatos_mcp_management_sdk::ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::CloudSandbox,
            provider_ref: Some("project:project-1".to_string()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: chatos_mcp_management_sdk::McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }];
        let target = SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        };
        bind_cloud_sandbox_routes(routes.as_mut_slice(), Some(&target));
        assert_eq!(
            routes[0].provider_ref.as_deref(),
            Some("sandbox:sandbox-1/lease:lease-1")
        );
    }

    #[test]
    fn cloud_sandbox_images_are_bound_without_a_runtime_sandbox_target() {
        let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
        let mut context = context();
        context.sandbox_provider = SandboxProviderKind::Cloud;

        bind_cloud_sandbox_routes(routes.as_mut_slice(), None);
        bind_sandbox_image_routes(routes.as_mut_slice(), &context);

        assert_eq!(routes[0].provider_kind, McpProviderKind::CloudSandbox);
        assert_eq!(
            routes[0].provider_ref.as_deref(),
            Some(crate::providers::sandbox_images_cloud_provider_ref())
        );
        assert!(!routes[0].cancel_supported);
    }

    #[test]
    fn local_sandbox_images_are_bound_to_the_authoritative_pairing() {
        let mut routes = vec![sandbox_images_route(McpProviderKind::LocalConnector)];
        let mut context = context();
        context.sandbox_provider = SandboxProviderKind::LocalConnector;
        context.sandbox_pairing_id = Some(" pairing-1 ".to_string());

        bind_sandbox_image_routes(routes.as_mut_slice(), &context);

        assert_eq!(routes[0].provider_kind, McpProviderKind::LocalConnector);
        assert_eq!(
            routes[0].provider_ref.as_deref(),
            Some("sandbox-images:local:pairing-1")
        );
        assert!(!routes[0].cancel_supported);
    }

    #[test]
    fn local_sandbox_images_are_unavailable_without_a_pairing() {
        let mut routes = vec![sandbox_images_route(McpProviderKind::LocalConnector)];
        let mut context = context();
        context.sandbox_provider = SandboxProviderKind::LocalConnector;

        bind_sandbox_image_routes(routes.as_mut_slice(), &context);

        assert_eq!(routes[0].provider_kind, McpProviderKind::Unavailable);
        assert_eq!(routes[0].provider_ref, None);
        assert!(!routes[0].allow_writes);
        assert!(!routes[0].cancel_supported);
    }

    #[test]
    fn cloud_sandbox_binding_does_not_overwrite_sandbox_images() {
        let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
        routes[0].provider_ref =
            Some(crate::providers::sandbox_images_cloud_provider_ref().to_string());
        let target = SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        };

        bind_cloud_sandbox_routes(routes.as_mut_slice(), Some(&target));

        assert_eq!(
            routes[0].provider_ref.as_deref(),
            Some(crate::providers::sandbox_images_cloud_provider_ref())
        );
    }

    fn sandbox_images_route(provider_kind: McpProviderKind) -> ResolvedMcpRoute {
        let mut route = system_route(SystemMcpKey::SandboxImages);
        route.provider_kind = provider_kind;
        route.provider_ref = Some("project:project-1".to_string());
        route.cancel_supported = true;
        route
    }

    fn system_route(key: SystemMcpKey) -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(key);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some(descriptor.owner_service.to_string()),
            tool_namespace: descriptor.server_name.to_string(),
            allow_writes: descriptor.allow_writes,
            retry_class: chatos_mcp_management_sdk::McpRetryClass::NoRetry,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }
}
