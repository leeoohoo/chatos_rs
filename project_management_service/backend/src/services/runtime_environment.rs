// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::store::AppStore;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub async fn ensure_runtime_environment_for_project(
    store: &AppStore,
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> Result<ProjectRuntimeEnvironmentRecord, String> {
    if let Some(mut existing) = store
        .get_project_runtime_environment(project.id.as_str())
        .await?
    {
        if let Some(sandbox_enabled) = sandbox_enabled {
            existing.sandbox_enabled = sandbox_enabled;
            existing.status = if sandbox_enabled {
                if existing.status == ProjectRuntimeEnvironmentStatus::Disabled {
                    ProjectRuntimeEnvironmentStatus::Pending
                } else {
                    existing.status
                }
            } else {
                ProjectRuntimeEnvironmentStatus::Disabled
            };
            if !sandbox_enabled {
                existing.sandbox_provider = RuntimeEnvironmentProvider::None;
                existing.file_provider = RuntimeEnvironmentProvider::None;
                existing.last_error = None;
            }
            existing.updated_at = now_rfc3339();
            let saved = store.upsert_project_runtime_environment(&existing).await?;
            if !sandbox_enabled {
                store
                    .replace_project_runtime_environment_images(project.id.as_str(), &[])
                    .await?;
            }
            return Ok(saved);
        }
        return Ok(existing);
    }
    let environment = default_runtime_environment_for_project(project, sandbox_enabled);
    store.upsert_project_runtime_environment(&environment).await
}

pub fn default_runtime_environment_for_project(
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> ProjectRuntimeEnvironmentRecord {
    let sandbox_enabled = sandbox_enabled.unwrap_or(true);
    let now = now_rfc3339();
    ProjectRuntimeEnvironmentRecord {
        project_id: project.id.clone(),
        status: if sandbox_enabled {
            ProjectRuntimeEnvironmentStatus::Pending
        } else {
            ProjectRuntimeEnvironmentStatus::Disabled
        },
        sandbox_enabled,
        sandbox_provider: RuntimeEnvironmentProvider::None,
        file_provider: RuntimeEnvironmentProvider::None,
        analysis_summary: None,
        not_runnable_reason: None,
        detected_stack: empty_object(),
        required_services: empty_array(),
        env_vars: empty_object(),
        environment_variables: Vec::new(),
        generated_config_files: Vec::new(),
        last_agent_run_id: None,
        last_error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

pub fn refresh_environment_variable_values(environment: &mut ProjectRuntimeEnvironmentRecord) {
    environment.environment_variables = normalize_environment_variable_records(
        std::mem::take(&mut environment.environment_variables),
        &environment.env_vars,
    );
    environment.env_vars = effective_environment_variables(&environment.environment_variables);
}

pub fn program_generated_runtime_analysis_summary(
    environment: &ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> String {
    match environment.status {
        ProjectRuntimeEnvironmentStatus::Analyzing => {
            return "正在分析项目技术栈和运行环境需求。".to_string();
        }
        ProjectRuntimeEnvironmentStatus::NotRunnable => {
            return "未识别到可自动初始化的应用或基础设施入口。".to_string();
        }
        ProjectRuntimeEnvironmentStatus::Failed => {
            return "项目技术分析未能完成。".to_string();
        }
        _ => {}
    }

    let application_count = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Application)
        .count();
    let dependency_count = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Dependency)
        .count();
    let config_file_count = environment.generated_config_files.len();
    let missing_variables = environment
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
        .collect::<Vec<_>>();
    let base = format!(
        "已识别 {application_count} 个应用组件和 {dependency_count} 个依赖服务，生成 {config_file_count} 个环境配置文件及项目级 Compose 计划"
    );
    match environment.status {
        ProjectRuntimeEnvironmentStatus::PendingImageBuild => {
            format!("{base}，等待生成应用镜像。")
        }
        ProjectRuntimeEnvironmentStatus::PendingConfiguration if missing_variables.is_empty() => {
            format!("{base}，仍需补充必填运行参数。")
        }
        ProjectRuntimeEnvironmentStatus::PendingConfiguration => format!(
            "{base}，仍需补充 {} 个必填运行参数：{}。",
            missing_variables.len(),
            missing_variables.join(", ")
        ),
        ProjectRuntimeEnvironmentStatus::Ready => format!("{base}，运行环境已就绪。"),
        _ => format!("{base}。"),
    }
}

pub fn replace_legacy_internal_routing_summary(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> bool {
    let is_legacy_internal_summary = matches!(
        environment.analysis_summary.as_deref(),
        Some("云端项目只通过 Harness MCP 读取文件，并只使用云端 Sandbox Manager。")
            | Some(
                "本地项目将通过 Local Connector 文件 MCP 读取文件，并按本地沙箱可用性选择沙箱镜像 MCP。"
            )
    );
    if !is_legacy_internal_summary {
        return false;
    }
    environment.analysis_summary = Some(program_generated_runtime_analysis_summary(
        environment,
        images,
    ));
    environment.updated_at = now_rfc3339();
    true
}

pub fn enforce_project_runtime_boundary(
    execution_plane: ProjectExecutionPlane,
    environment: &mut ProjectRuntimeEnvironmentRecord,
    images: &mut [ProjectRuntimeEnvironmentImageRecord],
) -> bool {
    let mut changed = false;
    if execution_plane == ProjectExecutionPlane::Cloud && environment.sandbox_enabled {
        if environment.sandbox_provider != RuntimeEnvironmentProvider::CloudSandboxManager {
            environment.sandbox_provider = RuntimeEnvironmentProvider::CloudSandboxManager;
            changed = true;
        }
        if environment.file_provider != RuntimeEnvironmentProvider::Harness {
            environment.file_provider = RuntimeEnvironmentProvider::Harness;
            changed = true;
        }
    }

    let mut application_image_reset = false;
    for image in images {
        let mut image_changed = apply_program_managed_image_policy(image);
        let wrong_provider = execution_plane == ProjectExecutionPlane::Cloud
            && image.image_provider != RuntimeEnvironmentProvider::CloudSandboxManager;
        if wrong_provider {
            image.image_provider = RuntimeEnvironmentProvider::CloudSandboxManager;
            changed = true;
            image_changed = true;
        }
        if wrong_provider && runtime_image_is_application(image) {
            image.image_id = None;
            image.image_ref = None;
            image.status = "planned".to_string();
            image.error = None;
            application_image_reset = true;
            changed = true;
            image_changed = true;
        }
        if image_changed {
            changed = true;
            image.updated_at = now_rfc3339();
        }
    }

    if application_image_reset
        && !matches!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::Disabled
                | ProjectRuntimeEnvironmentStatus::Analyzing
                | ProjectRuntimeEnvironmentStatus::NotRunnable
                | ProjectRuntimeEnvironmentStatus::Failed
        )
    {
        environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
        let missing_variables = environment
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
            .collect::<Vec<_>>();
        environment.analysis_summary = Some(if missing_variables.is_empty() {
            "运行环境分析和 Dockerfile 计划已保留；原有 Local Connector 镜像记录已作废，请执行生成云端镜像。"
                .to_string()
        } else {
            format!(
                "运行环境分析和 Dockerfile 计划已保留；原有 Local Connector 镜像记录已作废，请先执行生成云端镜像。镜像生成后仍需补充运行参数：{}。",
                missing_variables.join(", ")
            )
        });
    }
    if changed {
        environment.updated_at = now_rfc3339();
    }
    changed
}

pub fn apply_program_managed_image_policy(
    image: &mut ProjectRuntimeEnvironmentImageRecord,
) -> bool {
    let service_role = program_managed_service_role(image);
    let service_id = program_managed_service_id_for_role(image, service_role);
    let mcp_policy = match service_role {
        RuntimeServiceRole::Application => ProgramManagedMcpPolicy::application_target(),
        RuntimeServiceRole::Dependency | RuntimeServiceRole::Unknown => {
            ProgramManagedMcpPolicy::default()
        }
    };
    let changed = image.service_id != service_id
        || image.service_role != service_role
        || image.mcp_policy != mcp_policy;
    image.service_id = service_id;
    image.service_role = service_role;
    image.mcp_policy = mcp_policy;
    changed
}

pub fn program_managed_service_id(image: &ProjectRuntimeEnvironmentImageRecord) -> String {
    program_managed_service_id_for_role(image, program_managed_service_role(image))
}

fn program_managed_service_id_for_role(
    image: &ProjectRuntimeEnvironmentImageRecord,
    service_role: RuntimeServiceRole,
) -> String {
    const MAX_SERVICE_ID_LENGTH: usize = 63;

    let mut normalized = String::new();
    let mut previous_separator = false;
    for character in image.environment_key.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !normalized.is_empty() {
            normalized.push('-');
            previous_separator = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        normalized = match service_role {
            RuntimeServiceRole::Application => "application",
            RuntimeServiceRole::Dependency => "dependency",
            RuntimeServiceRole::Unknown => "service",
        }
        .to_string();
    } else if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        let prefix = match service_role {
            RuntimeServiceRole::Application => "app",
            RuntimeServiceRole::Dependency => "dependency",
            RuntimeServiceRole::Unknown => "service",
        };
        normalized = format!("{prefix}-{normalized}");
    }
    normalized.truncate(MAX_SERVICE_ID_LENGTH);
    while normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}

fn program_managed_service_role(
    image: &ProjectRuntimeEnvironmentImageRecord,
) -> RuntimeServiceRole {
    if runtime_image_is_known_dependency(image) {
        return RuntimeServiceRole::Dependency;
    }
    if runtime_image_declares_application(image)
        && image
            .dockerfile
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return RuntimeServiceRole::Application;
    }
    RuntimeServiceRole::Unknown
}

fn runtime_image_is_application(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image.service_role == RuntimeServiceRole::Application
}

fn runtime_image_declares_application(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let identity =
        format!("{} {}", image.environment_key, image.environment_type).to_ascii_lowercase();
    identity.contains("application")
        || identity.contains("runtime")
        || matches!(image.environment_key.as_str(), "app" | "application")
}

fn runtime_image_is_known_dependency(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let identity = format!(
        "{} {} {} {}",
        image.environment_key,
        image.environment_type,
        image.display_name,
        image.image_ref.as_deref().unwrap_or_default(),
    )
    .to_ascii_lowercase();
    [
        "mysql",
        "mariadb",
        "mongodb",
        "mongo:",
        "postgres",
        "redis",
        "nacos",
        "rabbitmq",
        "kafka",
        "elasticsearch",
        "opensearch",
        "minio",
    ]
    .iter()
    .any(|marker| identity.contains(marker))
}

pub fn normalize_environment_variable_records(
    records: Vec<ProjectRuntimeEnvironmentVariableRecord>,
    legacy_env_vars: &Value,
) -> Vec<ProjectRuntimeEnvironmentVariableRecord> {
    let mut by_name = BTreeMap::<String, ProjectRuntimeEnvironmentVariableRecord>::new();
    for mut record in records {
        let Some(name) = normalize_environment_variable_name(record.name.as_str()) else {
            continue;
        };
        record.name = name.clone();
        record.description = normalize_optional_text(record.description);
        record.recommendation_reason = normalize_optional_text(record.recommendation_reason);
        record.project_value = normalize_optional_value(record.project_value);
        record.recommended_value = normalize_optional_value(record.recommended_value);
        record.user_value = record.user_value.map(|value| value.trim().to_string());
        refresh_environment_variable_record(&mut record);
        by_name.insert(name, record);
    }
    if let Some(legacy) = legacy_env_vars.as_object() {
        for (name, value) in legacy {
            let Some(name) = normalize_environment_variable_name(name) else {
                continue;
            };
            let value = scalar_to_string(value);
            by_name.entry(name.clone()).or_insert_with(|| {
                let mut record = ProjectRuntimeEnvironmentVariableRecord {
                    name,
                    project_value: None,
                    project_value_suitable: false,
                    recommended_value: value,
                    user_value: None,
                    effective_value: None,
                    effective_source: RuntimeEnvironmentVariableSource::None,
                    description: Some("由历史运行环境配置迁移".to_string()),
                    recommendation_reason: Some(
                        "历史记录未保存来源，作为 AI 推荐值保留".to_string(),
                    ),
                    required: false,
                    secret: false,
                };
                record.secret = environment_variable_name_is_secret(record.name.as_str());
                refresh_environment_variable_record(&mut record);
                record
            });
        }
    }
    by_name
        .into_values()
        .filter(|record| {
            record.project_value.is_some()
                || record.recommended_value.is_some()
                || record.user_value.is_some()
        })
        .collect()
}

pub fn effective_environment_variables(
    records: &[ProjectRuntimeEnvironmentVariableRecord],
) -> Value {
    let mut values = Map::new();
    for record in records {
        if let Some(value) = record.effective_value.as_deref() {
            values.insert(record.name.clone(), Value::String(value.to_string()));
        }
    }
    Value::Object(values)
}

pub fn apply_environment_variable_overrides(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    overrides: Vec<ProjectRuntimeEnvironmentVariableOverride>,
) -> Result<(), String> {
    let mut records = normalize_environment_variable_records(
        std::mem::take(&mut environment.environment_variables),
        &environment.env_vars,
    );
    for record in &mut records {
        record.user_value = None;
    }
    let mut seen = BTreeSet::new();
    for item in overrides {
        let name = normalize_environment_variable_name(item.name.as_str())
            .ok_or_else(|| format!("invalid environment variable name: {}", item.name))?;
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate environment variable name: {name}"));
        }
        let value = item.value.trim().to_string();
        if let Some(record) = records.iter_mut().find(|record| record.name == name) {
            record.user_value = Some(value);
        } else {
            records.push(ProjectRuntimeEnvironmentVariableRecord {
                name: name.clone(),
                project_value: None,
                project_value_suitable: false,
                recommended_value: None,
                user_value: Some(value),
                effective_value: None,
                effective_source: RuntimeEnvironmentVariableSource::None,
                description: Some("用户自定义环境变量".to_string()),
                recommendation_reason: None,
                required: false,
                secret: environment_variable_name_is_secret(name.as_str()),
            });
        }
    }
    for record in &mut records {
        refresh_environment_variable_record(record);
    }
    environment.environment_variables = records
        .into_iter()
        .filter(|record| {
            record.project_value.is_some()
                || record.recommended_value.is_some()
                || record.user_value.is_some()
        })
        .collect();
    environment.env_vars = effective_environment_variables(&environment.environment_variables);
    Ok(())
}

pub fn refresh_environment_variable_record(record: &mut ProjectRuntimeEnvironmentVariableRecord) {
    let (value, source) = if let Some(value) = record.user_value.clone() {
        (Some(value), RuntimeEnvironmentVariableSource::User)
    } else if record.project_value_suitable && record.project_value.is_some() {
        (
            record.project_value.clone(),
            RuntimeEnvironmentVariableSource::Project,
        )
    } else if record.recommended_value.is_some() {
        (
            record.recommended_value.clone(),
            RuntimeEnvironmentVariableSource::AiRecommended,
        )
    } else {
        (None, RuntimeEnvironmentVariableSource::None)
    };
    record.effective_value = value;
    record.effective_source = source;
    record.secret = record.secret || environment_variable_name_is_secret(record.name.as_str());
}

pub fn required_environment_variables_are_complete(
    records: &[ProjectRuntimeEnvironmentVariableRecord],
) -> bool {
    records.iter().all(|record| {
        !record.required
            || record
                .effective_value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    })
}

pub fn normalize_environment_variable_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 256
        || !value.chars().enumerate().all(|(index, character)| {
            character == '_'
                || character.is_ascii_alphanumeric() && (index > 0 || !character.is_ascii_digit())
        })
    {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

pub fn environment_variable_name_is_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "credential",
        "private",
        "access_key",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
include!("runtime_environment.test.rs");
