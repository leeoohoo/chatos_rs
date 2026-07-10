// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback};

use crate::models::{
    empty_array, empty_object, now_rfc3339, ProjectRecord, ProjectRuntimeEnvironmentImageRecord,
    ProjectRuntimeEnvironmentStatus, RuntimeEnvironmentProvider,
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
    images: Vec<ProjectRuntimeEnvironmentImageInput>,
    #[serde(default)]
    last_error: Option<String>,
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
    #[serde(default)]
    image_provider: Option<String>,
    #[serde(default)]
    features: Option<Value>,
    #[serde(default)]
    ports: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
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
        vec![
            json!({
                "name": "get_current_project_runtime_environment",
                "description": "Get the current project details and persisted runtime environment for this project. The project id is bound by the server.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "update_current_project_runtime_environment",
                "description": "Persist the current project's runtime environment analysis, required service images, generated environment variables, or non-runnable reason. The project id is bound by the server.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["ready", "not_runnable", "failed", "pending_configuration"]
                        },
                        "analysis_summary": {"type": "string"},
                        "not_runnable_reason": {"type": ["string", "null"]},
                        "detected_stack": {"type": "object"},
                        "required_services": {"type": "array"},
                        "env_vars": {"type": "object"},
                        "last_error": {"type": ["string", "null"]},
                        "images": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "environment_key": {"type": "string"},
                                    "environment_type": {"type": "string"},
                                    "display_name": {"type": "string"},
                                    "image_id": {"type": ["string", "null"]},
                                    "image_ref": {"type": ["string", "null"]},
                                    "image_provider": {"type": "string"},
                                    "features": {"type": "array"},
                                    "ports": {"type": "array"},
                                    "env_vars": {"type": "object"},
                                    "status": {"type": "string"},
                                    "error": {"type": ["string", "null"]}
                                },
                                "required": ["environment_key", "environment_type", "display_name", "status"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "additionalProperties": false
                }
            }),
        ]
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
        let environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));
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

        if let Some(value) = args.analysis_summary.and_then(normalize_owned) {
            environment.analysis_summary = Some(value);
        }
        environment.not_runnable_reason = args.not_runnable_reason.and_then(normalize_owned);
        if let Some(value) = args.detected_stack {
            environment.detected_stack = ensure_object(value);
        }
        if let Some(value) = args.required_services {
            environment.required_services = ensure_array(value);
        }
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
        environment.status = match args.status.as_deref() {
            Some(status) => parse_runtime_environment_status(status)?,
            None => inferred_status,
        };
        if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable {
            environment.required_services = empty_array();
        }
        let env_source = args.env_vars.as_ref().or(Some(&environment.env_vars));
        environment.env_vars =
            generated_environment_variables(&environment.required_services, env_source);
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
    let status = image
        .status
        .and_then(normalize_owned)
        .unwrap_or_else(|| if error.is_some() { "failed" } else { "ready" }.to_string());
    ProjectRuntimeEnvironmentImageRecord {
        id: format!("project_env_image_{}", Uuid::new_v4()),
        project_id: project_id.to_string(),
        environment_key,
        environment_type,
        display_name,
        image_id: image.image_id.and_then(normalize_owned),
        image_ref: image.image_ref.and_then(normalize_owned),
        image_provider: image
            .image_provider
            .as_deref()
            .map(parse_runtime_environment_provider)
            .unwrap_or(default_provider),
        features: image.features.map(ensure_array).unwrap_or_else(empty_array),
        ports: image.ports.map(ensure_array).unwrap_or_else(empty_array),
        env_vars: image
            .env_vars
            .map(ensure_object)
            .unwrap_or_else(empty_object),
        status,
        error,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn parse_runtime_environment_status(
    value: &str,
) -> Result<ProjectRuntimeEnvironmentStatus, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" => Ok(ProjectRuntimeEnvironmentStatus::Disabled),
        "pending_configuration" | "pending-configuration" => {
            Ok(ProjectRuntimeEnvironmentStatus::PendingConfiguration)
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

fn parse_runtime_environment_provider(value: &str) -> RuntimeEnvironmentProvider {
    match value.trim().to_ascii_lowercase().as_str() {
        "local_connector" | "local" => RuntimeEnvironmentProvider::LocalConnector,
        "harness" => RuntimeEnvironmentProvider::Harness,
        "cloud_sandbox_manager" | "cloud" | "sandbox_manager" => {
            RuntimeEnvironmentProvider::CloudSandboxManager
        }
        _ => RuntimeEnvironmentProvider::None,
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
            "redis" => insert_secret_default(&mut env_vars, "REDIS_PASSWORD"),
            "postgres" | "postgresql" => {
                insert_text_default(&mut env_vars, "POSTGRES_USER", "app");
                insert_secret_default(&mut env_vars, "POSTGRES_PASSWORD");
                insert_text_default(&mut env_vars, "POSTGRES_DB", "app");
            }
            "mysql" | "mariadb" => {
                insert_secret_default(&mut env_vars, "MYSQL_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "MYSQL_DATABASE", "app");
                insert_text_default(&mut env_vars, "MYSQL_USER", "app");
                insert_secret_default(&mut env_vars, "MYSQL_PASSWORD");
            }
            "nacos" => {
                insert_text_default(&mut env_vars, "NACOS_USERNAME", "nacos");
                insert_secret_default(&mut env_vars, "NACOS_PASSWORD");
                insert_secret_default(&mut env_vars, "NACOS_AUTH_TOKEN");
            }
            "mongodb" | "mongo" => {
                insert_text_default(&mut env_vars, "MONGO_INITDB_ROOT_USERNAME", "app");
                insert_secret_default(&mut env_vars, "MONGO_INITDB_ROOT_PASSWORD");
            }
            "rabbitmq" => {
                insert_text_default(&mut env_vars, "RABBITMQ_DEFAULT_USER", "app");
                insert_secret_default(&mut env_vars, "RABBITMQ_DEFAULT_PASS");
            }
            _ => {}
        }
    }
    Value::Object(env_vars)
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
