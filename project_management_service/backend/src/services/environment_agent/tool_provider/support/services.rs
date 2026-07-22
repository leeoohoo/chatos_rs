// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::compose::*;
use super::super::*;
use super::artifacts::env_value_to_string;

pub(in crate::services::environment_agent::tool_provider) fn environment_has_provisionable_evidence(
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

pub(in crate::services::environment_agent::tool_provider) fn infer_service_kinds_from_environment_variables(
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

pub(in crate::services::environment_agent::tool_provider) fn infer_service_kinds_from_text(
    value: &str,
    kinds: &mut std::collections::BTreeSet<String>,
) {
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

pub(in crate::services::environment_agent::tool_provider) fn ensure_required_service_records(
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

pub(in crate::services::environment_agent::tool_provider) fn validate_environment_image_plans(
    detected_stack: &Value,
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<(), String> {
    let mut missing = Vec::new();
    if stack_requires_application_runtime(detected_stack)
        && !images
            .iter()
            .any(|image| image_plan_is_complete(image) && image_is_application_runtime(image))
    {
        missing.push("application runtime".to_string());
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
    let agent_controlled_dockerfiles = images
        .iter()
        .filter(|image| image_is_application_runtime(image))
        .filter(|image| {
            image
                .dockerfile
                .as_deref()
                .is_some_and(dockerfile_contains_program_managed_mcp_control)
        })
        .map(|image| image.environment_key.clone())
        .collect::<Vec<_>>();
    if missing.is_empty() && invalid_plans.is_empty() && agent_controlled_dockerfiles.is_empty() {
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
    if !agent_controlled_dockerfiles.is_empty() {
        reasons.push(format!(
            "application Dockerfiles attempt to install or configure the program-managed Chat OS MCP Agent: {}",
            agent_controlled_dockerfiles.join(", ")
        ));
    }
    Err(format!(
        "runtime environment composition planning is incomplete: {}. Generate one application record and Dockerfile for every independently deployable code component, and include one service record for every detected dependency; all application and dependency services are grouped under the generated project-level Docker Compose file.",
        reasons.join("; ")
    ))
}

fn dockerfile_contains_program_managed_mcp_control(dockerfile: &str) -> bool {
    let dockerfile = dockerfile.to_ascii_lowercase();
    [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "chat os mcp agent",
        "chatos mcp agent",
        "mcp_token",
        "mcp_port",
        "mcp_image",
        "mcp_command",
        "agent_install_script",
        "agent_injection_mode",
        "/opt/chatos/",
    ]
    .iter()
    .any(|marker| dockerfile.contains(marker))
}
