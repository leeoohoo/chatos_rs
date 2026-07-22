// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::local_runtime::{
    LocalEnvironmentProgressRecord, LocalRuntimeEnvironmentImageRecord,
    LocalRuntimeEnvironmentRecord,
};

pub(super) fn environment_response(
    environment: &LocalRuntimeEnvironmentRecord,
    images: &[LocalRuntimeEnvironmentImageRecord],
) -> Value {
    let detected_stack = parse_json(environment.detected_stack_json.as_str());
    let fallback_dockerfile =
        crate::local_runtime::environment::prompt::fallback_dockerfile(&detected_stack);
    json!({
        "environment": {
            "project_id": environment.project_id,
            "status": environment.status,
            "sandbox_enabled": environment.sandbox_enabled,
            "sandbox_provider": environment.sandbox_provider,
            "file_provider": environment.file_provider,
            "analysis_summary": environment.analysis_summary,
            "not_runnable_reason": environment.not_runnable_reason,
            "detected_stack": detected_stack,
            "required_services": parse_json(environment.required_services_json.as_str()),
            "env_vars": parse_json(environment.env_vars_json.as_str()),
            "generated_config_files": parse_json(environment.generated_config_files_json.as_str()),
            "last_agent_run_id": environment.last_agent_run_id,
            "last_error": environment.last_error,
            "created_at": environment.created_at,
            "updated_at": environment.updated_at,
        },
        "images": images.iter().map(|image| image_response(image, fallback_dockerfile.as_str())).collect::<Vec<_>>(),
    })
}

pub(super) fn progress_response(progress: &LocalEnvironmentProgressRecord) -> Value {
    json!({
        "project_id": progress.project_id,
        "run_id": progress.run_id,
        "phase": progress.phase,
        "status": progress.status,
        "progress_percent": progress.progress_percent,
        "provider": progress.provider,
        "started_at": progress.started_at,
        "updated_at": progress.updated_at,
        "finished_at": progress.finished_at,
        "logs": progress.logs,
        "error": progress.error,
    })
}

pub(super) fn idle_progress_response(project_id: &str) -> Value {
    json!({
        "project_id": project_id,
        "phase": "idle",
        "status": "idle",
        "provider": "local_connector",
        "progress_percent": 0,
        "logs": "",
    })
}

fn image_response(image: &LocalRuntimeEnvironmentImageRecord, fallback_dockerfile: &str) -> Value {
    let dockerfile = image.dockerfile.as_deref().or_else(|| {
        image
            .environment_type
            .trim()
            .eq_ignore_ascii_case("application")
            .then_some(fallback_dockerfile)
    });
    let service_role = program_managed_service_role(image, dockerfile);
    let service_id = program_managed_service_id(image.environment_key.as_str(), service_role);
    let mcp_policy = if service_role == "application" {
        json!({
            "managed_by": "system",
            "attachment": "project_gateway_target",
            "filesystem": true,
            "terminal": true,
        })
    } else {
        json!({
            "managed_by": "system",
            "attachment": "none",
            "filesystem": false,
            "terminal": false,
        })
    };
    json!({
        "id": image.id,
        "project_id": image.project_id,
        "environment_key": image.environment_key,
        "environment_type": image.environment_type,
        "display_name": image.display_name,
        "service_id": service_id,
        "service_role": service_role,
        "mcp_policy": mcp_policy,
        "image_id": image.image_id,
        "image_ref": image.image_ref,
        "image_provider": image.image_provider,
        "dockerfile": dockerfile,
        "features": parse_json(image.features_json.as_str()),
        "ports": parse_json(image.ports_json.as_str()),
        "env_vars": parse_json(image.env_vars_json.as_str()),
        "status": image.status,
        "error": image.error,
        "created_at": image.created_at,
        "updated_at": image.updated_at,
    })
}

pub(super) fn program_managed_service_id(environment_key: &str, service_role: &str) -> String {
    const MAX_SERVICE_ID_LENGTH: usize = 63;

    let mut normalized = String::new();
    let mut previous_separator = false;
    for character in environment_key.trim().chars() {
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
            "application" => "application",
            "dependency" => "dependency",
            _ => "service",
        }
        .to_string();
    } else if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        let prefix = match service_role {
            "application" => "app",
            "dependency" => "dependency",
            _ => "service",
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
    image: &LocalRuntimeEnvironmentImageRecord,
    dockerfile: Option<&str>,
) -> &'static str {
    let identity = format!(
        "{} {} {} {}",
        image.environment_key,
        image.environment_type,
        image.display_name,
        image.image_ref.as_deref().unwrap_or_default(),
    )
    .to_ascii_lowercase();
    if [
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
    {
        return "dependency";
    }
    let declared_application = image
        .environment_type
        .trim()
        .eq_ignore_ascii_case("application")
        || image
            .environment_type
            .trim()
            .eq_ignore_ascii_case("runtime")
        || matches!(
            image.environment_key.trim().to_ascii_lowercase().as_str(),
            "app" | "application" | "application_runtime"
        );
    if declared_application && dockerfile.is_some_and(|value| !value.trim().is_empty()) {
        "application"
    } else {
        "unknown"
    }
}

fn parse_json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::program_managed_service_id;

    #[test]
    fn local_service_ids_match_program_managed_compose_constraints() {
        assert_eq!(
            program_managed_service_id("services/API Worker", "application"),
            "services-api-worker"
        );
        let service_id = program_managed_service_id(
            "123/a deliberately very long application component name that exceeds compose limits",
            "application",
        );
        assert!(service_id.starts_with("app-123-a-deliberately"));
        assert!(service_id.len() <= 63);
        assert!(!service_id.ends_with('-'));
    }
}
