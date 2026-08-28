// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chatos_mcp_runtime::{extract_tools, parse_tool_definition, McpStdioServer};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, PluginComponentKind, PluginMcpServer,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::super::super::oauth_broker::PluginOAuthBroker;
use super::super::credentials::{
    PluginCredentialBindings, PluginHttpHeaderTemplates, PluginStdioEnvironmentTemplates,
};
use super::{
    PluginMcpPermissionRule, PluginMcpSnapshot, PluginMcpToolPolicy, PreparedPluginMcpTransport,
    MAX_MCP_TOOLS, MAX_MCP_TOOL_SNAPSHOT_BYTES, MAX_PLUGIN_MCP_TOOL_TIMEOUT_MS,
};
use crate::plugins::{ActivePluginInstallation, PluginCredentialVault, PluginInstaller};

pub(super) fn validate_required_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    let expected = granted_permissions_for_component(installation, component_key);
    if permission_snapshot != &expected {
        bail!("Plugin MCP permission grant snapshot does not match the active device grants");
    }
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

fn granted_permissions_for_component(
    installation: &ActivePluginInstallation,
    component_key: &str,
) -> BTreeSet<String> {
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
    installation
        .version
        .granted_permissions
        .iter()
        .filter(|permission| declared.contains(permission.as_str()))
        .cloned()
        .collect()
}

pub(super) fn prepare_transport(
    installation: &ActivePluginInstallation,
    server: &PluginMcpServer,
    adapter_session_id: &str,
    owner_user_id: &str,
    device_id: &str,
    workspace_root: Option<&Path>,
    credential_component_key: &str,
    permission_snapshot: &BTreeSet<String>,
    credential_vault: Option<PluginCredentialVault>,
    oauth_broker: Option<PluginOAuthBroker>,
) -> Result<PreparedPluginMcpTransport> {
    match server {
        PluginMcpServer::Stdio {
            component_key,
            bin,
            args,
            env,
            ..
        } => {
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
            let resolved = resolve_npm_bin(installation, bin)?;
            let server_name = format!(
                "plugin:{}:{}:{}",
                installation.plugin_id,
                installation.version.release_id,
                server.component_key()
            );
            let mut launch_args = resolved.prefix_args;
            launch_args.extend(args.clone());
            let runtime_directories =
                prepare_runtime_directories(installation, component_key, adapter_session_id)?;
            let mut runtime_environment = HashMap::from([
                (
                    "CHATOS_PLUGIN_ROOT".to_string(),
                    installation
                        .installation_path
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_DATA_DIR".to_string(),
                    runtime_directories.data.to_string_lossy().into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_CACHE_DIR".to_string(),
                    runtime_directories.cache.to_string_lossy().into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_ARTIFACT_DIR".to_string(),
                    runtime_directories.artifacts.to_string_lossy().into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_FILE_GRANT_DIR".to_string(),
                    runtime_directories
                        .file_grants
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_VISUAL_SESSION_DIR".to_string(),
                    runtime_directories
                        .visual_session
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "CHATOS_PLUGIN_ID".to_string(),
                    installation.plugin_id.clone(),
                ),
                (
                    "CHATOS_PLUGIN_COMPONENT_KEY".to_string(),
                    component_key.to_string(),
                ),
            ]);
            if let Some(workspace_root) = workspace_root {
                runtime_environment.insert(
                    "CHATOS_WORKSPACE".to_string(),
                    workspace_root.to_string_lossy().into_owned(),
                );
            }
            let server = McpStdioServer::new(server_name, resolved.command)
                .with_args(launch_args)
                .with_cwd(
                    installation
                        .installation_path
                        .to_string_lossy()
                        .into_owned(),
                )
                .with_env(runtime_environment)
                .with_user_id(format!("{owner_user_id}:{device_id}:{adapter_session_id}"));
            Ok(PreparedPluginMcpTransport::Stdio {
                server,
                environment,
                credential_bindings,
                artifact_dir: runtime_directories.artifacts,
                file_grant_dir: runtime_directories.file_grants,
                visual_session_dir: runtime_directories.visual_session,
                cancellation: CancellationToken::new(),
            })
        }
        PluginMcpServer::Http {
            url,
            headers,
            oauth_resource,
            connect_timeout_ms,
            ..
        } => {
            if workspace_root.is_some() {
                bail!("Plugin HTTP MCP cannot receive a local workspace binding");
            }
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

struct PluginRuntimeDirectories {
    data: PathBuf,
    cache: PathBuf,
    artifacts: PathBuf,
    file_grants: PathBuf,
    visual_session: PathBuf,
}

fn prepare_runtime_directories(
    installation: &ActivePluginInstallation,
    component_key: &str,
    adapter_session_id: &str,
) -> Result<PluginRuntimeDirectories> {
    let plugin_root = installation
        .installation_path
        .ancestors()
        .nth(3)
        .context("installed Plugin path is outside the expected storage layout")?;
    if installation
        .installation_path
        .strip_prefix(plugin_root.join("installed"))
        .is_err()
    {
        bail!("installed Plugin path is outside the expected storage layout");
    }
    let plugin_key = hex::encode(Sha256::digest(installation.plugin_id.as_bytes()));
    let release_key = hex::encode(Sha256::digest(installation.version.release_id.as_bytes()));
    let session_key = hex::encode(Sha256::digest(adapter_session_id.as_bytes()));
    let data = plugin_root.join("data").join(&plugin_key[..32]);
    let cache = plugin_root
        .join("cache")
        .join(&plugin_key[..32])
        .join(&release_key[..32]);
    let artifacts = plugin_root
        .join("artifacts")
        .join(&plugin_key[..32])
        .join(&release_key[..32])
        .join(&session_key[..32]);
    let file_grants = plugin_root
        .join("file-grants")
        .join(&plugin_key[..32])
        .join(&release_key[..32])
        .join(&session_key[..32]);
    let visual_session = plugin_root
        .join("visual-sessions")
        .join(&plugin_key[..32])
        .join(&release_key[..32])
        .join(&session_key[..32]);
    for path in [&data, &cache, &artifacts, &file_grants, &visual_session] {
        create_private_directory_tree(plugin_root, path)?;
    }
    let host_metadata = serde_json::json!({
        "protocol_version": 1,
        "adapter_session_id": adapter_session_id,
        "plugin_id": installation.plugin_id,
        "component_key": component_key,
    });
    fs::write(
        visual_session.join("host.json"),
        serde_json::to_vec(&host_metadata).context("encode Plugin visual-session host metadata")?,
    )
    .context("write Plugin visual-session host metadata")?;
    Ok(PluginRuntimeDirectories {
        data,
        cache,
        artifacts,
        file_grants,
        visual_session,
    })
}

fn create_private_directory_tree(plugin_root: &Path, target: &Path) -> Result<()> {
    let relative = target
        .strip_prefix(plugin_root)
        .context("Plugin runtime directory escaped Plugin storage")?;
    let mut current = plugin_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(current.as_path()) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    bail!(
                        "Plugin runtime directory contains a non-directory or symlink: {}",
                        current.display()
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(current.as_path()).with_context(|| {
                    format!("create Plugin runtime directory: {}", current.display())
                })?;
                set_private_directory_permissions(current.as_path())?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect Plugin runtime directory: {}", current.display())
                });
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("secure Plugin runtime directory: {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[derive(Debug, Deserialize)]
struct InstalledNpmPackage {
    name: String,
    bin: InstalledNpmBin,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum InstalledNpmBin {
    One(String),
    Many(std::collections::BTreeMap<String, String>),
}

struct ResolvedNpmBin {
    command: String,
    prefix_args: Vec<String>,
}

fn resolve_npm_bin(installation: &ActivePluginInstallation, bin: &str) -> Result<ResolvedNpmBin> {
    let package_json_path = installation.installation_path.join("package.json");
    let package_json =
        fs::read(package_json_path.as_path()).context("read installed package.json")?;
    let package: InstalledNpmPackage =
        serde_json::from_slice(package_json.as_slice()).context("parse installed package.json")?;
    let bins = match package.bin {
        InstalledNpmBin::One(path) => std::collections::BTreeMap::from([(
            package
                .name
                .rsplit('/')
                .next()
                .unwrap_or(package.name.as_str())
                .to_string(),
            path,
        )]),
        InstalledNpmBin::Many(values) => values,
    };
    let declared_path = bins
        .get(bin)
        .with_context(|| format!("installed npm package does not publish bin: {bin}"))?;
    let relative = normalize_plugin_relative_path(declared_path)
        .map_err(|message| anyhow!("invalid npm MCP bin path: {message}"))?;
    let relative = relative.trim_start_matches("./");
    if !installation
        .version
        .package_file_sha256
        .contains_key(relative)
    {
        bail!("npm MCP bin is not covered by package checksums");
    }
    let path = installation.installation_path.join(relative);
    let metadata = fs::symlink_metadata(path.as_path()).context("read npm MCP bin")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("npm MCP bin is not a safe regular file");
    }
    let prefix = fs::read(path.as_path())?
        .into_iter()
        .take(256)
        .collect::<Vec<_>>();
    let node_launcher = matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("js" | "cjs" | "mjs")
    ) || String::from_utf8_lossy(prefix.as_slice())
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("#!") && line.contains("node"));
    if node_launcher {
        return Ok(ResolvedNpmBin {
            command: "node".to_string(),
            prefix_args: vec![path.to_string_lossy().into_owned()],
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("npm MCP native bin is not executable");
        }
    }
    Ok(ResolvedNpmBin {
        command: path.to_string_lossy().into_owned(),
        prefix_args: Vec::new(),
    })
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

pub(super) fn sanitize_server_instructions(
    initialize_response: &Value,
    max_bytes: usize,
) -> Result<Option<String>> {
    let protocol_version = initialize_response
        .get("protocolVersion")
        .and_then(Value::as_str)
        .context("Plugin MCP initialize result is missing protocolVersion")?;
    if !matches!(protocol_version, "2025-06-18" | "2025-03-26" | "2024-11-05") {
        bail!("Plugin MCP initialize returned unsupported protocolVersion: {protocol_version}");
    }
    if !initialize_response
        .get("capabilities")
        .is_some_and(Value::is_object)
    {
        bail!("Plugin MCP initialize result is missing an object capabilities field");
    }
    let Some(instructions) = initialize_response.get("instructions") else {
        return Ok(None);
    };
    let instructions = instructions
        .as_str()
        .context("Plugin MCP initialize instructions must be a string")?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let instructions = instructions.trim();
    if instructions.is_empty() {
        return Ok(None);
    }
    if instructions.len() > max_bytes || instructions.contains('\0') {
        bail!("Plugin MCP initialize instructions are unsafe or exceed the byte limit");
    }
    Ok(Some(instructions.to_string()))
}

pub(super) fn validate_tool_policies(
    installation: &ActivePluginInstallation,
    component_key: &str,
    _permission_snapshot: &BTreeSet<String>,
    tools: &[Value],
) -> Result<()> {
    let declared_permissions = installation
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
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let policy = tool_policy(tool)?;
        for permission in &policy.required_permissions {
            if !declared_permissions.contains(permission.as_str()) {
                bail!("Plugin MCP tool {name} requires undeclared permission: {permission}");
            }
        }
        for rule in &policy.permission_rules {
            for permission in &rule.required_permissions {
                if !declared_permissions.contains(permission.as_str()) {
                    bail!(
                        "Plugin MCP tool {name} permission rule requires undeclared permission: {permission}"
                    );
                }
            }
        }
    }
    Ok(())
}

pub(super) fn tool_policy(tool: &Value) -> Result<PluginMcpToolPolicy> {
    let Some(meta) = tool.get("_meta") else {
        return Ok(PluginMcpToolPolicy::default());
    };
    let meta = meta
        .as_object()
        .context("Plugin MCP tool _meta must be an object")?;
    let has_chatos_policy = meta.keys().any(|key| key.starts_with("chatos/"));
    if !has_chatos_policy {
        return Ok(PluginMcpToolPolicy::default());
    }
    if meta.get("chatos/policyVersion").and_then(Value::as_u64) != Some(1) {
        bail!("Plugin MCP tool has an unsupported chatos/policyVersion");
    }
    let required_permissions = meta
        .get("chatos/requiredPermissions")
        .and_then(Value::as_array)
        .context("Plugin MCP tool policy is missing requiredPermissions")?;
    if required_permissions.len() > 64 {
        bail!("Plugin MCP tool policy declares too many required permissions");
    }
    let mut permissions = BTreeSet::new();
    for permission in required_permissions {
        let permission = permission
            .as_str()
            .map(str::trim)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
            })
            .context("Plugin MCP tool policy contains an invalid required permission")?;
        permissions.insert(permission.to_string());
    }
    let risk_level = meta
        .get("chatos/riskLevel")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "low" | "medium" | "high" | "critical"))
        .context("Plugin MCP tool policy has an invalid riskLevel")?
        .to_string();
    let permission_rules = parse_permission_rules(meta.get("chatos/permissionRules"))?;
    let approval_mode = meta
        .get("chatos/approvalMode")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "none" | "per_call"))
        .context("Plugin MCP tool policy has an invalid approvalMode")?
        .to_string();
    let parallel_safe = meta
        .get("chatos/parallelSafe")
        .and_then(Value::as_bool)
        .context("Plugin MCP tool policy has an invalid parallelSafe")?;
    let timeout_ms = meta
        .get("chatos/timeoutMs")
        .and_then(Value::as_u64)
        .filter(|value| (300..=MAX_PLUGIN_MCP_TOOL_TIMEOUT_MS).contains(value))
        .context("Plugin MCP tool policy has an invalid timeoutMs")?;
    let result_max_chars = meta
        .get("chatos/toolResultMaxChars")
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| {
                    (1..=chatos_mcp_service::TOOL_RESULT_MAX_CHARS_UPPER_BOUND).contains(value)
                })
                .context("Plugin MCP tool policy has an invalid toolResultMaxChars")
        })
        .transpose()?;
    Ok(PluginMcpToolPolicy {
        required_permissions: permissions,
        permission_rules,
        risk_level,
        approval_mode,
        parallel_safe,
        timeout_ms,
        result_max_chars,
    })
}

fn parse_permission_rules(value: Option<&Value>) -> Result<Vec<PluginMcpPermissionRule>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let rules = value
        .as_array()
        .context("Plugin MCP tool permissionRules must be an array")?;
    if rules.len() > 32 {
        bail!("Plugin MCP tool declares too many permissionRules");
    }
    rules
        .iter()
        .map(|rule| {
            let rule = rule
                .as_object()
                .context("Plugin MCP permission rule must be an object")?;
            if rule.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "argumentPointer" | "equals" | "matchWhenMissing" | "requiredPermissions"
                )
            }) {
                bail!("Plugin MCP permission rule contains an unknown field");
            }
            let argument_pointer = rule
                .get("argumentPointer")
                .and_then(Value::as_str)
                .filter(|value| {
                    value.starts_with('/')
                        && value.len() <= 512
                        && !value.chars().any(char::is_control)
                })
                .context("Plugin MCP permission rule argumentPointer is invalid")?
                .to_string();
            let equals = rule
                .get("equals")
                .cloned()
                .context("Plugin MCP permission rule equals value is required")?;
            if equals.is_array() || equals.is_object() {
                bail!("Plugin MCP permission rule equals must be a scalar JSON value");
            }
            let match_when_missing = match rule.get("matchWhenMissing") {
                Some(value) => value
                    .as_bool()
                    .context("Plugin MCP permission rule matchWhenMissing must be a boolean")?,
                None => false,
            };
            let required = rule
                .get("requiredPermissions")
                .and_then(Value::as_array)
                .context("Plugin MCP permission rule requiredPermissions is required")?;
            if required.is_empty() || required.len() > 64 {
                bail!("Plugin MCP permission rule requiredPermissions is invalid");
            }
            let mut required_permissions = BTreeSet::new();
            for permission in required {
                let permission = permission
                    .as_str()
                    .map(str::trim)
                    .filter(|value| {
                        !value.is_empty()
                            && value.len() <= 128
                            && value
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
                    })
                    .context("Plugin MCP permission rule contains an invalid permission")?;
                required_permissions.insert(permission.to_string());
            }
            Ok(PluginMcpPermissionRule {
                argument_pointer,
                equals,
                match_when_missing,
                required_permissions,
            })
        })
        .collect()
}

pub(super) fn validate_active_mcp_snapshot(
    installer: &PluginInstaller,
    snapshot: &PluginMcpSnapshot,
    permission_snapshot: &BTreeSet<String>,
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
    if granted_permissions_for_component(&installation, snapshot.component_key.as_str())
        != *permission_snapshot
    {
        bail!("Plugin MCP permission grants changed after prepare");
    }
    Ok(())
}

pub(super) fn mcp_snapshot_sha256(
    installation: &ActivePluginInstallation,
    component_key: &str,
    server_key: &str,
    transport: &str,
    server_instructions_sha256: &str,
    tool_snapshot_sha256: &str,
    permission_snapshot_sha256: &str,
    workspace_snapshot_sha256: Option<&str>,
    credential_snapshot_sha256: Option<&str>,
    oauth_snapshot_sha256: Option<&str>,
) -> String {
    let mut payload = format!(
        "chatos.plugin.mcp.snapshot.v3\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        installation.plugin_id,
        installation.version.release_id,
        installation.version.version,
        installation.version.artifact_sha256,
        component_key,
        server_key,
        transport,
        server_instructions_sha256,
        tool_snapshot_sha256,
        permission_snapshot_sha256,
    );
    if let Some(credential_snapshot_sha256) = credential_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(credential_snapshot_sha256);
    }
    if let Some(workspace_snapshot_sha256) = workspace_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(workspace_snapshot_sha256);
    }
    if let Some(oauth_snapshot_sha256) = oauth_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(oauth_snapshot_sha256);
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

pub(super) fn workspace_root_sha256(path: &Path) -> String {
    let payload = format!("chatos.plugin.mcp.workspace.v1\n{}", path.to_string_lossy());
    hex::encode(Sha256::digest(payload.as_bytes()))
}

pub(super) fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

pub(super) fn health_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn default_tool_policy_uses_two_hour_timeout() {
        let policy = tool_policy(&json!({
            "name": "slow_local_tool",
            "description": "Long-running local operation",
            "inputSchema": {"type": "object"}
        }))
        .expect("parse default policy");
        assert_eq!(policy.timeout_ms, 2 * 60 * 60 * 1_000);
    }

    #[test]
    fn initialize_instructions_are_normalized_and_preserved() {
        let instructions = sanitize_server_instructions(
            &json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "instructions": "  Observe again.\r\nThen press Return.  "
            }),
            1024,
        )
        .expect("sanitize instructions");
        assert_eq!(
            instructions.as_deref(),
            Some("Observe again.\nThen press Return.")
        );
    }

    #[test]
    fn initialize_instructions_reject_oversized_content() {
        let error = sanitize_server_instructions(
            &json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "instructions": "12345"
            }),
            4,
        )
        .expect_err("oversized instructions must fail");
        assert!(error.to_string().contains("byte limit"));
    }

    #[test]
    fn parameter_conditioned_permission_rules_are_parsed_generically() {
        let policy = tool_policy(&json!({
            "name": "open_session",
            "_meta": {
                "chatos/policyVersion": 1,
                "chatos/requiredPermissions": [],
                "chatos/permissionRules": [
                    {
                        "argumentPointer": "/mode",
                        "equals": "managed",
                        "matchWhenMissing": true,
                        "requiredPermissions": ["example.managed"]
                    },
                    {
                        "argumentPointer": "/mode",
                        "equals": "existing",
                        "requiredPermissions": ["example.attach"]
                    }
                ],
                "chatos/riskLevel": "high",
                "chatos/approvalMode": "per_call",
                "chatos/parallelSafe": false,
                "chatos/timeoutMs": 10000
            }
        }))
        .expect("parse policy");
        assert_eq!(policy.permission_rules.len(), 2);
        assert!(policy.permission_rules[0].match_when_missing);
        assert!(policy.permission_rules[1]
            .required_permissions
            .contains("example.attach"));
    }

    #[test]
    fn permission_rules_reject_unknown_fields() {
        let error = tool_policy(&json!({
            "name": "unsafe",
            "_meta": {
                "chatos/policyVersion": 1,
                "chatos/requiredPermissions": [],
                "chatos/permissionRules": [{
                    "argumentPointer": "/mode",
                    "equals": "managed",
                    "requiredPermissions": ["example.managed"],
                    "browserSpecificBypass": true
                }],
                "chatos/riskLevel": "high",
                "chatos/approvalMode": "per_call",
                "chatos/parallelSafe": false,
                "chatos/timeoutMs": 10000
            }
        }))
        .expect_err("unknown rule field must fail");
        assert!(error.to_string().contains("unknown field"));
    }
}
