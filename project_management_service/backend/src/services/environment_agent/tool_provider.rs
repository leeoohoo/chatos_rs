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

        let requested_status = args
            .status
            .as_deref()
            .map(parse_runtime_environment_status)
            .transpose()?;
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
        environment.status = requested_status.unwrap_or(inferred_status);
        if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable {
            environment.required_services = empty_array();
        } else {
            environment.not_runnable_reason = None;
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
    let ports = image
        .ports
        .map(ensure_array)
        .filter(|ports| ports.as_array().is_some_and(|ports| !ports.is_empty()))
        .unwrap_or_else(|| {
            default_ports_for_environment(environment_key.as_str(), environment_type.as_str())
        });
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
        ports,
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

#[cfg(test)]
mod tests {
    use super::*;

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
