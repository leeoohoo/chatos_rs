// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chatos_agent_profiles::TaskRunnerExecutionTool;
use chatos_plugin_capability::{SignedPluginManifest, SignedPluginMcpServer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::capability_loader::{StoredLocalCapabilityRecord, StoredLocalMcpComponent};

pub(crate) const MAXIMUM_RUNTIME_ARGUMENTS: usize = 128;
pub(crate) const MAXIMUM_RUNTIME_ARGUMENT_BYTES: usize = 8 * 1024;
pub(crate) const MAXIMUM_RUNTIME_ENVIRONMENT: usize = 128;
pub(crate) const MAXIMUM_RUNTIME_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1024;

pub(crate) fn selected_stdio_server<'a>(
    manifest: &'a SignedPluginManifest,
    component: &StoredLocalMcpComponent,
) -> Result<&'a SignedPluginMcpServer, String> {
    manifest
        .mcp_servers
        .iter()
        .find(|server| server.component_key() == component.component_key)
        .filter(|server| matches!(server, SignedPluginMcpServer::Stdio { .. }))
        .ok_or_else(|| "selected MCP component is absent from the signed manifest".to_string())
}

pub(crate) fn verify_runtime_declaration(
    stored: &StoredLocalMcpComponent,
    manifest_server: &SignedPluginMcpServer,
) -> Result<(), String> {
    let SignedPluginMcpServer::Stdio {
        args,
        env,
        component_key,
        ..
    } = manifest_server
    else {
        return Err("HTTP Plugin components cannot execute in the local stdio runtime".to_string());
    };
    if component_key != &stored.component_key || args != &stored.arguments {
        return Err("stored MCP arguments differ from the signed Plugin manifest".to_string());
    }
    if stored.arguments.len() > MAXIMUM_RUNTIME_ARGUMENTS
        || stored.arguments.iter().any(|argument| {
            argument.len() > MAXIMUM_RUNTIME_ARGUMENT_BYTES || argument.contains('\0')
        })
    {
        return Err("stored MCP arguments exceed the local execution policy".to_string());
    }
    if env.len() != stored.environment.len() {
        return Err("stored MCP environment differs from the signed Plugin manifest".to_string());
    }
    for (name, signed_value) in env {
        let Some(stored_value) = stored.environment.get(name) else {
            return Err(
                "stored MCP environment differs from the signed Plugin manifest".to_string(),
            );
        };
        if signed_value != &format!("${{credential:{}}}", stored_value.credential_name) {
            return Err(
                "stored MCP environment differs from the signed Plugin manifest".to_string(),
            );
        }
    }
    validate_sha256(stored.executable_sha256.as_str())?;
    if stored.tools.is_empty() {
        return Err("selected local MCP component has no frozen tools".to_string());
    }
    let mut names = BTreeSet::new();
    for tool in &stored.tools {
        if tool.name.trim().is_empty()
            || tool.schema.get("type").and_then(Value::as_str) != Some("function")
            || tool.schema.get("name").and_then(Value::as_str) != Some(tool.name.as_str())
            || !names.insert(tool.name.as_str())
        {
            return Err("stored local MCP tool declaration is invalid or duplicated".to_string());
        }
    }
    Ok(())
}

pub(crate) fn verify_resolved_executable_name(
    executable: &Path,
    manifest_server: &SignedPluginMcpServer,
) -> Result<(), String> {
    let SignedPluginMcpServer::Stdio { bin, .. } = manifest_server else {
        return Err("HTTP Plugin components cannot execute in the local stdio runtime".to_string());
    };
    let file_name = executable
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "local Plugin executable has no valid file name".to_string())?;
    #[cfg(windows)]
    let matches = file_name == bin
        || executable
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|stem| stem == bin);
    #[cfg(not(windows))]
    let matches = file_name == bin;
    if matches {
        Ok(())
    } else {
        Err("resolved local Plugin executable does not match the signed bin name".to_string())
    }
}

#[cfg(target_os = "macos")]
pub(crate) const fn current_platform() -> &'static str {
    "macos"
}

#[cfg(windows)]
pub(crate) const fn current_platform() -> &'static str {
    "windows"
}

#[cfg(target_os = "linux")]
pub(crate) const fn current_platform() -> &'static str {
    "linux"
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) const fn current_platform() -> &'static str {
    "unsupported"
}

pub(crate) fn verify_permissions(
    stored: &StoredLocalCapabilityRecord,
    manifest: &SignedPluginManifest,
    selected: &StoredLocalMcpComponent,
) -> Result<(), String> {
    let granted = stored
        .authorization
        .granted_permissions
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let component = selected.component_key.as_str();
    let missing = manifest
        .permissions
        .iter()
        .filter(|permission| {
            permission.required
                && (permission.components.is_empty()
                    || permission.components.iter().any(|key| key == component))
                && !granted.contains(permission.permission.as_str())
        })
        .map(|permission| permission.permission.as_str())
        .next();
    if let Some(permission) = missing {
        return Err(format!(
            "selected local MCP component is missing required permission {permission}"
        ));
    }
    Ok(())
}

pub(crate) fn verify_component_status(
    stored: &StoredLocalCapabilityRecord,
    selected: &StoredLocalMcpComponent,
) -> Result<(), String> {
    let statuses = stored
        .authorization
        .ready_component_keys
        .iter()
        .filter(|key| key.as_str() == selected.component_key)
        .count();
    if statuses != 1 {
        return Err("selected local MCP component is not ready".to_string());
    }
    Ok(())
}

pub(crate) fn run_plugin_snapshot(stored: &StoredLocalCapabilityRecord) -> Value {
    json!({
        "plugin_id": stored.plugin_id,
        "release_id": stored.release.release_id,
        "version": stored.release.version,
        "artifact_sha256": stored.release.artifact_sha256,
        "device_id": stored.device_id,
        "workspace_id": stored.project_id,
        "component_snapshots": stored
            .mcp_components
            .iter()
            .map(|component| {
                let mut runtime = BTreeMap::new();
                runtime.insert("transport".to_string(), Value::String("stdio".to_string()));
                json!({
                    "component_key": component.component_key,
                    "kind": "mcp_server",
                    "content_sha256": component.executable_sha256,
                    "runtime": runtime,
                })
            })
            .collect::<Vec<_>>(),
        "permission_snapshot": stored.authorization.granted_permissions,
        "auth_connection_ids": stored.auth_connection_ids,
    })
}

pub(crate) fn local_server_name(plugin_id: &str, component_key: &str) -> String {
    format!("plugin-{plugin_id}-{component_key}")
}

pub(crate) fn capability_revision(
    plugin_release_snapshot: &Value,
    tools: &[TaskRunnerExecutionTool],
) -> Result<String, String> {
    let encoded = serde_json::to_vec(&json!({
        "plugin_release_snapshot": plugin_release_snapshot,
        "tools": tools,
    }))
    .map_err(|error| format!("failed to serialize frozen local capability revision: {error}"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

pub(crate) fn require_identity(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(format!("local capability {field} is invalid"));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("local Plugin executable SHA-256 is invalid".to_string());
    }
    Ok(())
}

pub(crate) fn validate_environment_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || value.contains('=')
        || value.contains('\0')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("local Plugin environment name is invalid".to_string());
    }
    Ok(())
}

pub(crate) fn validate_environment_value(value: &str) -> Result<(), String> {
    if value.len() > MAXIMUM_RUNTIME_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
        return Err("local Plugin environment value is invalid".to_string());
    }
    Ok(())
}
