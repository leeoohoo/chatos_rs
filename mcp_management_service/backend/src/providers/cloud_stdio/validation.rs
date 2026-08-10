// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Component, Path};

use super::*;

pub(super) fn prepare_binding(
    resolved: &ResolvedMcp,
    route: &ResolvedMcpRoute,
) -> Result<CloudStdioProviderBinding, String> {
    if resolved.resource.runtime.kind.trim() != "stdio_cloud" {
        return Err("runtime kind is not stdio_cloud".to_string());
    }
    let provider_ref = route
        .provider_ref
        .clone()
        .filter(|value| value.starts_with("sandbox:"))
        .ok_or_else(|| "route has no bound sandbox target".to_string())?;
    let command = resolved
        .resource
        .runtime
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "runtime command is missing".to_string())?;
    validate_command(command, resolved.resource.runtime.args.as_slice())?;
    validate_arguments(resolved.resource.runtime.args.as_slice())?;
    validate_environment(&resolved.resource.runtime.env)?;
    let cwd = normalized_cwd(resolved.resource.runtime.cwd.as_deref())?;
    let allowed_tool_names = configured_tool_names(
        resolved.resource.security.allowed_tool_names.as_slice(),
        "allowed_tool_names",
    )?;
    let blocked_tool_names = configured_tool_names(
        resolved.resource.security.blocked_tool_names.as_slice(),
        "blocked_tool_names",
    )?;
    if !route.allow_writes && allowed_tool_names.is_empty() {
        return Err("read-only cloud stdio MCP requires allowed_tool_names".to_string());
    }
    Ok(CloudStdioProviderBinding {
        provider_ref,
        command: command.to_string(),
        args: resolved.resource.runtime.args.clone(),
        env: resolved.resource.runtime.env.clone(),
        cwd,
        plugin_artifact: None,
        allow_writes: route.allow_writes,
        allowed_tool_names,
        blocked_tool_names,
    })
}

pub(super) fn validate_command(command: &str, args: &[String]) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty()
        || command.len() > MAX_COMMAND_BYTES
        || command
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0'))
        || matches!(command, "." | "..")
    {
        return Err("command must be a PATH-resolved executable name".to_string());
    }
    let shell = command.trim_end_matches(".exe").to_ascii_lowercase();
    let is_shell = matches!(
        shell.as_str(),
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "cmd" | "powershell" | "pwsh"
    );
    let invokes_inline_command = args.iter().any(|arg| {
        matches!(
            arg.trim().to_ascii_lowercase().as_str(),
            "-c" | "/c" | "-command" | "-encodedcommand"
        )
    });
    if is_shell && invokes_inline_command {
        return Err("shell inline command execution is forbidden".to_string());
    }
    Ok(())
}

pub(super) fn validate_plugin_artifact_ref(value: &str) -> Result<(), String> {
    let url =
        reqwest::Url::parse(value).map_err(|_| "Plugin artifact URL is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Plugin artifact URL must use HTTPS without credentials or fragments".to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_arguments(args: &[String]) -> Result<(), String> {
    chatos_mcp_runtime::validate_stdio_arguments(args)
        .map_err(|_| "arguments exceed the supported limits".to_string())
}

pub(super) fn validate_environment(env: &BTreeMap<String, String>) -> Result<(), String> {
    chatos_mcp_runtime::validate_stdio_environment(env).map_err(|error| match error {
        chatos_mcp_runtime::StdioPolicyViolation::EnvironmentLimits => {
            "environment exceeds the supported limits".to_string()
        }
        chatos_mcp_runtime::StdioPolicyViolation::EnvironmentEntry
        | chatos_mcp_runtime::StdioPolicyViolation::Arguments => {
            "environment contains an invalid or Host-controlled entry".to_string()
        }
    })
}

pub(super) fn normalized_cwd(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("cwd must remain relative to the sandbox workspace".to_string());
    }
    Ok(Some(value.to_string()))
}

pub(super) fn configured_tool_names(
    values: &[String],
    field: &str,
) -> Result<HashSet<String>, String> {
    if values.len() > MAX_TOOL_POLICY_ITEMS {
        return Err(format!(
            "{field} exceeds the supported {MAX_TOOL_POLICY_ITEMS} entries"
        ));
    }
    let mut normalized = HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_TOOL_NAME_BYTES {
            return Err(format!("{field} contains an invalid tool name"));
        }
        normalized.insert(value.to_string());
    }
    Ok(normalized)
}
