// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::{
    ComposeUpRequest, APPLICATION_DOCKERFILE_NAME, MAX_COMPOSE_BYTES, MAX_ENV_BYTES,
    RUNTIME_DIRECTORY,
};

pub(super) fn validate_generated_content(
    input: &ComposeUpRequest,
    application_dockerfiles: &BTreeMap<String, String>,
) -> Result<()> {
    if input.compose_yaml.is_empty() || input.compose_yaml.len() > MAX_COMPOSE_BYTES {
        return Err(anyhow!(
            "generated Docker Compose file is empty or too large"
        ));
    }
    if input.env_file.len() > MAX_ENV_BYTES {
        return Err(anyhow!("generated environment file is too large"));
    }
    validate_compose_source_before_resolution(input.compose_yaml.as_str())?;
    let compose_lower = input.compose_yaml.to_ascii_lowercase();
    for forbidden in [
        "privileged: true",
        "network_mode: host",
        "pid: host",
        "ipc: host",
        "/var/run/docker.sock",
        "cap_add:",
        "devices:",
    ] {
        if compose_lower.contains(forbidden) {
            return Err(anyhow!(
                "generated Docker Compose file contains forbidden setting: {forbidden}"
            ));
        }
    }
    for marker in [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "mcp_token",
        "mcp_port",
        "agent_install_script",
        "agent_injection_mode",
    ] {
        if compose_lower.contains(marker) {
            return Err(anyhow!(
                "generated Docker Compose file cannot install or configure the program-managed Chat OS MCP Agent: {marker}"
            ));
        }
    }
    if !compose_lower.contains("services:") {
        return Err(anyhow!(
            "generated Docker Compose file must contain services"
        ));
    }
    if input.application_dockerfiles.is_empty() {
        if !compose_lower.contains("dockerfile.application") {
            return Err(anyhow!(
                "legacy generated Docker Compose file must reference Dockerfile.application"
            ));
        }
    } else {
        for service_id in application_dockerfiles.keys() {
            let expected = format!(".chatos/runtime-environment/services/{service_id}/dockerfile");
            if !compose_lower.contains(expected.as_str()) {
                return Err(anyhow!(
                    "generated Docker Compose file does not reference the managed Dockerfile for application service {service_id}"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_compose_source_before_resolution(source: &str) -> Result<()> {
    if source.contains('\0') {
        return Err(anyhow!("generated Docker Compose file contains a NUL byte"));
    }
    let mut env_file_block_indent = None;
    for raw_line in source.lines() {
        if raw_line
            .chars()
            .take_while(|character| character.is_whitespace())
            .any(|character| character == '\t')
        {
            return Err(anyhow!(
                "generated Docker Compose file must use spaces for indentation"
            ));
        }
        let indent = raw_line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let unquoted = yaml_line_without_quoted_content(trimmed);
        if unquoted.contains('{')
            || unquoted.contains('}')
            || unquoted.contains('!')
            || unquoted.trim_start().starts_with('?')
            || unquoted.split_whitespace().any(|token| {
                token.starts_with('&') || token.starts_with('*') || token.starts_with("<<:")
            })
        {
            return Err(anyhow!(
                "generated Docker Compose file cannot use YAML flow mappings, anchors, aliases, or merge keys"
            ));
        }

        if let Some(block_indent) = env_file_block_indent {
            if indent > block_indent {
                let value = trimmed
                    .strip_prefix('-')
                    .map(str::trim)
                    .ok_or_else(|| anyhow!("env_file entries must be a simple YAML list"))?;
                validate_env_file_value(value)?;
                continue;
            }
            env_file_block_indent = None;
        }

        let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        if raw_key.contains('\'') || raw_key.contains('"') {
            return Err(anyhow!(
                "generated Docker Compose file cannot use quoted or escaped mapping keys"
            ));
        }
        let key = raw_key
            .trim()
            .trim_matches(['\'', '"'])
            .to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "include" | "extends" | "configs" | "secrets" | "<<"
        ) {
            return Err(anyhow!(
                "generated Docker Compose file contains forbidden directive: {key}"
            ));
        }
        if key == "env_file" {
            let value = raw_value.trim();
            if value.is_empty() {
                env_file_block_indent = Some(indent);
            } else {
                let value = value
                    .strip_prefix('[')
                    .and_then(|value| value.strip_suffix(']'))
                    .unwrap_or(value);
                for entry in value.split(',') {
                    validate_env_file_value(entry)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_env_file_value(value: &str) -> Result<()> {
    let value = value.trim().trim_matches(['\'', '"']).trim();
    if value == ".env.chatos" {
        Ok(())
    } else {
        Err(anyhow!(
            "generated Docker Compose env_file must be exactly .env.chatos"
        ))
    }
}

fn yaml_line_without_quoted_content(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut quote = None;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
            result.push(' ');
            continue;
        }
        if quote == Some('"') && character == '\\' {
            escaped = true;
            result.push(' ');
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            result.push(' ');
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
            result.push(' ');
        } else if character == '#' {
            break;
        } else {
            result.push(character);
        }
    }
    result
}

pub(super) fn validate_normalized_compose(project_root: &Path, compose: &Value) -> Result<()> {
    let root = compose
        .as_object()
        .ok_or_else(|| anyhow!("normalized Docker Compose configuration must be an object"))?;
    let services = root
        .get("services")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("normalized Docker Compose configuration has no services"))?;
    let mut application_build_count = 0usize;
    for (service_name, service) in services {
        let service = service
            .as_object()
            .ok_or_else(|| anyhow!("Docker Compose service {service_name} must be an object"))?;
        for forbidden in [
            "cap_add",
            "cgroup",
            "cgroup_parent",
            "credential_spec",
            "device_cgroup_rules",
            "devices",
            "gpus",
            "ipc",
            "isolation",
            "network_mode",
            "pid",
            "privileged",
            "runtime",
            "security_opt",
            "sysctls",
            "use_api_socket",
            "userns_mode",
            "uts",
            "volumes_from",
        ] {
            if service.get(forbidden).is_some_and(compose_setting_enabled) {
                return Err(anyhow!(
                    "Docker Compose service {service_name} contains forbidden setting: {forbidden}"
                ));
            }
        }
        if let Some(build) = service.get("build") {
            validate_compose_build(project_root, service_name, build)?;
            application_build_count += 1;
        }
        if let Some(volumes) = service.get("volumes") {
            validate_compose_volumes(service_name, volumes)?;
        }
        if let Some(ports) = service.get("ports") {
            validate_compose_ports(service_name, ports)?;
        }
    }
    if application_build_count == 0 {
        return Err(anyhow!(
            "normalized Docker Compose configuration has no managed application build service"
        ));
    }
    for section in ["networks", "volumes"] {
        for (name, resource) in root
            .get(section)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if resource
                .get("external")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(anyhow!(
                    "Docker Compose {section} resource {name} cannot be external"
                ));
            }
        }
    }
    Ok(())
}

fn compose_setting_enabled(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(_) => true,
    }
}

fn validate_compose_build(project_root: &Path, service_name: &str, build: &Value) -> Result<()> {
    let build = build
        .as_object()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} build must be an object"))?;
    if build.get("additional_contexts").is_some() || build.get("dockerfile_inline").is_some() {
        return Err(anyhow!(
            "Docker Compose service {service_name} cannot use additional build contexts or inline Dockerfiles"
        ));
    }
    let context = build
        .get("context")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} build context is missing"))?;
    let context_path = Path::new(context);
    let context_path = if context_path.is_absolute() {
        context_path.to_path_buf()
    } else {
        project_root.join(context_path)
    };
    let context = std::fs::canonicalize(context_path.as_path())
        .with_context(|| format!("resolve Docker Compose build context {context}"))?;
    if !context.starts_with(project_root) {
        return Err(anyhow!(
            "Docker Compose service {service_name} build context escapes the authorized project"
        ));
    }
    if let Some(dockerfile) = build.get("dockerfile").and_then(Value::as_str) {
        let dockerfile = Path::new(dockerfile);
        let dockerfile = if dockerfile.is_absolute() {
            dockerfile.to_path_buf()
        } else {
            context.join(dockerfile)
        };
        let dockerfile = std::fs::canonicalize(dockerfile.as_path()).with_context(|| {
            format!("resolve Docker Compose Dockerfile {}", dockerfile.display())
        })?;
        let managed_service_dockerfile = std::fs::canonicalize(
            project_root
                .join(RUNTIME_DIRECTORY)
                .join("services")
                .join(service_name)
                .join("Dockerfile"),
        )
        .ok();
        let legacy_dockerfile = std::fs::canonicalize(
            project_root
                .join(RUNTIME_DIRECTORY)
                .join(APPLICATION_DOCKERFILE_NAME),
        )
        .ok();
        let managed = managed_service_dockerfile.as_deref() == Some(dockerfile.as_path())
            || (service_name == "application"
                && legacy_dockerfile.as_deref() == Some(dockerfile.as_path()));
        if !dockerfile.starts_with(project_root) || !dockerfile.is_file() || !managed {
            return Err(anyhow!(
                "Docker Compose service {service_name} Dockerfile is not a program-managed runtime artifact"
            ));
        }
    }
    Ok(())
}

fn validate_compose_volumes(service_name: &str, volumes: &Value) -> Result<()> {
    let volumes = volumes
        .as_array()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} volumes must be an array"))?;
    for volume in volumes {
        let volume = volume.as_object().ok_or_else(|| {
            anyhow!("Docker Compose service {service_name} volume must be normalized")
        })?;
        if volume.get("type").and_then(Value::as_str) != Some("volume") {
            return Err(anyhow!(
                "Docker Compose service {service_name} can only use managed named volumes; host bind mounts and named pipes are forbidden"
            ));
        }
        if volume
            .get("source")
            .and_then(Value::as_str)
            .is_none_or(|source| source.trim().is_empty())
        {
            return Err(anyhow!(
                "Docker Compose service {service_name} volume source is missing"
            ));
        }
    }
    Ok(())
}

fn validate_compose_ports(service_name: &str, ports: &Value) -> Result<()> {
    let ports = ports
        .as_array()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} ports must be an array"))?;
    for port in ports {
        let port = port.as_object().ok_or_else(|| {
            anyhow!("Docker Compose service {service_name} port must be normalized")
        })?;
        let host_ip = port
            .get("host_ip")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(host_ip, "127.0.0.1" | "::1") {
            return Err(anyhow!(
                "Docker Compose service {service_name} published ports must bind to loopback"
            ));
        }
    }
    Ok(())
}
