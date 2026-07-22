// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::http::StatusCode;

use crate::backend::SandboxEnvironmentServiceSpec;
use crate::error::ApiError;
use crate::models::{
    CreateSandboxEnvironmentLeaseRequest, SandboxEnvironmentMcpPolicy,
    SandboxEnvironmentServiceInput, SandboxEnvironmentServiceRecord,
};

use super::super::images;

pub(super) const MAX_ENVIRONMENT_SERVICES: usize = 64;
const MAX_DOCKERFILE_BYTES: usize = 512 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 512;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1024;

pub(super) struct PreparedEnvironmentService {
    pub(super) input: SandboxEnvironmentServiceInput,
    pub(super) image_ref: String,
}

pub(super) fn backend_environment_service_spec(
    service: &PreparedEnvironmentService,
) -> SandboxEnvironmentServiceSpec {
    SandboxEnvironmentServiceSpec {
        service_id: service.input.service_id.clone(),
        service_role: service.input.service_role.clone(),
        image: service.image_ref.clone(),
        dockerfile: service.input.dockerfile.clone(),
        environment: service.input.environment.clone(),
        mcp_enabled: service.input.service_role == "application",
    }
}

pub(super) fn ensure_terminal_target(
    service: &SandboxEnvironmentServiceRecord,
) -> Result<(), ApiError> {
    if service.service_role == "application"
        && service.mcp_policy.terminal
        && service.mcp_policy.managed_by == "system"
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "terminal execution is allowed only for system-managed application targets",
    ))
}

pub(super) fn ensure_mcp_target(service: &SandboxEnvironmentServiceRecord) -> Result<(), ApiError> {
    if service.service_role == "application"
        && service.mcp_policy.managed_by == "system"
        && service.mcp_policy.attachment == "project_gateway_target"
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "MCP is allowed only for system-managed application targets",
    ))
}

pub(super) fn validate_environment_identity(
    input: &CreateSandboxEnvironmentLeaseRequest,
) -> Result<(), ApiError> {
    for (name, value) in [
        ("tenant_id", input.tenant_id.as_str()),
        ("user_id", input.user_id.as_str()),
        ("project_id", input.project_id.as_str()),
        ("run_id", input.run_id.as_str()),
        ("workspace_root", input.workspace_root.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(format!("{name} is required")));
        }
    }
    Ok(())
}

pub(super) fn environment_backend_error(message: impl Into<String>) -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_GATEWAY,
        "sandbox_environment_backend_error",
        message,
    )
}

pub(super) fn validate_service_id(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 63
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        || !value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Err(ApiError::bad_request(format!(
            "invalid environment service_id: {value}"
        )));
    }
    Ok(())
}

pub(super) fn validate_environment_values(
    environment: &BTreeMap<String, String>,
) -> Result<(), ApiError> {
    if environment.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(ApiError::bad_request("too many environment variables"));
    }
    for (name, value) in environment {
        if name.starts_with("CHATOS_SANDBOX_MCP_")
            || matches!(
                name.as_str(),
                "MCP_TOKEN" | "MCP_PORT" | "MCP_IMAGE" | "MCP_COMMAND"
            )
        {
            return Err(ApiError::bad_request(format!(
                "program-managed MCP environment variable cannot be supplied: {name}"
            )));
        }
        if value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
            return Err(ApiError::bad_request(format!(
                "invalid environment variable value: {name}"
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_application_service(
    input: &SandboxEnvironmentServiceInput,
) -> Result<(), ApiError> {
    if input.mcp_policy
        != (SandboxEnvironmentMcpPolicy {
            managed_by: "system".to_string(),
            attachment: "project_gateway_target".to_string(),
            filesystem: true,
            terminal: true,
        })
    {
        return Err(ApiError::bad_request(
            "application service must use the system-managed project gateway target policy",
        ));
    }
    let dockerfile = input
        .dockerfile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("application Dockerfile is required"))?;
    if dockerfile.len() > MAX_DOCKERFILE_BYTES {
        return Err(ApiError::bad_request("application Dockerfile is too large"));
    }
    if dockerfile_contains_agent_control(dockerfile) {
        return Err(ApiError::bad_request(
            "application Dockerfile cannot install or configure the program-managed MCP Agent",
        ));
    }
    if input.image_id.as_deref().is_none_or(str::is_empty) {
        return Err(ApiError::bad_request(
            "application image_id is required as the program-managed Agent source image",
        ));
    }
    Ok(())
}

pub(super) fn validate_dependency_service(
    input: &SandboxEnvironmentServiceInput,
) -> Result<(), ApiError> {
    if input.mcp_policy != SandboxEnvironmentMcpPolicy::default() {
        return Err(ApiError::bad_request(
            "dependency service must not receive MCP policy or Agent access",
        ));
    }
    if input
        .dockerfile
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(ApiError::bad_request(
            "dependency service cannot provide a Dockerfile",
        ));
    }
    let image_ref = input.image_ref.as_deref().unwrap_or_default();
    if !images::known_dependency_image_ref(image_ref) {
        return Err(ApiError::bad_request(format!(
            "dependency image_ref is not a platform-managed image: {image_ref}"
        )));
    }
    Ok(())
}

pub(super) fn dockerfile_contains_agent_control(dockerfile: &str) -> bool {
    let dockerfile = dockerfile.to_ascii_lowercase();
    [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "mcp_token",
        "mcp_port",
        "agent_install_script",
        "agent_injection_mode",
        "/opt/chatos/",
    ]
    .iter()
    .any(|marker| dockerfile.contains(marker))
}

pub(super) fn resolve_primary_service_id(
    requested: Option<&str>,
    services: &[PreparedEnvironmentService],
) -> Result<String, ApiError> {
    let applications = services
        .iter()
        .filter(|service| service.input.service_role == "application")
        .collect::<Vec<_>>();
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if applications
            .iter()
            .any(|service| service.input.service_id == requested)
        {
            return Ok(requested.to_string());
        }
        return Err(ApiError::bad_request(format!(
            "primary_service_id is not an application service: {requested}"
        )));
    }
    if applications.len() == 1 {
        return Ok(applications[0].input.service_id.clone());
    }
    Err(ApiError::bad_request(
        "primary_service_id is required when multiple application services exist",
    ))
}
