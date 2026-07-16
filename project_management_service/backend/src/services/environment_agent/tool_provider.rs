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

fn merge_environment_variable_records(
    environment: &crate::models::ProjectRuntimeEnvironmentRecord,
    inputs: Vec<ProjectRuntimeEnvironmentVariableInput>,
    legacy_agent_env_vars: Option<&Value>,
) -> Vec<ProjectRuntimeEnvironmentVariableRecord> {
    let existing = normalize_environment_variable_records(
        environment.environment_variables.clone(),
        &environment.env_vars,
    );
    let existing_by_name = existing
        .iter()
        .map(|record| (record.name.clone(), record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut generated_base = serde_json::Map::new();
    for record in &existing {
        if let Some(value) = record
            .recommended_value
            .as_ref()
            .or(record.project_value.as_ref())
        {
            generated_base.insert(record.name.clone(), Value::String(value.clone()));
        }
    }
    if let Some(agent_values) = legacy_agent_env_vars.and_then(Value::as_object) {
        for (name, value) in agent_values {
            generated_base.insert(name.clone(), value.clone());
        }
    }
    let generated = generated_environment_variables(
        &environment.required_services,
        Some(&Value::Object(generated_base)),
    );
    let mut by_name = std::collections::BTreeMap::new();
    for input in inputs {
        let Some(name) = normalize_environment_variable_name(input.name.as_str()) else {
            continue;
        };
        let previous = existing_by_name.get(name.as_str()).copied();
        let mut record = ProjectRuntimeEnvironmentVariableRecord {
            name: name.clone(),
            project_value: normalize_env_value(input.project_value),
            project_value_suitable: input.project_value_suitable.unwrap_or(true),
            recommended_value: normalize_env_value(input.recommended_value),
            user_value: previous.and_then(|record| record.user_value.clone()),
            effective_value: None,
            effective_source: RuntimeEnvironmentVariableSource::None,
            description: input.description.and_then(normalize_owned),
            recommendation_reason: input.recommendation_reason.and_then(normalize_owned),
            required: input.required,
            secret: input.secret || environment_variable_name_is_secret(name.as_str()),
        };
        refresh_environment_variable_record(&mut record);
        by_name.insert(name, record);
    }
    if let Some(generated) = generated.as_object() {
        for (raw_name, value) in generated {
            let Some(name) = normalize_environment_variable_name(raw_name) else {
                continue;
            };
            let Some(value) = env_value_to_string(value) else {
                continue;
            };
            if let Some(record) = by_name.get_mut(name.as_str()) {
                if record.recommended_value.is_none()
                    && (!record.project_value_suitable || record.project_value.is_none())
                {
                    record.recommended_value = Some(value);
                    refresh_environment_variable_record(record);
                }
                continue;
            }
            let previous = existing_by_name.get(name.as_str()).copied();
            let mut record = ProjectRuntimeEnvironmentVariableRecord {
                name: name.clone(),
                project_value: previous.and_then(|record| record.project_value.clone()),
                project_value_suitable: previous
                    .map(|record| record.project_value_suitable)
                    .unwrap_or(false),
                recommended_value: Some(value),
                user_value: previous.and_then(|record| record.user_value.clone()),
                effective_value: None,
                effective_source: RuntimeEnvironmentVariableSource::None,
                description: previous.and_then(|record| record.description.clone()),
                recommendation_reason: previous
                    .and_then(|record| record.recommendation_reason.clone())
                    .or_else(|| Some("根据当前沙箱运行环境生成".to_string())),
                required: previous.is_some_and(|record| record.required),
                secret: previous.is_some_and(|record| record.secret)
                    || environment_variable_name_is_secret(name.as_str()),
            };
            refresh_environment_variable_record(&mut record);
            by_name.insert(name, record);
        }
    }
    for record in existing {
        if record.user_value.is_some() && !by_name.contains_key(record.name.as_str()) {
            by_name.insert(record.name.clone(), record);
        }
    }
    by_name.into_values().collect()
}

fn normalize_env_value(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

fn require_completed_environment_variable_scan(
    scan: Option<ProjectEnvironmentVariableScanInput>,
) -> Result<ProjectEnvironmentVariableScanInput, String> {
    let scan = scan.filter(|scan| scan.completed).ok_or_else(|| {
        "environment variable scan must be completed before provisioning images or saving the runtime environment"
            .to_string()
    })?;
    if scan
        .summary
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        return Err(
            "environment variable scan summary is required before saving the runtime environment"
                .to_string(),
        );
    }
    Ok(scan)
}

fn normalize_generated_config_files(
    inputs: Vec<ProjectRuntimeEnvironmentConfigFileInput>,
) -> Result<Vec<ProjectRuntimeEnvironmentConfigFileRecord>, String> {
    const MAX_CONFIG_FILE_BYTES: usize = 1024 * 1024;
    let mut by_path = std::collections::BTreeMap::new();
    for input in inputs {
        let path = normalize_generated_config_path(input.path.as_str())?;
        if by_path.contains_key(path.as_str()) {
            return Err(format!("duplicate generated config file path: {path}"));
        }
        if input.content.len() > MAX_CONFIG_FILE_BYTES {
            return Err(format!(
                "generated config file {path} exceeds {MAX_CONFIG_FILE_BYTES} bytes"
            ));
        }
        let format = input
            .format
            .and_then(normalize_owned)
            .unwrap_or_else(|| infer_config_format(path.as_str()).to_string());
        let source_files = input
            .source_files
            .into_iter()
            .filter_map(normalize_owned)
            .collect();
        by_path.insert(
            path.clone(),
            ProjectRuntimeEnvironmentConfigFileRecord {
                path,
                format,
                content: input.content,
                description: input.description.and_then(normalize_owned),
                source_files,
            },
        );
    }
    Ok(by_path.into_values().collect())
}

fn normalize_generated_config_path(value: &str) -> Result<String, String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err(format!("invalid generated config file path: {value}"));
    }
    let segments = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(|segment| *segment == "..") {
        return Err(format!("invalid generated config file path: {value}"));
    }
    Ok(segments.join("/"))
}

fn infer_config_format(path: &str) -> &'static str {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if file_name == ".env" || file_name.starts_with(".env.") {
        return "dotenv";
    }
    match file_name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("yml" | "yaml") => "yaml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("properties") => "properties",
        Some("xml") => "xml",
        Some("ini" | "conf") => "ini",
        _ => "text",
    }
}

fn env_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn image_input_to_record(
    project_id: &str,
    image: ProjectRuntimeEnvironmentImageInput,
    index: usize,
    default_provider: RuntimeEnvironmentProvider,
) -> ProjectRuntimeEnvironmentImageRecord {
    let now = now_rfc3339();
    let environment_type = image
        .environment_type
        .and_then(normalize_owned)
        .unwrap_or_else(|| "runtime".to_string());
    let environment_key = image
        .environment_key
        .and_then(normalize_owned)
        .unwrap_or_else(|| format!("{}_{}", environment_type, index + 1));
    let display_name = image
        .display_name
        .and_then(normalize_owned)
        .unwrap_or_else(|| environment_key.clone());
    let error = image.error.and_then(normalize_owned);
    let image_id = image.image_id.and_then(normalize_owned);
    let image_ref = image.image_ref.and_then(normalize_owned);
    let dockerfile = image.dockerfile.and_then(normalize_multiline_owned);
    let custom_build_script = image
        .custom_build_script
        .and_then(normalize_multiline_owned);
    let requested_status = image.status.and_then(normalize_owned);
    let status = if error.is_some() {
        "failed".to_string()
    } else if image_id.is_none() && image_ref.is_none() {
        "planned".to_string()
    } else {
        requested_status.unwrap_or_else(|| "ready".to_string())
    };
    let ports = image
        .ports
        .map(ensure_array)
        .filter(|ports| ports.as_array().is_some_and(|ports| !ports.is_empty()))
        .unwrap_or_else(|| {
            default_ports_for_environment(environment_key.as_str(), environment_type.as_str())
        });
    let mut record = ProjectRuntimeEnvironmentImageRecord {
        id: format!("project_env_image_{}", Uuid::new_v4()),
        project_id: project_id.to_string(),
        environment_key,
        environment_type,
        display_name,
        image_id,
        image_ref,
        image_provider: default_provider,
        features: image.features.map(ensure_array).unwrap_or_else(empty_array),
        ports,
        env_vars: image
            .env_vars
            .map(ensure_object)
            .unwrap_or_else(empty_object),
        dockerfile,
        custom_build_script,
        status,
        error,
        created_at: now.clone(),
        updated_at: now,
    };
    if image_is_application_runtime(&record) {
        record.image_id = None;
        record.image_ref = None;
        record.status = "planned".to_string();
        record.error = None;
    } else if let Some(image_ref) = super::compose_dependency_image_ref(&record) {
        record.image_id = None;
        record.image_ref = Some(image_ref);
        record.status = "ready".to_string();
        record.error = None;
    }
    record
}

fn parse_runtime_environment_status(
    value: &str,
) -> Result<ProjectRuntimeEnvironmentStatus, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" => Ok(ProjectRuntimeEnvironmentStatus::Disabled),
        "pending_configuration" | "pending-configuration" => {
            Ok(ProjectRuntimeEnvironmentStatus::PendingConfiguration)
        }
        "pending_image_build" | "pending-image-build" => {
            Ok(ProjectRuntimeEnvironmentStatus::PendingImageBuild)
        }
        "pending" => Ok(ProjectRuntimeEnvironmentStatus::Pending),
        "analyzing" => Ok(ProjectRuntimeEnvironmentStatus::Analyzing),
        "ready" => Ok(ProjectRuntimeEnvironmentStatus::Ready),
        "not_runnable" | "not-runnable" => Ok(ProjectRuntimeEnvironmentStatus::NotRunnable),
        "failed" => Ok(ProjectRuntimeEnvironmentStatus::Failed),
        other => Err(format!(
            "unsupported project runtime environment status: {other}"
        )),
    }
}

fn ensure_array(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        empty_array()
    }
}

fn ensure_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        empty_object()
    }
}

fn mcp_tool_result(message: impl Into<String>, structured: Value) -> Value {
    let message = message.into();
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| message.clone());
    json!({
        "content": [{
            "type": "text",
            "text": format!("{message}\n{text}")
        }],
        "_structured_result": structured
    })
}

fn generated_environment_variables(
    required_services: &Value,
    agent_env_vars: Option<&Value>,
) -> Value {
    let mut env_vars = agent_env_vars
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for service in required_services.as_array().into_iter().flatten() {
        let service_type = service
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match service_type.as_str() {
            "redis" => {
                insert_text_default(&mut env_vars, "REDIS_HOST", "redis");
                insert_text_default(&mut env_vars, "REDIS_PORT", "6379");
                insert_secret_default(&mut env_vars, "REDIS_PASSWORD");
                insert_text_default(&mut env_vars, "SPRING_DATA_REDIS_HOST", "redis");
                insert_text_default(&mut env_vars, "SPRING_DATA_REDIS_PORT", "6379");
                copy_text_default(
                    &mut env_vars,
                    "REDIS_PASSWORD",
                    "SPRING_DATA_REDIS_PASSWORD",
                );
            }
            "postgres" | "postgresql" => {
                insert_text_default(&mut env_vars, "POSTGRES_HOST", "postgres");
                insert_text_default(&mut env_vars, "POSTGRES_PORT", "5432");
                insert_text_default(&mut env_vars, "POSTGRES_USER", "app");
                insert_secret_default(&mut env_vars, "POSTGRES_PASSWORD");
                insert_text_default(&mut env_vars, "POSTGRES_DB", "app");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_DATASOURCE_URL",
                    "jdbc:postgresql://postgres:5432/app",
                );
                copy_text_default(&mut env_vars, "POSTGRES_USER", "SPRING_DATASOURCE_USERNAME");
                copy_text_default(
                    &mut env_vars,
                    "POSTGRES_PASSWORD",
                    "SPRING_DATASOURCE_PASSWORD",
                );
            }
            "mysql" | "mariadb" => {
                insert_text_default(&mut env_vars, "MYSQL_HOST", "mysql");
                insert_text_default(&mut env_vars, "MYSQL_PORT", "3306");
                insert_secret_default(&mut env_vars, "MYSQL_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "MYSQL_DATABASE", "app");
                insert_text_default(&mut env_vars, "MYSQL_USER", "app");
                insert_secret_default(&mut env_vars, "MYSQL_PASSWORD");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_DATASOURCE_URL",
                    "jdbc:mysql://mysql:3306/app?useSSL=false&allowPublicKeyRetrieval=true",
                );
                copy_text_default(&mut env_vars, "MYSQL_USER", "SPRING_DATASOURCE_USERNAME");
                copy_text_default(
                    &mut env_vars,
                    "MYSQL_PASSWORD",
                    "SPRING_DATASOURCE_PASSWORD",
                );
            }
            "nacos" => {
                insert_text_default(&mut env_vars, "NACOS_SERVER_ADDR", "nacos:8848");
                insert_text_default(&mut env_vars, "NACOS_NAMESPACE", "public");
                insert_text_default(&mut env_vars, "NACOS_USERNAME", "nacos");
                insert_secret_default(&mut env_vars, "NACOS_PASSWORD");
                insert_secret_default(&mut env_vars, "NACOS_AUTH_TOKEN");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_CLOUD_NACOS_SERVER_ADDR",
                    "nacos:8848",
                );
                copy_text_default(
                    &mut env_vars,
                    "NACOS_USERNAME",
                    "SPRING_CLOUD_NACOS_USERNAME",
                );
                copy_text_default(
                    &mut env_vars,
                    "NACOS_PASSWORD",
                    "SPRING_CLOUD_NACOS_PASSWORD",
                );
            }
            "mongodb" | "mongo" => {
                insert_text_default(&mut env_vars, "MONGODB_HOST", "mongodb");
                insert_text_default(&mut env_vars, "MONGODB_PORT", "27017");
                insert_text_default(&mut env_vars, "MONGO_INITDB_ROOT_USERNAME", "app");
                insert_secret_default(&mut env_vars, "MONGO_INITDB_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "SPRING_DATA_MONGODB_HOST", "mongodb");
                insert_text_default(&mut env_vars, "SPRING_DATA_MONGODB_PORT", "27017");
                copy_text_default(
                    &mut env_vars,
                    "MONGO_INITDB_ROOT_USERNAME",
                    "SPRING_DATA_MONGODB_USERNAME",
                );
                copy_text_default(
                    &mut env_vars,
                    "MONGO_INITDB_ROOT_PASSWORD",
                    "SPRING_DATA_MONGODB_PASSWORD",
                );
            }
            "rabbitmq" => {
                insert_text_default(&mut env_vars, "RABBITMQ_HOST", "rabbitmq");
                insert_text_default(&mut env_vars, "RABBITMQ_PORT", "5672");
                insert_text_default(&mut env_vars, "RABBITMQ_DEFAULT_USER", "app");
                insert_secret_default(&mut env_vars, "RABBITMQ_DEFAULT_PASS");
            }
            _ => {}
        }
    }
    Value::Object(env_vars)
}

fn environment_has_provisionable_evidence(
    detected_stack: &Value,
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageInput],
) -> bool {
    !images.is_empty()
        || required_services
            .as_array()
            .is_some_and(|services| !services.is_empty())
        || [
            "language",
            "languages",
            "runtime",
            "framework",
            "frameworks",
            "build_tool",
            "package_manager",
            "project_type",
            "entrypoint",
            "startup_command",
        ]
        .iter()
        .any(|key| json_value_has_content(detected_stack.get(*key)))
        || detected_stack
            .get("manifests")
            .and_then(Value::as_array)
            .is_some_and(|manifests| {
                manifests.iter().any(|manifest| {
                    manifest
                        .as_str()
                        .is_some_and(is_executable_project_manifest)
                })
            })
}

fn infer_service_kinds_from_environment_variables(
    existing: &[ProjectRuntimeEnvironmentVariableRecord],
    inputs: &[ProjectRuntimeEnvironmentVariableInput],
    legacy_env_vars: Option<&Value>,
) -> std::collections::BTreeSet<String> {
    let mut kinds = std::collections::BTreeSet::new();
    for record in existing {
        infer_service_kinds_from_text(record.name.as_str(), &mut kinds);
        for value in [
            record.project_value.as_deref(),
            record.recommended_value.as_deref(),
            record.user_value.as_deref(),
            record.effective_value.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            infer_service_kinds_from_text(value, &mut kinds);
        }
    }
    for input in inputs {
        infer_service_kinds_from_text(input.name.as_str(), &mut kinds);
        for value in [
            input.project_value.as_deref(),
            input.recommended_value.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            infer_service_kinds_from_text(value, &mut kinds);
        }
    }
    if let Some(values) = legacy_env_vars.and_then(Value::as_object) {
        for (name, value) in values {
            infer_service_kinds_from_text(name.as_str(), &mut kinds);
            if let Some(value) = env_value_to_string(value) {
                infer_service_kinds_from_text(value.as_str(), &mut kinds);
            }
        }
    }
    kinds
}

fn infer_service_kinds_from_text(value: &str, kinds: &mut std::collections::BTreeSet<String>) {
    let value = value.trim().to_ascii_lowercase();
    for (kind, markers) in [
        ("nacos", &["nacos"] as &[_]),
        ("mongodb", &["mongodb", "mongo_", "mongo.", "mongo://"]),
        ("mysql", &["mysql", "mariadb"]),
        ("postgres", &["postgres", "postgresql"]),
        ("redis", &["redis"]),
        ("rabbitmq", &["rabbitmq", "amqp://"]),
        ("kafka", &["kafka"]),
        ("elasticsearch", &["elasticsearch", "opensearch"]),
        ("minio", &["minio"]),
    ] {
        if markers.iter().any(|marker| value.contains(marker)) {
            kinds.insert(kind.to_string());
        }
    }
}

fn ensure_required_service_records(
    required_services: &mut Value,
    inferred_kinds: std::collections::BTreeSet<String>,
) {
    let services = required_services
        .as_array_mut()
        .expect("required services are normalized to an array");
    let existing = provisionable_service_kinds(&Value::Array(services.clone()));
    for kind in inferred_kinds {
        if !existing.contains(kind.as_str()) {
            services.push(json!({
                "type": kind,
                "source": "environment_variable_scan"
            }));
        }
    }
}

fn validate_environment_image_plans(
    detected_stack: &Value,
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<(), String> {
    let mut missing = Vec::new();
    if stack_requires_application_runtime(detected_stack) {
        if !images
            .iter()
            .any(|image| image_plan_is_complete(image) && image_is_application_runtime(image))
        {
            missing.push("application runtime".to_string());
        }
    }
    for service in provisionable_service_kinds(required_services) {
        if !images
            .iter()
            .any(|image| image_matches_service(image, service.as_str()))
        {
            missing.push(service);
        }
    }
    let invalid_plans = images
        .iter()
        .filter(|image| image_is_application_runtime(image) && !image_plan_is_complete(image))
        .map(|image| image.environment_key.clone())
        .collect::<Vec<_>>();
    if missing.is_empty() && invalid_plans.is_empty() {
        return Ok(());
    }
    let mut reasons = Vec::new();
    if !missing.is_empty() {
        reasons.push(format!(
            "missing real ready images for: {}",
            missing.join(", ")
        ));
    }
    if !invalid_plans.is_empty() {
        reasons.push(format!(
            "application plans without Dockerfile content: {}",
            invalid_plans.join(", ")
        ));
    }
    Err(format!(
        "runtime environment composition planning is incomplete: {}. Generate one application Dockerfile and include one service record for every detected dependency; dependency services are grouped under the generated project-level Docker Compose file.",
        reasons.join("; ")
    ))
}

const PROJECT_COMPOSE_FILE_PATH: &str = ".chatos/runtime-environment/docker-compose.chatos.yml";

fn upsert_project_compose_config_file(
    project_id: &str,
    files: &mut Vec<ProjectRuntimeEnvironmentConfigFileRecord>,
    variables: &[ProjectRuntimeEnvironmentVariableRecord],
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<(), String> {
    let compose = build_project_compose_yaml(project_id, variables, required_services, images)?;
    files.retain(|file| file.path != PROJECT_COMPOSE_FILE_PATH);
    files.push(ProjectRuntimeEnvironmentConfigFileRecord {
        path: PROJECT_COMPOSE_FILE_PATH.to_string(),
        format: "yaml".to_string(),
        content: compose,
        description: Some(
            "项目级 Docker Compose 编排文件：应用和所有依赖服务会作为同一个 Compose 项目启动。"
                .to_string(),
        ),
        source_files: vec!["项目环境扫描结果".to_string()],
    });
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

fn build_project_compose_yaml(
    project_id: &str,
    variables: &[ProjectRuntimeEnvironmentVariableRecord],
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<String, String> {
    let application = images
        .iter()
        .find(|image| image_is_application_runtime(image))
        .ok_or_else(|| "application runtime plan is required for Docker Compose".to_string())?;
    if application
        .dockerfile
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err("application Dockerfile is required for Docker Compose".to_string());
    }
    let service_kinds = provisionable_service_kinds(required_services);
    let mut output = String::new();
    output.push_str("name: ");
    output.push_str(yaml_string(compose_project_name(project_id).as_str()).as_str());
    output.push_str("\nservices:\n  application:\n    build:\n      context: ../..\n      dockerfile: .chatos/runtime-environment/Dockerfile.application\n    env_file:\n      - .env.chatos\n");
    if let Some(ports) = application.ports.as_array() {
        let ports = ports
            .iter()
            .filter_map(Value::as_u64)
            .filter(|port| *port > 0 && *port <= u16::MAX as u64)
            .collect::<Vec<_>>();
        if !ports.is_empty() {
            output.push_str("    ports:\n");
            for port in ports {
                output.push_str(format!("      - \"{port}:{port}\"\n").as_str());
            }
        }
    }
    if !service_kinds.is_empty() {
        output.push_str("    depends_on:\n");
        for service in &service_kinds {
            output.push_str(
                format!(
                    "      {}:\n        condition: service_healthy\n",
                    compose_service_name(service)
                )
                .as_str(),
            );
        }
    }
    output.push_str("    networks:\n      - chatos-runtime\n    restart: unless-stopped\n");
    for service in &service_kinds {
        append_compose_dependency_service(&mut output, service.as_str());
    }
    output.push_str("networks:\n  chatos-runtime:\n    driver: bridge\n");
    let volumes = service_kinds
        .iter()
        .filter_map(|service| compose_service_volume(service.as_str()))
        .collect::<Vec<_>>();
    if !volumes.is_empty() {
        output.push_str("volumes:\n");
        for volume in volumes {
            output.push_str(format!("  {volume}:\n").as_str());
        }
    }
    let _ = variables;
    Ok(output)
}

fn compose_project_name(project_id: &str) -> String {
    let suffix = project_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    format!(
        "chatos-{}",
        if suffix.is_empty() {
            "project"
        } else {
            suffix.as_str()
        }
    )
}

fn compose_service_name(service: &str) -> &str {
    match service {
        "mongodb" => "mongodb",
        "postgres" => "postgres",
        other => other,
    }
}

fn compose_service_volume(service: &str) -> Option<&'static str> {
    match service {
        "mysql" => Some("mysql-data"),
        "mongodb" => Some("mongodb-data"),
        "postgres" => Some("postgres-data"),
        "redis" => Some("redis-data"),
        "nacos" => Some("nacos-data"),
        "rabbitmq" => Some("rabbitmq-data"),
        "kafka" => Some("kafka-data"),
        "elasticsearch" => Some("elasticsearch-data"),
        "minio" => Some("minio-data"),
        _ => None,
    }
}

fn append_compose_dependency_service(output: &mut String, service: &str) {
    match service {
        "mysql" => output.push_str("  mysql:\n    image: mysql:8.4\n    env_file: [.env.chatos]\n    environment:\n      MYSQL_DATABASE: ${MYSQL_DATABASE:-app}\n      MYSQL_USER: ${MYSQL_USER:-app}\n      MYSQL_PASSWORD: ${MYSQL_PASSWORD}\n      MYSQL_ROOT_PASSWORD: ${MYSQL_ROOT_PASSWORD}\n    ports: [\"3306:3306\"]\n    volumes: [mysql-data:/var/lib/mysql]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mysqladmin ping -h 127.0.0.1 -p$${MYSQL_ROOT_PASSWORD} --silent\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "mongodb" => output.push_str("  mongodb:\n    image: mongo:7.0\n    env_file: [.env.chatos]\n    environment:\n      MONGO_INITDB_ROOT_USERNAME: ${MONGO_INITDB_ROOT_USERNAME:-app}\n      MONGO_INITDB_ROOT_PASSWORD: ${MONGO_INITDB_ROOT_PASSWORD}\n      MONGO_INITDB_DATABASE: ${MONGODB_DATABASE:-app}\n    ports: [\"27017:27017\"]\n    volumes: [mongodb-data:/data/db]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mongosh --quiet --eval 'db.runCommand({ ping: 1 }).ok' || exit 1\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "postgres" => output.push_str("  postgres:\n    image: postgres:16-alpine\n    env_file: [.env.chatos]\n    environment:\n      POSTGRES_DB: ${POSTGRES_DB:-app}\n      POSTGRES_USER: ${POSTGRES_USER:-app}\n      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}\n    ports: [\"5432:5432\"]\n    volumes: [postgres-data:/var/lib/postgresql/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "redis" => output.push_str("  redis:\n    image: redis:7-alpine\n    env_file: [.env.chatos]\n    command: [\"sh\", \"-c\", \"exec redis-server --appendonly yes --requirepass '$${REDIS_PASSWORD}'\"]\n    ports: [\"6379:6379\"]\n    volumes: [redis-data:/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"redis-cli -a '$${REDIS_PASSWORD}' ping | grep PONG\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "nacos" => output.push_str("  nacos:\n    image: nacos/nacos-server:v2.4.3\n    environment:\n      MODE: standalone\n      NACOS_AUTH_ENABLE: \"false\"\n    ports: [\"8848:8848\", \"9848:9848\", \"9849:9849\"]\n    volumes: [nacos-data:/home/nacos/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:8848/nacos/ >/dev/null || exit 1\"]\n      interval: 15s\n      timeout: 5s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "rabbitmq" => output.push_str("  rabbitmq:\n    image: rabbitmq:3.13-management-alpine\n    env_file: [.env.chatos]\n    environment:\n      RABBITMQ_DEFAULT_USER: ${RABBITMQ_DEFAULT_USER:-app}\n      RABBITMQ_DEFAULT_PASS: ${RABBITMQ_DEFAULT_PASS}\n    ports: [\"5672:5672\", \"15672:15672\"]\n    volumes: [rabbitmq-data:/var/lib/rabbitmq]\n    healthcheck:\n      test: [\"CMD\", \"rabbitmq-diagnostics\", \"-q\", \"ping\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "kafka" => output.push_str("  kafka:\n    image: bitnami/kafka:3.7\n    environment:\n      KAFKA_CFG_NODE_ID: 1\n      KAFKA_CFG_PROCESS_ROLES: broker,controller\n      KAFKA_CFG_CONTROLLER_QUORUM_VOTERS: 1@kafka:9093\n      KAFKA_CFG_LISTENERS: PLAINTEXT://:9092,CONTROLLER://:9093\n      KAFKA_CFG_ADVERTISED_LISTENERS: PLAINTEXT://kafka:9092\n      KAFKA_CFG_LISTENER_SECURITY_PROTOCOL_MAP: CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT\n      KAFKA_CFG_CONTROLLER_LISTENER_NAMES: CONTROLLER\n    ports: [\"9092:9092\"]\n    volumes: [kafka-data:/bitnami/kafka]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"kafka-topics.sh --bootstrap-server 127.0.0.1:9092 --list >/dev/null 2>&1\"]\n      interval: 15s\n      timeout: 10s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "elasticsearch" => output.push_str("  elasticsearch:\n    image: docker.elastic.co/elasticsearch/elasticsearch:8.14.3\n    environment:\n      discovery.type: single-node\n      xpack.security.enabled: \"false\"\n      ES_JAVA_OPTS: -Xms512m -Xmx512m\n    ports: [\"9200:9200\"]\n    volumes: [elasticsearch-data:/usr/share/elasticsearch/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:9200/_cluster/health >/dev/null\"]\n      interval: 15s\n      timeout: 10s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "minio" => output.push_str("  minio:\n    image: minio/minio:latest\n    env_file: [.env.chatos]\n    environment:\n      MINIO_ROOT_USER: ${MINIO_ROOT_USER:-minioadmin}\n      MINIO_ROOT_PASSWORD: ${MINIO_ROOT_PASSWORD}\n    command: server /data --console-address :9001\n    ports: [\"9000:9000\", \"9001:9001\"]\n    volumes: [minio-data:/data]\n    healthcheck:\n      test: [\"CMD\", \"mc\", \"ready\", \"local\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        _ => {}
    }
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"chatos-project\"".to_string())
}

fn image_plan_is_complete(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image_is_real_and_ready(image)
        || image
            .dockerfile
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn stack_requires_application_runtime(detected_stack: &Value) -> bool {
    [
        "language",
        "languages",
        "runtime",
        "framework",
        "frameworks",
        "build_tool",
        "package_manager",
        "project_type",
        "entrypoint",
        "startup_command",
    ]
    .iter()
    .any(|key| json_value_has_content(detected_stack.get(*key)))
        || detected_stack
            .get("manifests")
            .and_then(Value::as_array)
            .is_some_and(|manifests| {
                manifests.iter().any(|manifest| {
                    manifest.as_str().is_some_and(|value| {
                        let value = value.trim().to_ascii_lowercase();
                        [
                            "package.json",
                            "cargo.toml",
                            "pyproject.toml",
                            "requirements.txt",
                            "go.mod",
                            "pom.xml",
                            "build.gradle",
                            "build.gradle.kts",
                        ]
                        .iter()
                        .any(|candidate| value.ends_with(candidate))
                    })
                })
            })
}

fn provisionable_service_kinds(required_services: &Value) -> std::collections::BTreeSet<String> {
    let mut kinds = std::collections::BTreeSet::new();
    for service in required_services.as_array().into_iter().flatten() {
        let raw = service
            .as_str()
            .or_else(|| {
                ["type", "service_type", "kind", "name", "service"]
                    .iter()
                    .find_map(|key| service.get(*key).and_then(Value::as_str))
            })
            .unwrap_or_default();
        infer_service_kinds_from_text(raw, &mut kinds);
    }
    kinds
}

fn image_is_real_and_ready(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image.image_provider != RuntimeEnvironmentProvider::None
        && image
            .image_id
            .as_deref()
            .or(image.image_ref.as_deref())
            .is_some_and(|value| !value.trim().is_empty())
        && matches!(
            image.status.trim().to_ascii_lowercase().as_str(),
            "ready" | "available" | "local" | "succeeded" | "completed" | "running"
        )
}

fn image_is_application_runtime(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let environment_type = image.environment_type.trim().to_ascii_lowercase();
    let environment_key = image.environment_key.trim().to_ascii_lowercase();
    environment_type.contains("runtime")
        || environment_type.contains("application")
        || matches!(
            environment_key.as_str(),
            "app" | "application" | "runtime" | "application_runtime"
        )
        || environment_key.ends_with("_runtime")
}

fn image_matches_service(image: &ProjectRuntimeEnvironmentImageRecord, service: &str) -> bool {
    let identity = format!(
        "{} {} {}",
        image.environment_key, image.environment_type, image.display_name
    )
    .to_ascii_lowercase();
    match service {
        "mongodb" => ["mongodb", "mongo"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "mysql" => ["mysql", "mariadb"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "postgres" => ["postgres", "postgresql"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "elasticsearch" => ["elasticsearch", "opensearch"]
            .iter()
            .any(|alias| identity.contains(alias)),
        other => identity.contains(other),
    }
}

fn json_value_has_content(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(_)) => true,
        _ => false,
    }
}

fn is_executable_project_manifest(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    [
        "package.json",
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ]
    .iter()
    .any(|manifest| value.ends_with(manifest))
}

fn default_ports_for_environment(environment_key: &str, environment_type: &str) -> Value {
    let identity = format!("{environment_key} {environment_type}").to_ascii_lowercase();
    let ports: &[u16] = if identity.contains("nacos") {
        &[8848, 9848, 9849]
    } else if identity.contains("postgres") {
        &[5432]
    } else if identity.contains("mysql") || identity.contains("mariadb") {
        &[3306]
    } else if identity.contains("redis") {
        &[6379]
    } else if identity.contains("mongo") {
        &[27017]
    } else if identity.contains("rabbitmq") {
        &[5672, 15672]
    } else {
        &[]
    };
    Value::Array(ports.iter().copied().map(Value::from).collect())
}

fn insert_text_default(env_vars: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn copy_text_default(env_vars: &mut serde_json::Map<String, Value>, source: &str, target: &str) {
    let Some(value) = env_vars
        .get(source)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        return;
    };
    insert_text_default(env_vars, target, value.as_str());
}

fn insert_secret_default(env_vars: &mut serde_json::Map<String, Value>, key: &str) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(
            key.to_string(),
            Value::String(format!("pm-{}", Uuid::new_v4().simple())),
        );
    }
}

fn normalize_owned(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn normalize_multiline_owned(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned_image(
        environment_key: &str,
        environment_type: &str,
    ) -> ProjectRuntimeEnvironmentImageRecord {
        ProjectRuntimeEnvironmentImageRecord {
            id: format!("record-{environment_key}"),
            project_id: "project-1".to_string(),
            environment_key: environment_key.to_string(),
            environment_type: environment_type.to_string(),
            display_name: environment_key.to_string(),
            image_id: None,
            image_ref: None,
            image_provider: RuntimeEnvironmentProvider::LocalConnector,
            features: json!(["base"]),
            ports: json!([]),
            env_vars: json!({}),
            dockerfile: Some(format!("FROM ubuntu:24.04\n# {environment_key}\n")),
            custom_build_script: Some("set -e\ntrue\n".to_string()),
            status: "planned".to_string(),
            error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn runtime_environment_cannot_be_saved_before_environment_variable_scan() {
        let missing = require_completed_environment_variable_scan(None).unwrap_err();
        assert!(missing.contains("scan must be completed"));

        let incomplete = require_completed_environment_variable_scan(Some(
            ProjectEnvironmentVariableScanInput {
                completed: false,
                files_scanned: vec![".env.example".to_string()],
                reference_count: 1,
                summary: Some("发现一个变量".to_string()),
            },
        ))
        .unwrap_err();
        assert!(incomplete.contains("scan must be completed"));

        let missing_summary = require_completed_environment_variable_scan(Some(
            ProjectEnvironmentVariableScanInput {
                completed: true,
                files_scanned: Vec::new(),
                reference_count: 0,
                summary: Some(" ".to_string()),
            },
        ))
        .unwrap_err();
        assert!(missing_summary.contains("scan summary is required"));

        assert!(require_completed_environment_variable_scan(Some(
            ProjectEnvironmentVariableScanInput {
                completed: true,
                files_scanned: vec!["src/main.rs".to_string()],
                reference_count: 0,
                summary: Some("已完成全项目扫描，未发现环境变量引用。".to_string()),
            },
        ))
        .is_ok());
    }

    #[test]
    fn generated_config_files_are_normalized_and_cannot_escape_workspace() {
        let files =
            normalize_generated_config_files(vec![ProjectRuntimeEnvironmentConfigFileInput {
                path: " ./config/application-sandbox.yml ".to_string(),
                format: None,
                content: "server:\n  port: ${APP_PORT}\n".to_string(),
                description: Some("沙箱运行配置".to_string()),
                source_files: vec!["src/main/resources/application.yml".to_string()],
            }])
            .expect("normalize generated config file");
        assert_eq!(files[0].path, "config/application-sandbox.yml");
        assert_eq!(files[0].format, "yaml");
        assert!(
            normalize_generated_config_files(vec![ProjectRuntimeEnvironmentConfigFileInput {
                path: "../application.yml".to_string(),
                content: "server: {}".to_string(),
                ..ProjectRuntimeEnvironmentConfigFileInput::default()
            },])
            .is_err()
        );
    }

    #[test]
    fn environment_variables_restore_omitted_dependency_plans() {
        let kinds = infer_service_kinds_from_environment_variables(
            &[],
            &[
                ProjectRuntimeEnvironmentVariableInput {
                    name: "SPRING_DATASOURCE_URL".to_string(),
                    recommended_value: Some("jdbc:mysql://mysql:3306/app".to_string()),
                    ..ProjectRuntimeEnvironmentVariableInput::default()
                },
                ProjectRuntimeEnvironmentVariableInput {
                    name: "SPRING_DATA_MONGODB_HOST".to_string(),
                    recommended_value: Some("mongodb".to_string()),
                    ..ProjectRuntimeEnvironmentVariableInput::default()
                },
                ProjectRuntimeEnvironmentVariableInput {
                    name: "SPRING_CLOUD_NACOS_CONFIG_ENABLED".to_string(),
                    recommended_value: Some("false".to_string()),
                    ..ProjectRuntimeEnvironmentVariableInput::default()
                },
                ProjectRuntimeEnvironmentVariableInput {
                    name: "REDIS_HOST".to_string(),
                    recommended_value: Some("redis".to_string()),
                    ..ProjectRuntimeEnvironmentVariableInput::default()
                },
            ],
            None,
        );
        assert_eq!(
            kinds,
            ["mongodb", "mysql", "nacos", "redis"]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        );
        let mut services = json!([]);
        ensure_required_service_records(&mut services, kinds);
        assert_eq!(provisionable_service_kinds(&services).len(), 4);
    }

    #[test]
    fn application_image_plan_ignores_agent_ready_state_and_provider_override() {
        let record = image_input_to_record(
            "project-1",
            ProjectRuntimeEnvironmentImageInput {
                environment_key: Some("application_runtime".to_string()),
                environment_type: Some("runtime".to_string()),
                image_id: Some("agent-image".to_string()),
                image_ref: Some("agent/runtime:latest".to_string()),
                _image_provider: Some("local_connector".to_string()),
                dockerfile: Some("FROM node:24".to_string()),
                status: Some("ready".to_string()),
                ..ProjectRuntimeEnvironmentImageInput::default()
            },
            0,
            RuntimeEnvironmentProvider::CloudSandboxManager,
        );

        assert_eq!(
            record.image_provider,
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
        assert_eq!(record.status, "planned");
        assert!(record.image_id.is_none());
        assert!(record.image_ref.is_none());
    }

    #[test]
    fn dependency_image_plan_uses_platform_image_without_manual_build() {
        let record = image_input_to_record(
            "project-1",
            ProjectRuntimeEnvironmentImageInput {
                environment_key: Some("redis".to_string()),
                environment_type: Some("service".to_string()),
                _image_provider: Some("local_connector".to_string()),
                status: Some("planned".to_string()),
                ..ProjectRuntimeEnvironmentImageInput::default()
            },
            1,
            RuntimeEnvironmentProvider::CloudSandboxManager,
        );

        assert_eq!(
            record.image_provider,
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
        assert_eq!(record.image_ref.as_deref(), Some("redis:7-alpine"));
        assert_eq!(record.status, "ready");
    }

    #[test]
    fn compose_planning_requires_application_dockerfile_and_each_dependency_record() {
        let stack = json!({"languages": ["java"], "manifests": ["pom.xml"]});
        let services = json!([
            {"type": "mysql"},
            {"type": "mongodb"},
            {"type": "redis"},
            {"type": "nacos"}
        ]);
        let mut images = vec![
            planned_image("application_runtime", "runtime"),
            planned_image("mysql", "service"),
            planned_image("mongodb", "service"),
            planned_image("redis", "service"),
            planned_image("nacos", "service"),
        ];
        validate_environment_image_plans(&stack, &services, &images)
            .expect("all Dockerfile plans exist");

        images.retain(|image| image.environment_key != "redis");
        let missing = validate_environment_image_plans(&stack, &services, &images)
            .expect_err("missing redis plan must be rejected");
        assert!(missing.contains("redis"));

        let mut standard_service = planned_image("redis", "service");
        standard_service.dockerfile = None;
        images.push(standard_service);
        validate_environment_image_plans(&stack, &services, &images)
            .expect("dependency services use platform-maintained images");

        images[0].dockerfile = None;
        let invalid = validate_environment_image_plans(&stack, &services, &images)
            .expect_err("application Dockerfile must be rejected");
        assert!(invalid.contains("application"));
    }

    #[test]
    fn project_compose_groups_application_and_dependencies() {
        let images = vec![
            planned_image("application_runtime", "runtime"),
            planned_image("mysql", "service"),
            planned_image("redis", "service"),
        ];
        let compose = build_project_compose_yaml(
            "project-123",
            &[],
            &json!([{"type": "mysql"}, {"type": "redis"}]),
            images.as_slice(),
        )
        .expect("compose plan");
        assert!(compose.contains("name: \"chatos-project123\""));
        assert!(compose.contains("  application:"));
        assert!(compose.contains("  mysql:"));
        assert!(compose.contains("  redis:"));
        assert!(compose.contains("depends_on:"));
    }

    #[test]
    fn runnable_stack_cannot_be_downgraded_to_not_runnable() {
        assert!(environment_has_provisionable_evidence(
            &json!({
                "language": "Java",
                "build_tool": "Maven",
                "project_type": "Spring Boot backend"
            }),
            &json!([]),
            &[],
        ));
        assert!(environment_has_provisionable_evidence(
            &json!({}),
            &json!([{"type": "redis"}]),
            &[],
        ));
        assert!(!environment_has_provisionable_evidence(
            &json!({"source": "scan"}),
            &json!([]),
            &[],
        ));
    }

    #[test]
    fn detected_services_receive_local_connection_defaults() {
        let env = generated_environment_variables(
            &json!([
                {"type": "nacos"},
                {"type": "redis"},
                {"type": "mongodb"},
                {"type": "mysql"}
            ]),
            None,
        );
        assert_eq!(env["NACOS_SERVER_ADDR"], "nacos:8848");
        assert_eq!(env["SPRING_DATA_REDIS_HOST"], "redis");
        assert_eq!(env["SPRING_DATA_MONGODB_HOST"], "mongodb");
        assert_eq!(
            env["SPRING_DATASOURCE_URL"],
            "jdbc:mysql://mysql:3306/app?useSSL=false&allowPublicKeyRetrieval=true"
        );
        assert_eq!(env["MYSQL_PASSWORD"], env["SPRING_DATASOURCE_PASSWORD"]);
    }

    #[test]
    fn service_images_receive_default_ports() {
        assert_eq!(
            default_ports_for_environment("redis", "service"),
            json!([6379])
        );
        assert_eq!(
            default_ports_for_environment("nacos", "service"),
            json!([8848, 9848, 9849])
        );
        assert_eq!(default_ports_for_environment("app", "runtime"), json!([]));
    }
}
