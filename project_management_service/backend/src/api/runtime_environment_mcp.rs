// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chatos_mcp_service::{McpRequestContext, McpToolProvider};
use serde_json::{json, Value};

use super::internal_auth::{
    require_project_internal_request, PROJECT_READ_SCOPE, PROJECT_SERVICE_CALLER,
    TASK_RUNNER_CALLER,
};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse};
use crate::services::runtime_environment::{
    apply_program_managed_image_policy, default_runtime_environment_for_project,
    refresh_environment_variable_values,
};
use crate::state::AppState;

const TOOL_NAME: &str = "get_project_runtime_environment_info";
const TASK_RUNNER_PROJECT_ID_HEADER: &str = "x-task-runner-project-id";

pub(in crate::api) async fn project_runtime_environment_mcp_entrypoint(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(err) = require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER, PROJECT_SERVICE_CALLER],
        PROJECT_READ_SCOPE,
    ) {
        return Json(mcp_server::jsonrpc_error_response(
            err.status,
            id,
            err.message,
        ));
    }
    if let Err(message) = ensure_project_header_matches(&headers, project_id.as_str()) {
        return Json(mcp_server::jsonrpc_error_response(
            StatusCode::FORBIDDEN,
            id,
            message,
        ));
    }
    Json(handle_jsonrpc(state, project_id, request).await)
}

async fn handle_jsonrpc(
    state: AppState,
    project_id: String,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let descriptor = chatos_mcp::system_mcp_descriptor(
        chatos_plugin_management_sdk::SystemMcpKey::ProjectRuntimeEnvironment,
    );
    mcp_server::handle_provider_jsonrpc(
        descriptor.server_name,
        request,
        Arc::new(ProjectRuntimeEnvironmentMcpProvider { state, project_id }),
    )
    .await
}

#[derive(Clone)]
struct ProjectRuntimeEnvironmentMcpProvider {
    state: AppState,
    project_id: String,
}

#[async_trait]
impl McpToolProvider for ProjectRuntimeEnvironmentMcpProvider {
    fn server_name(&self) -> &str {
        chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectRuntimeEnvironment,
        )
        .server_name
    }

    fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
        chatos_mcp::system_mcp_static_tools(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectRuntimeEnvironment,
        )
        .expect("Project Runtime Environment must have a static system MCP catalog")
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: McpRequestContext,
    ) -> Result<Value, String> {
        call_tool(
            &self.state,
            self.project_id.as_str(),
            Some(json!({
                "name": name,
                "arguments": args,
            })),
        )
        .await
    }
}

async fn call_tool(
    state: &AppState,
    project_id: &str,
    params: Option<Value>,
) -> Result<Value, String> {
    let params = params.unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "tools/call params.name is required".to_string())?;
    if name != TOOL_NAME {
        return Err(format!("Tool not found: {name}"));
    }
    let project = state
        .store
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    let mut environment = state
        .store
        .get_project_runtime_environment(project_id)
        .await?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(project_id)
        .await?;
    for image in &mut images {
        apply_program_managed_image_policy(image);
    }
    Ok(tool_result(compact_runtime_environment_payload(
        project_id,
        &environment,
        &images,
    )))
}

fn compact_runtime_environment_payload(
    project_id: &str,
    environment: &crate::models::ProjectRuntimeEnvironmentRecord,
    images: &[crate::models::ProjectRuntimeEnvironmentImageRecord],
) -> Value {
    let detected_stack = json!({
        "project_type": environment.detected_stack.get("project_type"),
        "selected_dependencies": environment.detected_stack.get("selected_dependencies"),
        "components": environment
            .detected_stack
            .get("components")
            .and_then(Value::as_array)
            .map(|components| components.iter().map(compact_component).collect::<Vec<_>>())
            .unwrap_or_default(),
    });
    let required_services = environment
        .required_services
        .as_array()
        .map(|services| {
            services
                .iter()
                .map(|service| {
                    json!({
                        "environment_key": service.get("environment_key"),
                        "service_type": service.get("service_type"),
                        "display_name": service.get("display_name"),
                        "version": service.get("version"),
                        "required": service.get("required"),
                        "ports": service.get("ports"),
                        "reason": service.get("reason"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let environment_variables = environment
        .environment_variables
        .iter()
        .map(|variable| {
            let effective_value = (!variable.secret)
                .then(|| variable.effective_value.clone())
                .flatten();
            json!({
                "name": variable.name,
                "effective_value": effective_value,
                "effective_source": variable.effective_source,
                "configured": variable
                    .effective_value
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "required": variable.required,
                "secret": variable.secret,
                "description": variable.description,
            })
        })
        .collect::<Vec<_>>();
    let config_files = environment
        .generated_config_files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "format": file.format,
                "description": file.description,
                "source_files": file.source_files,
            })
        })
        .collect::<Vec<_>>();
    let images = images
        .iter()
        .map(|image| {
            json!({
                "id": image.id,
                "environment_key": image.environment_key,
                "environment_type": image.environment_type,
                "display_name": image.display_name,
                "service_id": image.service_id,
                "service_role": image.service_role,
                "source_root": image.source_root,
                "component_kind": image.component_kind,
                "startup_command": image.startup_command,
                "test_command": image.test_command,
                "depends_on": image.depends_on,
                "auto_start": image.auto_start,
                "mcp_policy": image.mcp_policy,
                "image_id": image.image_id,
                "image_ref": image.image_ref,
                "image_provider": image.image_provider,
                "features": image.features,
                "ports": image.ports,
                "status": image.status,
                "error": image.error,
                "updated_at": image.updated_at,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "project_id": project_id,
        "environment": {
            "status": environment.status,
            "sandbox_enabled": environment.sandbox_enabled,
            "sandbox_provider": environment.sandbox_provider,
            "file_provider": environment.file_provider,
            "analysis_summary": environment.analysis_summary,
            "not_runnable_reason": environment.not_runnable_reason,
            "execution_service_id": environment.execution_service_id,
            "detected_stack": detected_stack,
            "required_services": required_services,
            "environment_variables": environment_variables,
            "generated_config_files": config_files,
            "last_error": environment.last_error,
            "updated_at": environment.updated_at,
        },
        "images": images,
    })
}

fn compact_component(component: &Value) -> Value {
    json!({
        "name": component.get("name"),
        "root": component.get("root"),
        "runtime": component.get("runtime"),
        "entrypoint": component.get("entrypoint"),
        "default_port": component.get("default_port"),
        "storage": component.get("storage"),
    })
}

fn tool_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "_structured_result": payload,
        "isError": false
    })
}

fn ensure_project_header_matches(headers: &HeaderMap, project_id: &str) -> Result<(), String> {
    let Some(header_project_id) = headers
        .get(TASK_RUNNER_PROJECT_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if header_project_id == project_id.trim() {
        Ok(())
    } else {
        Err("x-task-runner-project-id does not match request project id".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ProgramManagedMcpPolicy, ProjectRuntimeEnvironmentConfigFileRecord,
        ProjectRuntimeEnvironmentImageRecord, ProjectRuntimeEnvironmentRecord,
        ProjectRuntimeEnvironmentStatus, ProjectRuntimeEnvironmentVariableRecord,
        RuntimeEnvironmentProvider, RuntimeEnvironmentVariableSource, RuntimeServiceRole,
    };
    use axum::http::HeaderValue;

    #[test]
    fn task_runner_project_header_must_match_route() {
        let mut headers = HeaderMap::new();
        headers.insert(
            TASK_RUNNER_PROJECT_ID_HEADER,
            HeaderValue::from_static("project-1"),
        );
        assert!(ensure_project_header_matches(&headers, "project-1").is_ok());
        assert!(ensure_project_header_matches(&headers, "project-2").is_err());
    }

    #[test]
    fn task_runner_environment_payload_is_compact_and_redacts_secret_values() {
        let environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::Ready,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: Some("ready".to_string()),
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            detected_stack: json!({
                "project_type": "python",
                "selected_dependencies": ["PostgreSQL"],
                "components": [{
                    "name": "api",
                    "root": "backend",
                    "runtime": "python",
                    "entrypoint": "python -m api",
                    "default_port": 8080,
                    "evidence": ["very large evidence that should not be returned"]
                }],
                "environment_variable_scan": {"summary": "large scan output"}
            }),
            required_services: json!([{
                "environment_key": "postgresql",
                "service_type": "postgresql",
                "display_name": "PostgreSQL",
                "required": true,
                "ports": [{"container_port": 5432}],
                "env_vars": {"POSTGRES_PASSWORD": "secret-service-value"}
            }]),
            env_vars: json!({"DATABASE_URL": "super-secret-dsn"}),
            environment_variables: vec![
                ProjectRuntimeEnvironmentVariableRecord {
                    name: "DATABASE_URL".to_string(),
                    project_value: None,
                    project_value_suitable: false,
                    recommended_value: Some("super-secret-dsn".to_string()),
                    user_value: None,
                    effective_value: Some("super-secret-dsn".to_string()),
                    effective_source: RuntimeEnvironmentVariableSource::AiRecommended,
                    description: Some("database connection".to_string()),
                    recommendation_reason: None,
                    required: true,
                    secret: true,
                },
                ProjectRuntimeEnvironmentVariableRecord {
                    name: "POSTGRES_HOST".to_string(),
                    project_value: None,
                    project_value_suitable: false,
                    recommended_value: None,
                    user_value: None,
                    effective_value: Some("postgresql".to_string()),
                    effective_source: RuntimeEnvironmentVariableSource::AiRecommended,
                    description: Some("database host".to_string()),
                    recommendation_reason: None,
                    required: true,
                    secret: false,
                },
            ],
            generated_config_files: vec![ProjectRuntimeEnvironmentConfigFileRecord {
                path: ".chatos/runtime/docker-compose.yml".to_string(),
                format: "yaml".to_string(),
                content: "large generated compose content with secret-service-value".to_string(),
                description: Some("runtime compose".to_string()),
                source_files: vec!["backend/app.py".to_string()],
            }],
            last_agent_run_id: Some("run-1".to_string()),
            last_error: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let images = vec![ProjectRuntimeEnvironmentImageRecord {
            id: "image-1".to_string(),
            project_id: "project-1".to_string(),
            environment_key: "postgresql".to_string(),
            environment_type: "service".to_string(),
            display_name: "PostgreSQL".to_string(),
            service_id: "postgresql".to_string(),
            service_role: RuntimeServiceRole::Dependency,
            source_root: ".".to_string(),
            component_kind: String::new(),
            startup_command: None,
            test_command: None,
            depends_on: Vec::new(),
            auto_start: false,
            mcp_policy: ProgramManagedMcpPolicy::default(),
            image_id: None,
            image_ref: Some("postgres:16-alpine".to_string()),
            image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            features: json!(["postgresql"]),
            ports: json!([{"container_port": 5432}]),
            env_vars: json!({"POSTGRES_PASSWORD": "secret-image-value"}),
            dockerfile: Some("FROM postgres\nENV PASSWORD=secret-image-value".to_string()),
            custom_build_script: Some("echo secret-image-value".to_string()),
            status: "ready".to_string(),
            error: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }];

        let payload = compact_runtime_environment_payload("project-1", &environment, &images);
        let text = payload.to_string();

        assert!(!text.contains("super-secret-dsn"));
        assert!(!text.contains("secret-service-value"));
        assert!(!text.contains("secret-image-value"));
        assert!(!text.contains("large scan output"));
        assert!(!text.contains("very large evidence"));
        assert_eq!(
            payload.pointer("/environment/environment_variables/0/configured"),
            Some(&json!(true))
        );
        assert_eq!(
            payload.pointer("/environment/environment_variables/0/effective_value"),
            Some(&Value::Null)
        );
        assert_eq!(
            payload.pointer("/environment/environment_variables/1/effective_value"),
            Some(&json!("postgresql"))
        );
        assert_eq!(
            payload.pointer("/images/0/image_ref"),
            Some(&json!("postgres:16-alpine"))
        );
        assert_eq!(
            payload.pointer("/environment/generated_config_files/0/path"),
            Some(&json!(".chatos/runtime/docker-compose.yml"))
        );
    }
}
