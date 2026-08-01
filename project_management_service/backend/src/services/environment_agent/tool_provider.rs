// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback};
use chatos_mcp_service::{McpRequestContext, McpToolProvider};

use crate::models::{
    empty_array, empty_object, now_rfc3339, ProgramManagedMcpPolicy, ProjectRecord,
    ProjectRuntimeEnvironmentConfigFileRecord, ProjectRuntimeEnvironmentImageRecord,
    ProjectRuntimeEnvironmentStatus, ProjectRuntimeEnvironmentVariableRecord,
    RuntimeEnvironmentProvider, RuntimeEnvironmentVariableSource, RuntimeServiceRole,
};
use crate::services::runtime_environment::{
    enforce_project_runtime_boundary, environment_variable_name_is_secret,
    normalize_environment_variable_name, normalize_environment_variable_records,
    program_generated_runtime_analysis_summary, refresh_environment_variable_record,
    required_environment_variables_are_complete, runtime_image_is_execution_required,
};
use crate::state::AppState;

use super::super::runtime_environment::default_runtime_environment_for_project;

#[derive(Clone)]
pub(crate) struct ProjectEnvironmentToolProvider {
    state: AppState,
    project: ProjectRecord,
    run_id: String,
    selected_dependencies: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateProjectEnvironmentToolArgs {
    #[serde(default)]
    not_runnable_reason: Option<String>,
    #[serde(default)]
    detected_stack: Option<Value>,
    #[serde(default)]
    required_services: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
    #[serde(default)]
    environment_variables: Vec<ProjectRuntimeEnvironmentVariableInput>,
    #[serde(default)]
    environment_variable_scan: Option<ProjectEnvironmentVariableScanInput>,
    #[serde(default)]
    generated_config_files: Option<Vec<ProjectRuntimeEnvironmentConfigFileInput>>,
    #[serde(default)]
    images: Vec<ProjectRuntimeEnvironmentImageInput>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ProjectEnvironmentVariableScanInput {
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    files_scanned: Vec<String>,
    #[serde(default)]
    reference_count: usize,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectRuntimeEnvironmentVariableInput {
    name: String,
    #[serde(default)]
    project_value: Option<String>,
    #[serde(default)]
    project_value_suitable: Option<bool>,
    #[serde(default)]
    recommended_value: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    recommendation_reason: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    secret: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectRuntimeEnvironmentConfigFileInput {
    path: String,
    #[serde(default)]
    format: Option<String>,
    content: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    source_files: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRuntimeEnvironmentImageInput {
    #[serde(default)]
    environment_key: Option<String>,
    #[serde(default)]
    environment_type: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    source_root: Option<String>,
    #[serde(default)]
    component_kind: Option<String>,
    #[serde(default)]
    startup_command: Option<String>,
    #[serde(default)]
    test_command: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    auto_start: bool,
    #[serde(default)]
    features: Option<Value>,
    #[serde(default)]
    ports: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
    #[serde(default)]
    dockerfile: Option<String>,
}

#[async_trait]
impl BuiltinToolProvider for ProjectEnvironmentToolProvider {
    fn server_name(&self) -> &str {
        chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectEnvironment,
        )
        .server_name
    }

    fn list_tools(&self) -> Vec<Value> {
        chatos_mcp::system_mcp_static_tools(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectEnvironment,
        )
        .expect("Project Environment must have a static system MCP catalog")
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.call_tool_inner(name, args).await
    }
}

#[async_trait]
impl McpToolProvider for ProjectEnvironmentToolProvider {
    fn server_name(&self) -> &str {
        chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectEnvironment,
        )
        .server_name
    }

    fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
        chatos_mcp::system_mcp_static_tools(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectEnvironment,
        )
        .expect("Project Environment must have a static system MCP catalog")
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: McpRequestContext,
    ) -> Result<Value, String> {
        self.call_tool_inner(name, args).await
    }
}

impl ProjectEnvironmentToolProvider {
    pub(crate) fn new(
        state: AppState,
        project: ProjectRecord,
        run_id: impl Into<String>,
        selected_dependencies: Vec<String>,
    ) -> Self {
        Self {
            state,
            project,
            run_id: run_id.into(),
            selected_dependencies,
        }
    }

    async fn call_tool_inner(&self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "get_current_project_runtime_environment" => {
                self.get_current_project_runtime_environment().await
            }
            "update_current_project_runtime_environment" => {
                self.update_current_project_runtime_environment(args).await
            }
            other => Err(format!("unknown project environment tool: {other}")),
        }
    }

    async fn get_current_project_runtime_environment(&self) -> Result<Value, String> {
        let mut environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));
        ensure_project_environment_agent_run(&environment, self.run_id.as_str())?;
        crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
        let images = self
            .state
            .store
            .list_project_runtime_environment_images(self.project.id.as_str())
            .await?;
        Ok(mcp_tool_result(
            "当前项目运行环境详情已读取。",
            agent_visible_runtime_state(&self.project, &environment, images.as_slice()),
        ))
    }

    async fn update_current_project_runtime_environment(
        &self,
        args: Value,
    ) -> Result<Value, String> {
        let args: UpdateProjectEnvironmentToolArgs = serde_json::from_value(args)
            .map_err(|err| format!("invalid project environment update args: {err}"))?;
        let mut environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));
        ensure_project_environment_agent_run(&environment, self.run_id.as_str())?;

        let environment_variable_scan =
            require_completed_environment_variable_scan(args.environment_variable_scan.clone())?;
        let generated_config_files =
            normalize_generated_config_files(args.generated_config_files.ok_or_else(|| {
                "generated_config_files must be provided before saving the runtime environment"
                    .to_string()
            })?)?;
        let proposed_not_runnable_reason = args
            .not_runnable_reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let proposes_not_runnable = proposed_not_runnable_reason.is_some();
        let proposed_stack = args
            .detected_stack
            .as_ref()
            .unwrap_or(&environment.detected_stack);
        let proposed_services = args
            .required_services
            .as_ref()
            .unwrap_or(&environment.required_services);
        let selected_service_kinds =
            selected_dependency_service_kinds(self.selected_dependencies.as_slice());
        if proposes_not_runnable
            && (!selected_service_kinds.is_empty()
                || environment_has_provisionable_evidence(
                    proposed_stack,
                    proposed_services,
                    args.images.as_slice(),
                ))
        {
            return Err(
                "not_runnable is rejected because the project contains a provisionable runtime or dependency service. Continue initialization: create/reuse the application runtime image, create local replacements for detected databases/caches/configuration centers, generate connection environment variables, then save ready; use pending_configuration only for irreducible user-supplied business credentials."
                    .to_string(),
            );
        }

        environment.not_runnable_reason = args.not_runnable_reason.and_then(normalize_owned);
        if let Some(value) = args.detected_stack {
            environment.detected_stack = ensure_object(value);
        }
        let detected_stack = environment
            .detected_stack
            .as_object_mut()
            .expect("ensure_object always returns an object");
        detected_stack.insert(
            "selected_dependencies".to_string(),
            json!(self.selected_dependencies.clone()),
        );
        detected_stack.insert(
            "environment_variable_scan".to_string(),
            json!({
                "completed": true,
                "files_scanned": environment_variable_scan.files_scanned,
                "reference_count": environment_variable_scan.reference_count,
                "summary": environment_variable_scan.summary,
            }),
        );
        if let Some(value) = args.required_services {
            environment.required_services = ensure_array(value);
        }
        let inferred_service_kinds = infer_service_kinds_from_environment_variables(
            &environment.environment_variables,
            &args.environment_variables,
            args.env_vars.as_ref(),
        );
        ensure_required_service_records(&mut environment.required_services, inferred_service_kinds);
        ensure_selected_service_records(
            &mut environment.required_services,
            selected_service_kinds.clone(),
        );
        environment.status = if environment.not_runnable_reason.is_some() {
            ProjectRuntimeEnvironmentStatus::NotRunnable
        } else {
            ProjectRuntimeEnvironmentStatus::Ready
        };
        if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable {
            environment.required_services = empty_array();
        } else {
            environment.not_runnable_reason = None;
        }
        environment.environment_variables = merge_environment_variable_records(
            &environment,
            args.environment_variables,
            args.env_vars.as_ref(),
        );
        environment.env_vars =
            crate::services::runtime_environment::effective_environment_variables(
                &environment.environment_variables,
            );
        environment.last_agent_run_id = Some(self.run_id.clone());
        environment.last_error = None;
        environment.updated_at = now_rfc3339();

        let mut image_records = Vec::new();
        if environment.status != ProjectRuntimeEnvironmentStatus::NotRunnable {
            for (index, image) in args.images.into_iter().enumerate() {
                image_records.push(image_input_to_record(
                    self.project.id.as_str(),
                    image,
                    index,
                    environment.sandbox_provider,
                    None,
                )?);
            }
            ensure_selected_dependency_image_records(
                self.project.id.as_str(),
                environment.sandbox_provider,
                selected_service_kinds,
                &mut image_records,
            )?;
            enforce_project_runtime_boundary(&self.project, &mut environment, &mut image_records);
        } else {
            environment.execution_service_id = None;
        }
        environment.generated_config_files = generated_config_files;
        if !matches!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::NotRunnable | ProjectRuntimeEnvironmentStatus::Failed
        ) {
            validate_environment_image_plans(
                &environment.detected_stack,
                &environment.required_services,
                image_records.as_slice(),
            )?;
            upsert_project_compose_config_file(
                self.project.id.as_str(),
                &mut environment.generated_config_files,
                &environment.environment_variables,
                &environment.required_services,
                image_records.as_slice(),
            )?;
            if image_records
                .iter()
                .filter(|image| runtime_image_is_execution_required(image))
                .any(|image| !image_is_real_and_ready(image))
            {
                environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
            } else if !required_environment_variables_are_complete(
                &environment.environment_variables,
            ) {
                environment.status = ProjectRuntimeEnvironmentStatus::PendingConfiguration;
            } else {
                environment.status = ProjectRuntimeEnvironmentStatus::Ready;
            }
        }
        environment.analysis_summary = Some(program_generated_runtime_analysis_summary(
            &environment,
            image_records.as_slice(),
        ));

        let environment = self
            .state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        let images = self
            .state
            .store
            .replace_project_runtime_environment_images(
                self.project.id.as_str(),
                image_records.as_slice(),
            )
            .await?;
        Ok(mcp_tool_result(
            "当前项目运行环境初始化结果已保存。",
            agent_visible_runtime_state(&self.project, &environment, images.as_slice()),
        ))
    }
}

pub(crate) fn ensure_project_environment_agent_run(
    environment: &crate::models::ProjectRuntimeEnvironmentRecord,
    run_id: &str,
) -> Result<(), String> {
    if environment.status != ProjectRuntimeEnvironmentStatus::Analyzing {
        return Err("project environment analysis is no longer active".to_string());
    }
    if environment.last_agent_run_id.as_deref().map(str::trim) != Some(run_id.trim()) {
        return Err("project environment analysis run has changed".to_string());
    }
    Ok(())
}

fn selected_dependency_service_kinds(
    selected_dependencies: &[String],
) -> std::collections::BTreeSet<String> {
    let mut kinds = std::collections::BTreeSet::new();
    for dependency in selected_dependencies {
        infer_service_kinds_from_text(dependency, &mut kinds);
    }
    kinds
}

fn ensure_selected_dependency_image_records(
    project_id: &str,
    provider: RuntimeEnvironmentProvider,
    selected_service_kinds: std::collections::BTreeSet<String>,
    images: &mut Vec<ProjectRuntimeEnvironmentImageRecord>,
) -> Result<(), String> {
    for service in selected_service_kinds {
        if images
            .iter()
            .any(|image| image_matches_service(image, service.as_str()))
        {
            continue;
        }
        let index = images.len();
        images.push(image_input_to_record(
            project_id,
            ProjectRuntimeEnvironmentImageInput {
                environment_key: Some(service.clone()),
                environment_type: Some("service".to_string()),
                display_name: Some(dependency_service_display_name(service.as_str()).to_string()),
                ..ProjectRuntimeEnvironmentImageInput::default()
            },
            index,
            provider,
            None,
        )?);
    }
    Ok(())
}

fn dependency_service_display_name(service: &str) -> &str {
    match service {
        "postgres" => "PostgreSQL",
        "mysql" => "MySQL / MariaDB",
        "mongodb" => "MongoDB",
        "redis" => "Redis-compatible cache",
        "nacos" => "Nacos",
        "rabbitmq" => "RabbitMQ",
        "kafka" => "Apache Kafka-compatible broker",
        "elasticsearch" => "Elasticsearch-compatible search",
        "minio" => "MinIO / S3-compatible storage",
        other => other,
    }
}

fn agent_visible_runtime_state(
    project: &ProjectRecord,
    environment: &crate::models::ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Value {
    json!({
        "project": {
            "id": project.id,
            "name": project.name,
        },
        "analysis": {
            "not_runnable_reason": environment.not_runnable_reason,
            "execution_service_id": environment.execution_service_id,
            "detected_stack": environment.detected_stack,
            "required_services": environment.required_services,
            "environment_variables": environment.environment_variables.iter().map(|record| json!({
                "name": record.name,
                "project_value": (!record.secret).then_some(record.project_value.as_deref()).flatten(),
                "project_value_present": record.project_value.is_some(),
                "project_value_suitable": record.project_value_suitable,
                "recommended_value": (!record.secret).then_some(record.recommended_value.as_deref()).flatten(),
                "recommended_value_present": record.recommended_value.is_some(),
                "description": record.description,
                "recommendation_reason": record.recommendation_reason,
                "required": record.required,
                "secret": record.secret,
            })).collect::<Vec<_>>(),
            "generated_config_files": environment.generated_config_files,
        },
        "images": images.iter().map(|image| json!({
            "environment_key": image.environment_key,
            "environment_type": image.environment_type,
            "display_name": image.display_name,
            "features": image.features,
            "ports": image.ports,
            "env_vars": image.env_vars,
            "dockerfile": image.dockerfile,
        })).collect::<Vec<_>>(),
    })
}

pub(super) mod compose;
mod support;
#[cfg(test)]
mod tests;

use self::compose::*;
use self::support::*;
