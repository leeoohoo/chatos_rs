// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::compose::*;
use super::super::*;

pub(in crate::services::environment_agent::tool_provider) fn normalize_generated_config_files(
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

pub(in crate::services::environment_agent::tool_provider) fn normalize_generated_config_path(
    value: &str,
) -> Result<String, String> {
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

pub(in crate::services::environment_agent::tool_provider) fn infer_config_format(
    path: &str,
) -> &'static str {
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

pub(in crate::services::environment_agent::tool_provider) fn env_value_to_string(
    value: &Value,
) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(in crate::services::environment_agent::tool_provider) fn image_input_to_record(
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
    } else if let Some(image_ref) = super::super::super::compose_dependency_image_ref(&record) {
        record.image_id = None;
        record.image_ref = Some(image_ref);
        record.status = "ready".to_string();
        record.error = None;
    }
    record
}

pub(in crate::services::environment_agent::tool_provider) fn parse_runtime_environment_status(
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

pub(in crate::services::environment_agent::tool_provider) fn ensure_array(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        empty_array()
    }
}

pub(in crate::services::environment_agent::tool_provider) fn ensure_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        empty_object()
    }
}

pub(in crate::services::environment_agent::tool_provider) fn mcp_tool_result(
    message: impl Into<String>,
    structured: Value,
) -> Value {
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
