// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use super::internal_auth::{
    require_project_internal_request, MCP_MANAGEMENT_CALLER, PROJECT_ENVIRONMENT_SCOPE,
};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::models::{ProjectRecord, ProjectRuntimeEnvironmentRecord};
use crate::services::environment_agent::{
    ensure_project_environment_agent_run, ProjectEnvironmentToolProvider,
};
use crate::state::AppState;

const PROJECT_ID_HEADER: &str = "x-mcp-management-project-id";
const OWNER_USER_ID_HEADER: &str = "x-mcp-management-owner-user-id";
const AGENT_KEY_HEADER: &str = "x-mcp-management-agent-key";
const SESSION_ID_HEADER: &str = "x-mcp-management-session-id";
const RUN_ID_HEADER: &str = "x-mcp-management-run-id";

pub(in crate::api) async fn project_environment_agent_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(err) = require_project_internal_request(
        &state.config,
        &headers,
        &[MCP_MANAGEMENT_CALLER],
        PROJECT_ENVIRONMENT_SCOPE,
    ) {
        return Json(mcp_server::jsonrpc_error_response(
            err.status,
            id,
            err.message,
        ));
    }
    let result = resolve_runtime_binding(&state, &headers, project_id.as_str()).await;
    let (project, run_id, selected_dependencies) = match result {
        Ok(binding) => binding,
        Err(message) => {
            return Json(mcp_server::jsonrpc_error_response(
                StatusCode::FORBIDDEN,
                id,
                message,
            ));
        }
    };
    let descriptor = chatos_mcp::system_mcp_descriptor(
        chatos_plugin_management_sdk::SystemMcpKey::ProjectEnvironment,
    );
    Json(
        mcp_server::handle_provider_jsonrpc(
            descriptor.server_name,
            request,
            Arc::new(ProjectEnvironmentToolProvider::new(
                state,
                project,
                run_id,
                selected_dependencies,
            )),
        )
        .await,
    )
}

async fn resolve_runtime_binding(
    state: &AppState,
    headers: &HeaderMap,
    project_id: &str,
) -> Result<(ProjectRecord, String, Vec<String>), String> {
    require_matching_header(headers, PROJECT_ID_HEADER, project_id)?;
    let _session_id = required_header(headers, SESSION_ID_HEADER)?;
    let owner_user_id = required_header(headers, OWNER_USER_ID_HEADER)?;
    let run_id = required_header(headers, RUN_ID_HEADER)?.to_string();
    let project = state
        .store
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    let expected_agent_key = if matches!(
        project.source_type,
        crate::models::ProjectSourceType::Local | crate::models::ProjectSourceType::LocalConnector
    ) {
        SystemAgentKey::ProjectManagementLocalAgent.as_str()
    } else {
        SystemAgentKey::ProjectManagementAgent.as_str()
    };
    require_matching_header(headers, AGENT_KEY_HEADER, expected_agent_key)?;
    if project.owner_user_id.as_deref().map(str::trim) != Some(owner_user_id) {
        return Err("runtime session owner does not match project owner".to_string());
    }
    let environment = state
        .store
        .get_project_runtime_environment(project_id)
        .await?
        .ok_or_else(|| "project runtime environment is not initialized".to_string())?;
    ensure_project_environment_agent_run(&environment, run_id.as_str())?;
    Ok((
        project,
        run_id,
        selected_dependencies_from_environment(&environment),
    ))
}

fn selected_dependencies_from_environment(
    environment: &ProjectRuntimeEnvironmentRecord,
) -> Vec<String> {
    let mut dependencies = environment
        .detected_stack
        .get("selected_dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn require_matching_header(
    headers: &HeaderMap,
    name: &'static str,
    expected: &str,
) -> Result<(), String> {
    let actual = required_header(headers, name)?;
    if actual == expected.trim() {
        Ok(())
    } else {
        Err(format!("{name} does not match the bound runtime context"))
    }
}

fn required_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use serde_json::json;

    use super::*;

    #[test]
    fn runtime_binding_headers_are_fail_closed() {
        let mut headers = HeaderMap::new();
        headers.insert(PROJECT_ID_HEADER, HeaderValue::from_static("project-1"));
        headers.insert(
            AGENT_KEY_HEADER,
            HeaderValue::from_static("project_management_agent"),
        );
        assert!(require_matching_header(&headers, PROJECT_ID_HEADER, "project-1").is_ok());
        assert!(require_matching_header(&headers, PROJECT_ID_HEADER, "project-2").is_err());
        assert!(required_header(&headers, SESSION_ID_HEADER).is_err());
    }

    #[test]
    fn selected_dependencies_are_normalized_from_the_bound_environment() {
        let mut environment = test_environment();
        environment.detected_stack = json!({
            "selected_dependencies": [" Redis ", "PostgreSQL", "Redis", ""]
        });
        assert_eq!(
            selected_dependencies_from_environment(&environment),
            vec!["PostgreSQL", "Redis"]
        );
    }

    #[test]
    fn completed_or_replaced_analysis_runs_cannot_reuse_the_agent_provider() {
        let mut environment = test_environment();
        assert!(ensure_project_environment_agent_run(&environment, "run-1").is_ok());
        environment.status = crate::models::ProjectRuntimeEnvironmentStatus::Ready;
        assert!(ensure_project_environment_agent_run(&environment, "run-1").is_err());
        environment.status = crate::models::ProjectRuntimeEnvironmentStatus::Analyzing;
        assert!(ensure_project_environment_agent_run(&environment, "run-2").is_err());
    }

    fn test_environment() -> ProjectRuntimeEnvironmentRecord {
        ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: crate::models::ProjectRuntimeEnvironmentStatus::Analyzing,
            sandbox_enabled: true,
            sandbox_provider: crate::models::RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: crate::models::RuntimeEnvironmentProvider::Harness,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: json!({}),
            required_services: json!([]),
            env_vars: json!({}),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: Some("run-1".to_string()),
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }
}
