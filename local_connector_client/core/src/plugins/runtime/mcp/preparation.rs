// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chatos_mcp_runtime::{extract_tools, parse_tool_definition, McpStdioServer};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, PluginComponentKind, PluginMcpServer,
};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::super::super::oauth_broker::PluginOAuthBroker;
use super::super::credentials::{
    PluginCredentialBindings, PluginHttpHeaderTemplates, PluginStdioEnvironmentTemplates,
};
use super::super::sandbox::PluginStdioSandboxLauncher;
use super::{
    PluginMcpSnapshot, PreparedPluginMcpTransport, MAX_MCP_TOOLS, MAX_MCP_TOOL_SNAPSHOT_BYTES,
};
use crate::plugins::{ActivePluginInstallation, PluginCredentialVault, PluginInstaller};

pub(super) fn validate_required_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    for requirement in installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.required
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
    {
        if !permission_snapshot.contains(requirement.permission.as_str()) {
            bail!(
                "Plugin MCP required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

pub(super) fn prepare_transport(
    installation: &ActivePluginInstallation,
    server: &PluginMcpServer,
    adapter_session_id: &str,
    owner_user_id: &str,
    device_id: &str,
    credential_component_key: &str,
    permission_snapshot: &BTreeSet<String>,
    stdio_execution_enabled: bool,
    stdio_sandbox_launcher: Option<&PluginStdioSandboxLauncher>,
    stdio_unavailable_reason: &str,
    credential_vault: Option<PluginCredentialVault>,
    oauth_broker: Option<PluginOAuthBroker>,
) -> Result<PreparedPluginMcpTransport> {
    match server {
        PluginMcpServer::ConfigFile { .. } => {
            bail!("Plugin MCP config-file components are not supported yet")
        }
        PluginMcpServer::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } => {
            if !stdio_execution_enabled {
                bail!(stdio_unavailable_reason.to_string());
            }
            if !permission_snapshot.contains("process.spawn") {
                bail!("Plugin stdio MCP requires process.spawn in the permission snapshot");
            }
            let environment = PluginStdioEnvironmentTemplates::parse(env)?;
            if !environment.secret_names().is_empty() {
                validate_credential_permission(
                    installation,
                    credential_component_key,
                    permission_snapshot,
                )?;
            }
            let credential_bindings = PluginCredentialBindings::prepare(
                credential_vault,
                owner_user_id,
                device_id,
                installation.plugin_id.as_str(),
                installation.version.release_id.as_str(),
                credential_component_key,
                environment.secret_names(),
            )?;
            validate_arguments(args)?;
            let command = resolve_signed_command(installation, command)?;
            let cwd = resolve_cwd(installation, cwd.as_ref().map(|path| path.path.as_str()))?;
            let server_name = format!(
                "plugin:{}:{}:{}",
                installation.plugin_id,
                installation.version.release_id,
                server.component_key()
            );
            let server = McpStdioServer::new(server_name, command.to_string_lossy().into_owned())
                .with_args(args.clone())
                .with_cwd(cwd.to_string_lossy().into_owned())
                .with_user_id(format!("{owner_user_id}:{device_id}:{adapter_session_id}"));
            let (server, sandbox_runtime) = match stdio_sandbox_launcher {
                Some(launcher) => {
                    let (server, runtime) = launcher.prepare(
                        installation
                            .installation_path
                            .parent()
                            .unwrap_or(installation.installation_path.as_path()),
                        installation.installation_path.as_path(),
                        &server,
                        environment.variable_names(),
                        &installation.version.package_file_sha256,
                    )?;
                    (server, Some(runtime))
                }
                None => (server, None),
            };
            Ok(PreparedPluginMcpTransport::Stdio {
                server,
                environment,
                credential_bindings,
                cancellation: CancellationToken::new(),
                _sandbox_runtime: sandbox_runtime,
            })
        }
        PluginMcpServer::Http {
            url,
            headers,
            oauth_resource,
            connect_timeout_ms,
            ..
        } => {
            validate_http_permission(url, permission_snapshot)?;
            let header_templates = PluginHttpHeaderTemplates::parse(headers)?;
            if !header_templates.secret_names().is_empty() {
                validate_credential_permission(
                    installation,
                    credential_component_key,
                    permission_snapshot,
                )?;
            }
            let credential_bindings = PluginCredentialBindings::prepare(
                credential_vault,
                owner_user_id,
                device_id,
                installation.plugin_id.as_str(),
                installation.version.release_id.as_str(),
                credential_component_key,
                header_templates.secret_names(),
            )?;
            let oauth_binding = match oauth_resource.as_deref() {
                Some(resource) => {
                    if header_templates.contains("authorization") {
                        bail!(
                            "Plugin HTTP MCP cannot combine oauth_resource with an Authorization header template"
                        );
                    }
                    let broker = oauth_broker
                        .context("Plugin OAuth HTTP MCP requires the local OAuth Broker")?;
                    let binding = broker.prepare_token_binding(
                        owner_user_id,
                        device_id,
                        installation.plugin_id.as_str(),
                        installation.version.release_id.as_str(),
                        resource,
                    )?;
                    validate_oauth_permissions(
                        installation,
                        credential_component_key,
                        binding.provider(),
                        binding.scopes(),
                        permission_snapshot,
                    )?;
                    Some(Box::new(binding))
                }
                None => None,
            };
            Ok(PreparedPluginMcpTransport::Http {
                url: url.clone(),
                headers: header_templates,
                credential_bindings,
                oauth_binding,
                cancellation: CancellationToken::new(),
                timeout: Duration::from_millis(
                    connect_timeout_ms.unwrap_or(30_000).clamp(300, 120_000),
                ),
            })
        }
    }
}

fn resolve_signed_command(
    installation: &ActivePluginInstallation,
    command: &str,
) -> Result<PathBuf> {
    if !command.contains('/') {
        bail!("reviewed Plugin MCP command identifiers are not enabled yet");
    }
    let relative = normalize_plugin_relative_path(command)
        .map_err(|message| anyhow!("invalid Plugin MCP command path: {message}"))?;
    let relative = relative.trim_start_matches("./");
    if !installation
        .version
        .package_file_sha256
        .contains_key(relative)
    {
        bail!("Plugin MCP command is not covered by package checksums");
    }
    let path = installation.installation_path.join(relative);
    let metadata = fs::symlink_metadata(path.as_path()).context("read Plugin MCP command")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Plugin MCP command is not a safe regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("Plugin MCP command is not executable");
        }
    }
    Ok(path)
}

fn resolve_cwd(installation: &ActivePluginInstallation, cwd: Option<&str>) -> Result<PathBuf> {
    let Some(cwd) = cwd else {
        return Ok(installation.installation_path.clone());
    };
    let relative = normalize_plugin_relative_path(cwd)
        .map_err(|message| anyhow!("invalid Plugin MCP cwd: {message}"))?;
    let path = installation
        .installation_path
        .join(relative.trim_start_matches("./"));
    let metadata = fs::symlink_metadata(path.as_path()).context("read Plugin MCP cwd")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Plugin MCP cwd is not a safe directory");
    }
    Ok(path)
}

fn validate_arguments(args: &[String]) -> Result<()> {
    if args.len() > 128 {
        bail!("Plugin MCP command has too many arguments");
    }
    if args.iter().any(|arg| {
        arg.len() > 8 * 1024
            || arg.contains('\0')
            || matches!(arg.as_str(), "-c" | "--eval" | "--execute")
    }) {
        bail!("Plugin MCP command contains an unsafe or oversized argument");
    }
    Ok(())
}

fn validate_http_permission(url: &str, permission_snapshot: &BTreeSet<String>) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("parse Plugin MCP HTTP URL")?;
    let host = parsed
        .host_str()
        .context("Plugin MCP HTTP URL is missing a host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|address| address.is_loopback());
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        bail!("Plugin HTTP MCP requires HTTPS, except for loopback development servers");
    }
    let permission = format!("network.domain:{}", host.to_ascii_lowercase());
    if !permission_snapshot.contains(permission.as_str()) {
        bail!("Plugin HTTP MCP requires {permission} in the permission snapshot");
    }
    Ok(())
}

fn validate_credential_permission(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    let declared = installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.components.is_empty()
                || requirement
                    .components
                    .iter()
                    .any(|key| key == component_key)
        })
        .map(|requirement| requirement.permission.as_str())
        .filter(|permission| {
            *permission == "credential.use" || permission.starts_with("credential.use:")
        })
        .collect::<BTreeSet<_>>();
    if declared.is_empty()
        || !declared
            .iter()
            .any(|permission| permission_snapshot.contains(*permission))
    {
        bail!(
            "Plugin MCP credential template requires a signed credential.use permission in the prepared snapshot"
        );
    }
    Ok(())
}

fn validate_oauth_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    provider: &str,
    scopes: &[String],
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    let declared = installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.components.is_empty()
                || requirement
                    .components
                    .iter()
                    .any(|key| key == component_key)
        })
        .map(|requirement| requirement.permission.as_str())
        .collect::<BTreeSet<_>>();
    for scope in scopes {
        let permission = format!("oauth.scope:{provider}:{scope}");
        if !declared.contains(permission.as_str())
            || !permission_snapshot.contains(permission.as_str())
        {
            bail!("Plugin OAuth MCP requires signed permission: {permission}");
        }
    }
    Ok(())
}

pub(super) fn sanitize_tools(
    response: Value,
    tool_allowlist: &BTreeSet<String>,
    tool_blocklist: &BTreeSet<String>,
) -> Result<Vec<Value>> {
    let mut tools = extract_tools(&response)
        .map_err(anyhow::Error::msg)?
        .into_iter()
        .filter_map(|tool| parse_tool_definition(&tool).map(|parsed| (parsed.name, tool)))
        .filter(|(name, _)| {
            (tool_allowlist.is_empty() || tool_allowlist.contains(name))
                && !tool_blocklist.contains(name)
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left.0.cmp(&right.0));
    tools.dedup_by(|left, right| left.0 == right.0);
    if tools.is_empty() {
        bail!("Plugin MCP tools/list returned no permitted valid tools");
    }
    if tools.len() > MAX_MCP_TOOLS {
        bail!("Plugin MCP tool catalog exceeds the tool count limit");
    }
    let tools = tools.into_iter().map(|(_, tool)| tool).collect::<Vec<_>>();
    if serde_json::to_vec(&tools)?.len() > MAX_MCP_TOOL_SNAPSHOT_BYTES {
        bail!("Plugin MCP tool snapshot exceeds the byte limit");
    }
    Ok(tools)
}

pub(super) fn validate_active_mcp_snapshot(
    installer: &PluginInstaller,
    snapshot: &PluginMcpSnapshot,
) -> Result<()> {
    let installation = installer
        .active_installation(snapshot.plugin_id.as_str())?
        .context("Plugin is no longer installed and active")?;
    if installation.version.release_id != snapshot.release_id
        || installation.version.version != snapshot.version
        || installation.version.artifact_sha256 != snapshot.artifact_sha256
        || !installation
            .version
            .inventory
            .components
            .iter()
            .any(|component| {
                component.component_key == snapshot.component_key
                    && component.kind == PluginComponentKind::McpServer
            })
    {
        bail!("Plugin MCP snapshot does not match the active immutable Release");
    }
    Ok(())
}

pub(super) fn mcp_snapshot_sha256(
    installation: &ActivePluginInstallation,
    component_key: &str,
    server_key: &str,
    transport: &str,
    tool_snapshot_sha256: &str,
    credential_snapshot_sha256: Option<&str>,
    oauth_snapshot_sha256: Option<&str>,
) -> String {
    let mut payload = format!(
        "chatos.plugin.mcp.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        installation.plugin_id,
        installation.version.release_id,
        installation.version.version,
        installation.version.artifact_sha256,
        component_key,
        server_key,
        transport,
        tool_snapshot_sha256,
    );
    if let Some(credential_snapshot_sha256) = credential_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(credential_snapshot_sha256);
    }
    if let Some(oauth_snapshot_sha256) = oauth_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(oauth_snapshot_sha256);
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

pub(super) fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

pub(super) fn health_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
