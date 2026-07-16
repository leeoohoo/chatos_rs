// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback};

use crate::models::{
    empty_array, empty_object, now_rfc3339, ProjectRecord,
    ProjectRuntimeEnvironmentConfigFileRecord, ProjectRuntimeEnvironmentImageRecord,
    ProjectRuntimeEnvironmentStatus, ProjectRuntimeEnvironmentVariableRecord,
    RuntimeEnvironmentProvider, RuntimeEnvironmentVariableSource,
};
use crate::services::runtime_environment::{
    environment_variable_name_is_secret, normalize_environment_variable_name,
    normalize_environment_variable_records, refresh_environment_variable_record,
    required_environment_variables_are_complete,
};
use crate::state::AppState;

use super::super::runtime_environment::default_runtime_environment_for_project;
use super::PROJECT_ENVIRONMENT_MCP_SERVER_NAME;

#[derive(Clone)]
pub(super) struct ProjectEnvironmentToolProvider {
    pub(super) state: AppState,
    pub(super) project: ProjectRecord,
    pub(super) run_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct UpdateProjectEnvironmentToolArgs {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    analysis_summary: Option<String>,
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
    #[serde(default)]
    last_error: Option<String>,
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
struct ProjectRuntimeEnvironmentImageInput {
    #[serde(default)]
    environment_key: Option<String>,
    #[serde(default)]
    environment_type: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    image_id: Option<String>,
    #[serde(default)]
    image_ref: Option<String>,
    #[serde(default, rename = "image_provider")]
    _image_provider: Option<String>,
    #[serde(default)]
    features: Option<Value>,
    #[serde(default)]
    ports: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
    #[serde(default)]
    dockerfile: Option<String>,
    #[serde(default)]
    custom_build_script: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[async_trait]
impl BuiltinToolProvider for ProjectEnvironmentToolProvider {
    fn server_name(&self) -> &str {
        PROJECT_ENVIRONMENT_MCP_SERVER_NAME
    }

    fn list_tools(&self) -> Vec<Value> {
        chatos_mcp_runtime::project_environment_tool_definitions()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
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
}

impl ProjectEnvironmentToolProvider {
    async fn get_current_project_runtime_environment(&self) -> Result<Value, String> {
        let mut environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));
        crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
        let images = self
            .state
            .store
            .list_project_runtime_environment_images(self.project.id.as_str())
            .await?;
        Ok(mcp_tool_result(
            "当前项目运行环境详情已读取。",
            json!({
                "project": self.project,
                "environment": environment,
                "images": images,
            }),
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

        let requested_status = args
            .status
            .as_deref()
            .map(parse_runtime_environment_status)
            .transpose()?;
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
        let proposes_not_runnable = requested_status
            == Some(ProjectRuntimeEnvironmentStatus::NotRunnable)
            || (requested_status.is_none() && proposed_not_runnable_reason.is_some());
        let proposed_stack = args
            .detected_stack
            .as_ref()
            .unwrap_or(&environment.detected_stack);
        let proposed_services = args
            .required_services
            .as_ref()
            .unwrap_or(&environment.required_services);
        if proposes_not_runnable
            && environment_has_provisionable_evidence(
                proposed_stack,
                proposed_services,
                args.images.as_slice(),
            )
        {
            return Err(
                "not_runnable is rejected because the project contains a provisionable runtime or dependency service. Continue initialization: create/reuse the application runtime image, create local replacements for detected databases/caches/configuration centers, generate connection environment variables, then save ready; use pending_configuration only for irreducible user-supplied business credentials."
                    .to_string(),
            );
        }

        if let Some(value) = args.analysis_summary.and_then(normalize_owned) {
            environment.analysis_summary = Some(value);
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
        let inferred_status = if environment.not_runnable_reason.is_some() {
            ProjectRuntimeEnvironmentStatus::NotRunnable
        } else if args
            .last_error
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            ProjectRuntimeEnvironmentStatus::Failed
        } else {
            ProjectRuntimeEnvironmentStatus::Ready
        };
        environment.status = requested_status.unwrap_or(inferred_status);
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
        environment.last_error = args.last_error.and_then(normalize_owned);
        environment.updated_at = now_rfc3339();

        let mut image_records = Vec::new();
        if environment.status != ProjectRuntimeEnvironmentStatus::NotRunnable {
            for (index, image) in args.images.into_iter().enumerate() {
                image_records.push(image_input_to_record(
                    self.project.id.as_str(),
                    image,
                    index,
                    environment.sandbox_provider,
                ));
            }
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
        if !required_environment_variables_are_complete(&environment.environment_variables) {
            let missing = environment
                .environment_variables
                .iter()
                .filter(|record| {
                    record.required
                        && record
                            .effective_value
                            .as_deref()
                            .is_none_or(|value| value.trim().is_empty())
                })
                .map(|record| record.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            environment.analysis_summary = Some(format!(
                "{} 仍需补充必填运行参数：{}。",
                environment
                    .analysis_summary
                    .as_deref()
                    .unwrap_or("运行环境分析和镜像计划已完成。"),
                missing
            ));
        }

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
            json!({
                "environment": environment,
                "images": images,
            }),
        ))
    }
}

mod compose;
mod support;
#[cfg(test)]
mod tests;

use self::compose::*;
use self::support::*;
