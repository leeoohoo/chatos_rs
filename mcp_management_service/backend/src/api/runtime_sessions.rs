// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, RuntimeSessionResponse, RuntimeSessionRoutesResponse,
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
    let capabilities = state
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
    let materialized = materialize_mcp_candidates(&capabilities);
    let route_response =
        state
            .routing
            .resolve(chatos_mcp_management_sdk::ResolveMcpRoutesRequest {
                context: project_context.clone(),
                resources: materialized.resources,
            });
    let tool_result = materialize_runtime_tools(&capabilities, route_response.routes.as_slice())
        .map_err(ApiError::conflict)?;
    let route_revision = runtime_route_revision(
        route_response.route_revision.as_str(),
        tool_result.tools.as_slice(),
    )
    .map_err(ApiError::internal)?;
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
    let session_id = format!("mcp_session_{}", Uuid::new_v4().simple());
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
        policy_revision: capabilities.policy_revision.clone(),
        route_revision: route_revision.clone(),
        allowed_resource_ids,
        iat: 0,
        exp: 0,
    };
    let grant = state
        .runtime_grants
        .issue(claims)
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
        project_context,
        policy_revision: capabilities.policy_revision.clone(),
        route_revision: route_revision.clone(),
        routes: route_response.routes,
        tools: tool_result.tools,
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
        if requested != context.sandbox_provider {
            return Err(ApiError::conflict(
                "sandbox provider override is not authorized by Project Context",
            ));
        }
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
            requested_device_id: Some("device-1".to_string()),
            requested_sandbox_provider: None,
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
    fn only_registered_system_agent_keys_are_accepted() {
        assert_eq!(
            parse_agent_key("task_runner_run_phase").unwrap(),
            SystemAgentKey::TaskRunnerRunPhase
        );
        assert!(parse_agent_key("arbitrary-agent").is_err());
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
}
