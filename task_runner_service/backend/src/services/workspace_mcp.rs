// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};
use chatos_mcp_service::{
    builtin_kind_header_value, selected_host_builtin_kind_names, BuiltinHostBackend,
    HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{
    ModelConfigRecord, TaskEphemeralHttpMcpServer, TaskMcpConfig, TaskMcpResolutionResponse,
    TaskRecord, PUBLIC_PROJECT_ID, TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL,
};
use crate::store::AppStore;

use super::mcp_resolution::{
    hosted_builtin_kinds_for, resolve_task_mcp, selected_builtin_kinds_from_config,
    task_mcp_resolution_response as build_mcp_resolution_response,
};
use super::normalize_strings;
use super::normalized_optional;

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";
const LOCAL_CONNECTOR_MCP_SERVER_NAME: &str = "local_connector";
const HARNESS_CODE_MCP_SERVER_NAME: &str = "harness_code";

#[derive(Debug, Clone)]
struct LocalConnectorProjectRef {
    device_id: String,
    workspace_id: String,
    relative_path: Option<String>,
}

pub(super) fn selected_builtin_kinds(mcp_config: &TaskMcpConfig) -> Vec<BuiltinMcpKind> {
    selected_builtin_kinds_from_config(mcp_config)
}

pub(super) fn runtime_selected_builtin_kinds(task: &TaskRecord) -> Vec<BuiltinMcpKind> {
    resolve_task_mcp(task, active_host_backends_for_task(task).as_slice())
        .server_local_builtin_kinds
}

pub(super) fn task_mcp_resolution_response(task: &TaskRecord) -> TaskMcpResolutionResponse {
    build_mcp_resolution_response(task, active_host_backends_for_task(task).as_slice())
}

pub(super) async fn task_with_runtime_mcp_routing(
    config: &AppConfig,
    store: &AppStore,
    mut task: TaskRecord,
) -> Result<TaskRecord, String> {
    if !task.mcp_config.enabled {
        return Ok(task);
    }

    if let Some(project_root) = resolve_project_root_for_task(config, store, &task).await? {
        if parse_local_connector_project_root(project_root.as_str()).is_some() {
            apply_local_connector_runtime_routing_to_task(&mut task, project_root.as_str());
            return Ok(task);
        }
    }

    apply_harness_project_runtime_routing_to_task(config, store, &mut task).await?;
    Ok(task)
}

pub(super) fn task_uses_local_connector(task: &TaskRecord) -> bool {
    task.mcp_config
        .ephemeral_http_servers
        .iter()
        .any(is_local_connector_ephemeral_server)
}

pub(super) fn task_uses_harness_code(task: &TaskRecord) -> bool {
    task.mcp_config
        .ephemeral_http_servers
        .iter()
        .any(is_harness_code_ephemeral_server)
}

fn active_host_backends_for_task(task: &TaskRecord) -> Vec<BuiltinHostBackend> {
    let mut hosts = Vec::new();
    if task_uses_local_connector(task) {
        hosts.push(BuiltinHostBackend::LocalConnector);
    }
    if task_uses_harness_code(task) {
        hosts.push(BuiltinHostBackend::HarnessCode);
    }
    hosts
}

async fn apply_harness_project_runtime_routing_to_task(
    config: &AppConfig,
    store: &AppStore,
    task: &mut TaskRecord,
) -> Result<bool, String> {
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    if project_id == PUBLIC_PROJECT_ID {
        return Ok(false);
    }
    let Some(project) = resolve_project_for_task(config, store, project_id.as_str()).await? else {
        return Ok(false);
    };
    if !project_is_ready_harness_repo(&project) {
        return Ok(false);
    }
    let Some(server) = harness_code_runtime_server(config, task, &project)? else {
        return Ok(false);
    };

    let before_config = serde_json::to_value(&task.mcp_config).ok();
    remove_internal_host_ephemeral_servers(&mut task.mcp_config);
    task.mcp_config.workspace_dir = None;
    task.mcp_config.ephemeral_http_servers.push(server);

    Ok(before_config != serde_json::to_value(&task.mcp_config).ok())
}

pub(super) async fn resolve_project_root_for_project_id(
    config: &AppConfig,
    store: &AppStore,
    project_id: &str,
) -> Result<Option<String>, String> {
    let project_id = project_id.trim();
    if project_id.is_empty() || project_id == PUBLIC_PROJECT_ID {
        return Ok(None);
    }
    if config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return Ok(
            super::project_management_api_client::get_project_from_project_service(
                config, project_id,
            )
            .await?
            .and_then(|project| project.root_path),
        );
    }
    store
        .get_task_project(project_id)
        .await
        .map(|project| project.and_then(|project| project.root_path))
}

pub(super) async fn resolve_project_root_for_task(
    config: &AppConfig,
    store: &AppStore,
    task: &TaskRecord,
) -> Result<Option<String>, String> {
    if let Some(root) = project_root_from_payload(task.input_payload.as_ref()) {
        return Ok(Some(root));
    }
    resolve_project_root_for_project_id(config, store, task.project_id.as_str()).await
}

pub(super) fn project_root_from_payload(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| {
            value
                .get("project_root")
                .or_else(|| value.get("projectRoot"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn apply_local_connector_runtime_routing_to_task(
    task: &mut TaskRecord,
    project_root: &str,
) -> bool {
    let Some(server) = local_connector_runtime_server(task, project_root) else {
        return false;
    };
    let before_config = serde_json::to_value(&task.mcp_config).ok();
    remove_internal_host_ephemeral_servers(&mut task.mcp_config);
    task.mcp_config.workspace_dir = None;
    task.mcp_config.ephemeral_http_servers.push(server);
    before_config != serde_json::to_value(&task.mcp_config).ok()
}

async fn resolve_project_for_task(
    config: &AppConfig,
    store: &AppStore,
    project_id: &str,
) -> Result<Option<crate::models::TaskProjectRecord>, String> {
    if config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return super::project_management_api_client::get_project_from_project_service(
            config, project_id,
        )
        .await;
    }
    store.get_task_project(project_id).await
}

fn project_is_ready_harness_repo(project: &crate::models::TaskProjectRecord) -> bool {
    project
        .harness_repo_path
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && project
            .import_status
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case("ready"))
}

fn selected_harness_code_builtin_kinds_for_task(task: &TaskRecord) -> Vec<BuiltinMcpKind> {
    let resolution = resolve_task_mcp(task, &[BuiltinHostBackend::HarnessCode]);
    hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::HarnessCode)
}

fn harness_code_runtime_server(
    config: &AppConfig,
    task: &TaskRecord,
    project: &crate::models::TaskProjectRecord,
) -> Result<Option<TaskEphemeralHttpMcpServer>, String> {
    let harness_kinds = selected_harness_code_builtin_kinds_for_task(task);
    if harness_kinds.is_empty() {
        return Ok(None);
    }
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        harness_code_builtin_kinds_header_value(harness_kinds.as_slice()),
    );
    Ok(Some(TaskEphemeralHttpMcpServer {
        name: HARNESS_CODE_MCP_SERVER_NAME.to_string(),
        url: harness_code_mcp_url(config, project.id.as_str())?,
        headers,
        auth_mode: Some(crate::models::TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC.to_string()),
    }))
}

fn is_harness_code_ephemeral_server(server: &TaskEphemeralHttpMcpServer) -> bool {
    server
        .name
        .trim()
        .eq_ignore_ascii_case(HARNESS_CODE_MCP_SERVER_NAME)
}

fn harness_code_builtin_kinds_header_value(kinds: &[BuiltinMcpKind]) -> String {
    builtin_kind_header_value(kinds.iter().map(|kind| kind.kind_name()))
}

fn harness_code_mcp_url(config: &AppConfig, project_id: &str) -> Result<String, String> {
    let base = config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project service base url is required for Harness MCP routing".to_string())?
        .trim_end_matches('/');
    Ok(format!(
        "{base}/api/chatos-sync/projects/{}/harness/mcp",
        urlencoding::encode(project_id.trim())
    ))
}

fn local_connector_runtime_server(
    task: &TaskRecord,
    project_root: &str,
) -> Option<TaskEphemeralHttpMcpServer> {
    let Some(project) = parse_local_connector_project_root(project_root) else {
        return None;
    };
    let local_kinds = selected_local_connector_builtin_kinds_for_task(task);
    let local_kinds = normalize_local_connector_builtin_kinds(local_kinds.iter().copied());
    if local_kinds.is_empty() {
        return None;
    }
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        local_connector_builtin_kinds_header_value(local_kinds.as_slice()),
    );
    Some(TaskEphemeralHttpMcpServer {
        name: LOCAL_CONNECTOR_MCP_SERVER_NAME.to_string(),
        url: local_connector_mcp_url(&project),
        headers,
        auth_mode: Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL.to_string()),
    })
}

fn selected_local_connector_builtin_kinds_for_task(task: &TaskRecord) -> Vec<BuiltinMcpKind> {
    let resolution = resolve_task_mcp(task, &[BuiltinHostBackend::LocalConnector]);
    hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::LocalConnector)
}

fn normalize_local_connector_builtin_kinds<I>(kinds: I) -> Vec<BuiltinMcpKind>
where
    I: IntoIterator<Item = BuiltinMcpKind>,
{
    selected_host_builtin_kind_names(
        BuiltinHostBackend::LocalConnector,
        kinds.into_iter().map(|kind| kind.kind_name()),
    )
    .into_iter()
    .filter_map(builtin_kind_by_any)
    .collect()
}

fn local_connector_builtin_kinds_header_value(kinds: &[BuiltinMcpKind]) -> String {
    builtin_kind_header_value(kinds.iter().map(|kind| kind.kind_name()))
}

fn is_local_connector_ephemeral_server(server: &TaskEphemeralHttpMcpServer) -> bool {
    server
        .auth_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| {
            value.eq_ignore_ascii_case(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
        })
        || server.name.trim().eq_ignore_ascii_case("local_connector")
}

fn is_internal_host_ephemeral_server(server: &TaskEphemeralHttpMcpServer) -> bool {
    is_local_connector_ephemeral_server(server) || is_harness_code_ephemeral_server(server)
}

fn remove_internal_host_ephemeral_servers(mcp_config: &mut TaskMcpConfig) {
    mcp_config
        .ephemeral_http_servers
        .retain(|server| !is_internal_host_ephemeral_server(server));
}

pub(super) fn task_uses_local_connector_builtin_kind(
    task: &TaskRecord,
    kind: BuiltinMcpKind,
) -> bool {
    task.mcp_config
        .ephemeral_http_servers
        .iter()
        .filter(|server| is_local_connector_ephemeral_server(server))
        .any(|server| local_connector_server_enables_builtin_kind(server, kind))
}

pub(super) fn local_connector_server_enables_builtin_kind(
    server: &TaskEphemeralHttpMcpServer,
    kind: BuiltinMcpKind,
) -> bool {
    let Some(raw) = server
        .headers
        .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
    else {
        return false;
    };
    HostCapabilityPolicy::from_header_value(raw).enables_builtin_kind_name(kind.kind_name())
}

fn parse_local_connector_project_root(project_root: &str) -> Option<LocalConnectorProjectRef> {
    let rest = project_root
        .trim()
        .strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = normalized_optional(parts.next().map(ToOwned::to_owned))?;
    let workspace_id = normalized_optional(parts.next().map(ToOwned::to_owned))?;
    let relative_path = match parts.next() {
        Some(path) => Some(decode_local_connector_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorProjectRef {
        device_id,
        workspace_id,
        relative_path,
    })
}

fn local_connector_mcp_url(project: &LocalConnectorProjectRef) -> String {
    let mut url = format!(
        "{}/api/local-connectors/relay/{}/mcp?workspace_id={}",
        local_connector_service_base_url().trim_end_matches('/'),
        urlencoding::encode(project.device_id.as_str()),
        urlencoding::encode(project.workspace_id.as_str())
    );
    if let Some(relative_path) = project.relative_path.as_deref() {
        url.push_str("&cwd=");
        url.push_str(urlencoding::encode(relative_path).as_ref());
    }
    url
}

fn local_connector_service_base_url() -> String {
    std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:39230".to_string())
}

fn decode_local_connector_relative_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|part| !part.trim().is_empty()) {
        let decoded = urlencoding::decode(part).ok()?.into_owned();
        parts.push(decoded);
    }
    let joined = parts.join("/");
    normalize_local_relative_path(joined.as_str()).filter(|path| local_relative_path_is_safe(path))
}

fn normalize_local_relative_path(value: &str) -> Option<String> {
    let value = value.trim().replace('\\', "/");
    let value = value.trim_matches('/');
    if value.is_empty() || value == "." {
        return None;
    }
    let parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn local_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path.split('/').all(|part| {
            let part = part.trim();
            !part.is_empty() && part != "." && part != ".."
        })
}

pub(super) fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    let kinds = values
        .into_iter()
        .filter_map(|value| builtin_kind_by_any(&value))
        .filter(|kind| {
            !matches!(
                kind,
                BuiltinMcpKind::ProjectManagement
                    | BuiltinMcpKind::TaskManager
                    | BuiltinMcpKind::AskUser
            )
        })
        .collect::<Vec<_>>();
    complete_builtin_kind_dependencies(kinds)
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

pub(super) fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.init_mode = chatos_ai_runtime::TaskMcpInitMode::Full;
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.sandbox_manager_base_url = normalized_optional(config.sandbox_manager_base_url)
        .map(|value| value.trim_end_matches('/').to_string());
    config.default_remote_server_id = normalized_optional(config.default_remote_server_id);
    config.external_mcp_config_ids = normalize_strings(config.external_mcp_config_ids);
    config.ephemeral_http_servers = normalize_ephemeral_http_servers(config.ephemeral_http_servers);
    config.skill_ids = normalize_strings(config.skill_ids);
    config
}

fn normalize_ephemeral_http_servers(
    values: Vec<TaskEphemeralHttpMcpServer>,
) -> Vec<TaskEphemeralHttpMcpServer> {
    values
        .into_iter()
        .filter_map(|mut server| {
            server.name = normalized_optional(Some(server.name))?;
            server.url = normalized_optional(Some(server.url))?;
            server.auth_mode = normalized_optional(server.auth_mode).map(|value| {
                if value.eq_ignore_ascii_case(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL) {
                    TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL.to_string()
                } else if value
                    .eq_ignore_ascii_case(crate::models::TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC)
                {
                    crate::models::TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC.to_string()
                } else {
                    value
                }
            });
            server.headers = server
                .headers
                .into_iter()
                .filter_map(|(key, value)| {
                    let key = normalized_optional(Some(key))?;
                    let value = normalized_optional(Some(value))?;
                    Some((key, value))
                })
                .collect();
            Some(server)
        })
        .collect()
}

pub(super) fn ensure_effective_task_workspace_dir(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<String, String> {
    let configured = task
        .mcp_config
        .workspace_dir
        .as_deref()
        .or(model_config.request_cwd.as_deref());
    if configured.is_some() {
        return ensure_workspace_dir_available(config.default_workspace_dir.as_str(), configured);
    }

    ensure_default_user_workspace_dir_available(
        config.default_workspace_dir.as_str(),
        task.subject_id.as_str(),
    )
}

pub(super) fn resolve_workspace_dir_with_base(base_dir: &str, configured: Option<&str>) -> String {
    let candidate = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(base_dir);
    let path = PathBuf::from(candidate);
    let resolved = if path.is_absolute() {
        path
    } else {
        PathBuf::from(base_dir).join(path)
    };
    std::fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .to_string()
}

pub(super) fn ensure_workspace_dir_available(
    base_dir: &str,
    configured: Option<&str>,
) -> Result<String, String> {
    let resolved = resolve_workspace_dir_with_base(base_dir, configured);
    ensure_workspace_is_inside_base(base_dir, resolved.as_str())?;
    let path = PathBuf::from(&resolved);

    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(format!("工作目录不是目录: {}", path.display()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&path).map_err(|create_err| {
                format!(
                    "create workspace dir {} failed: {}",
                    path.display(),
                    create_err
                )
            })?;
        }
        Err(err) => {
            return Err(format!(
                "read workspace dir {} failed: {}",
                path.display(),
                err
            ));
        }
    }

    Ok(path
        .canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string())
}

pub(super) fn ensure_default_user_workspace_dir_available(
    base_dir: &str,
    subject_id: &str,
) -> Result<String, String> {
    let user_component = user_workspace_component(subject_id);
    let relative = PathBuf::from("users")
        .join(user_component)
        .join("workspaces")
        .join("default");
    ensure_workspace_dir_available(base_dir, relative.to_str())
}

pub(super) fn default_user_workspace_dir(base_dir: &str, subject_id: Option<&str>) -> PathBuf {
    let subject_id = subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task_runner");
    PathBuf::from(base_dir)
        .join("users")
        .join(user_workspace_component(subject_id))
        .join("workspaces")
        .join("default")
}

fn ensure_workspace_is_inside_base(base_dir: &str, workspace_dir: &str) -> Result<(), String> {
    let base = canonical_or_absolute(Path::new(base_dir));
    let workspace = canonical_or_absolute(Path::new(workspace_dir));
    if path_is_within_root(workspace.as_path(), base.as_path()) {
        Ok(())
    } else {
        Err(format!(
            "workspace dir is outside task runner workspace base: {}",
            workspace.display()
        ))
    }
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    canonicalize_existing_prefix(&absolute)
}

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    let mut missing = Vec::<OsString>::new();
    while !current.exists() {
        let Some(file_name) = current.file_name() else {
            break;
        };
        missing.push(file_name.to_os_string());
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    let mut resolved = std::fs::canonicalize(&current).unwrap_or(current);
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    resolved
}

fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize_path_for_compare(candidate);
    let root = normalize_path_for_compare(root);
    candidate == root || candidate.starts_with(format!("{root}/").as_str())
}

fn user_workspace_component(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '_', '-'])
        .chars()
        .take(80)
        .collect::<String>();
    let prefix = if normalized.is_empty() {
        "user".to_string()
    } else {
        normalized
    };
    format!("{prefix}-{:016x}", stable_hash64(value.trim().as_bytes()))
}

fn stable_hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        if let Some(stripped) = value.strip_prefix("//?/UNC/") {
            value = format!("//{stripped}");
        } else if let Some(stripped) = value.strip_prefix("//?/") {
            value = stripped.to_string();
        }
    }
    let (prefix, rest) = if value.len() >= 2 && value.as_bytes()[1] == b':' {
        (value[..2].to_string(), &value[2..])
    } else {
        (String::new(), value.as_str())
    };
    let absolute = rest.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                let _ = segments.pop();
            }
            value => segments.push(value),
        }
    }
    let mut out = String::new();
    out.push_str(prefix.as_str());
    if absolute {
        out.push('/');
    }
    out.push_str(segments.join("/").as_str());
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(windows) {
        out.make_ascii_lowercase();
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::models::{
        now_rfc3339, TaskEphemeralHttpMcpServer, TaskMcpConfig, TaskRecord, TaskScheduleConfig,
        TaskStatus, TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL,
        TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC, TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
    };

    use super::{
        ensure_workspace_is_inside_base, runtime_selected_builtin_kinds, selected_builtin_kinds,
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
    };
    use chatos_mcp_runtime::BuiltinMcpKind;

    #[test]
    fn empty_builtin_selection_stays_empty() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: Vec::new(),
            ..TaskMcpConfig::default()
        };

        assert!(selected_builtin_kinds(&config).is_empty());
    }

    #[test]
    fn default_config_still_selects_builtin_kinds() {
        let config = TaskMcpConfig::default();

        assert!(!selected_builtin_kinds(&config).is_empty());
    }

    #[test]
    fn plan_task_builtin_selection_uses_fixed_allowlist() {
        let task = sample_task(
            TASK_PROFILE_CHATOS_PLAN,
            vec![
                "CodeMaintainerWrite".to_string(),
                "AgentBuilder".to_string(),
            ],
        );

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(selected.contains(&BuiltinMcpKind::TaskManager));
        assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
        assert!(selected.contains(&BuiltinMcpKind::BrowserTools));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(!selected.contains(&BuiltinMcpKind::AgentBuilder));
    }

    #[test]
    fn default_task_builtin_selection_keeps_requested_kinds() {
        let task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec!["CodeMaintainerWrite".to_string()],
        );

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    }

    #[test]
    fn contact_async_task_adds_required_task_manager_and_ask_user_at_runtime() {
        let mut task = sample_task(TASK_PROFILE_DEFAULT, Vec::new());
        task.schedule.mode = crate::models::TaskScheduleMode::ContactAsync;

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(task.mcp_config.enabled_builtin_kinds.is_empty());
        assert!(selected.contains(&BuiltinMcpKind::TaskManager));
        assert!(selected.contains(&BuiltinMcpKind::AskUser));
    }

    #[test]
    fn local_connector_task_removes_server_local_builtin_kinds() {
        let mut task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec![
                "CodeMaintainerWrite".to_string(),
                "TerminalController".to_string(),
                "BrowserTools".to_string(),
                "WebTools".to_string(),
            ],
        );
        task.mcp_config
            .ephemeral_http_servers
            .push(local_connector_server());

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(!selected.contains(&BuiltinMcpKind::TerminalController));
        assert!(!selected.contains(&BuiltinMcpKind::BrowserTools));
        assert!(selected.contains(&BuiltinMcpKind::WebTools));
    }

    #[test]
    fn local_connector_plan_task_removes_fixed_server_local_builtin_kinds() {
        let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());
        task.mcp_config
            .ephemeral_http_servers
            .push(local_connector_server());

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(!selected.contains(&BuiltinMcpKind::TerminalController));
        assert!(!selected.contains(&BuiltinMcpKind::BrowserTools));
        assert!(selected.contains(&BuiltinMcpKind::TaskManager));
        assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
    }

    #[test]
    fn harness_code_task_removes_server_local_code_builtin_kinds() {
        let mut task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec![
                "CodeMaintainerWrite".to_string(),
                "TerminalController".to_string(),
                "WebTools".to_string(),
            ],
        );
        task.mcp_config
            .ephemeral_http_servers
            .push(harness_code_server());

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(selected.contains(&BuiltinMcpKind::TerminalController));
        assert!(selected.contains(&BuiltinMcpKind::WebTools));
    }

    #[test]
    fn harness_code_plan_task_removes_fixed_server_local_code_builtin_kinds() {
        let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());
        task.mcp_config
            .ephemeral_http_servers
            .push(harness_code_server());

        let selected = runtime_selected_builtin_kinds(&task);

        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
        assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
        assert!(selected.contains(&BuiltinMcpKind::TerminalController));
        assert!(selected.contains(&BuiltinMcpKind::BrowserTools));
        assert!(selected.contains(&BuiltinMcpKind::TaskManager));
        assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
    }

    #[test]
    fn local_connector_runtime_routing_keeps_requested_config_and_payload() {
        let mut task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec![
                "CodeMaintainerRead".to_string(),
                "TerminalController".to_string(),
                "BrowserTools".to_string(),
                "TaskManager".to_string(),
            ],
        );
        task.input_payload = Some(serde_json::json!({ "source": "test" }));

        let changed = super::apply_local_connector_runtime_routing_to_task(
            &mut task,
            "local://connector/device-1/workspace-1/apps/web",
        );

        assert!(changed);
        assert_eq!(
            task.input_payload,
            Some(serde_json::json!({ "source": "test" }))
        );
        assert_eq!(
            task.mcp_config.enabled_builtin_kinds,
            vec![
                "CodeMaintainerRead".to_string(),
                "TerminalController".to_string(),
                "BrowserTools".to_string(),
                "TaskManager".to_string(),
            ]
        );
        let server = task
            .mcp_config
            .ephemeral_http_servers
            .first()
            .expect("local connector server");
        assert_eq!(server.name, "local_connector");
        assert_eq!(
            server.auth_mode.as_deref(),
            Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
        );
        assert_eq!(
            server
                .headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                .map(String::as_str),
            Some("CodeMaintainerRead,TerminalController,BrowserTools")
        );
        assert!(server
            .url
            .contains("/api/local-connectors/relay/device-1/mcp"));
        assert!(server.url.contains("workspace_id=workspace-1"));
        assert!(server.url.contains("cwd=apps%2Fweb"));
    }

    #[test]
    fn local_connector_routing_passes_only_selected_local_capabilities() {
        let mut task = sample_task(
            TASK_PROFILE_DEFAULT,
            vec!["BrowserTools".to_string(), "TaskManager".to_string()],
        );

        let changed = super::apply_local_connector_runtime_routing_to_task(
            &mut task,
            "local://connector/device-1/workspace-1/apps/web",
        );

        assert!(changed);
        assert_eq!(
            task.mcp_config.enabled_builtin_kinds,
            vec!["BrowserTools".to_string(), "TaskManager".to_string()]
        );
        let server = task
            .mcp_config
            .ephemeral_http_servers
            .first()
            .expect("local connector server");
        assert_eq!(
            server
                .headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                .map(String::as_str),
            Some("BrowserTools")
        );
        assert!(super::local_connector_server_enables_builtin_kind(
            server,
            BuiltinMcpKind::BrowserTools
        ));
        assert!(!super::local_connector_server_enables_builtin_kind(
            server,
            BuiltinMcpKind::TerminalController
        ));
        assert!(!super::local_connector_server_enables_builtin_kind(
            &local_connector_server(),
            BuiltinMcpKind::BrowserTools
        ));
    }

    #[test]
    fn local_connector_plan_routing_routes_profile_required_capabilities() {
        let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());

        let changed = super::apply_local_connector_runtime_routing_to_task(
            &mut task,
            "local://connector/device-1/workspace-1/apps/web",
        );

        assert!(changed);
        assert!(task.input_payload.is_none());
        let server = task
            .mcp_config
            .ephemeral_http_servers
            .first()
            .expect("local connector server");
        assert_eq!(
            server
                .headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                .map(String::as_str),
            Some("CodeMaintainerRead,TerminalController,BrowserTools")
        );
    }

    #[test]
    fn local_connector_plan_routing_merges_profile_and_selected_capabilities() {
        let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, vec!["BrowserTools".to_string()]);

        let changed = super::apply_local_connector_runtime_routing_to_task(
            &mut task,
            "local://connector/device-1/workspace-1/apps/web",
        );

        assert!(changed);
        let server = task
            .mcp_config
            .ephemeral_http_servers
            .first()
            .expect("local connector server");
        assert_eq!(
            server
                .headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                .map(String::as_str),
            Some("CodeMaintainerRead,TerminalController,BrowserTools")
        );
        assert!(super::local_connector_server_enables_builtin_kind(
            server,
            BuiltinMcpKind::BrowserTools
        ));
        assert!(super::local_connector_server_enables_builtin_kind(
            server,
            BuiltinMcpKind::TerminalController
        ));
        assert!(super::local_connector_server_enables_builtin_kind(
            server,
            BuiltinMcpKind::CodeMaintainerRead
        ));
    }

    #[test]
    fn workspace_base_check_accepts_relative_child_under_relative_base() {
        assert!(
            ensure_workspace_is_inside_base(".", ".\\users\\subject\\workspaces\\default").is_ok()
        );
    }

    #[test]
    fn workspace_base_check_rejects_relative_parent_escape() {
        let err = ensure_workspace_is_inside_base(".", "..\\outside")
            .expect_err("parent traversal should be outside workspace base");

        assert!(err.contains("workspace dir is outside"));
    }

    #[test]
    fn normalized_config_removes_project_management_selection() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec![
                "ProjectManagement".to_string(),
                "TaskManager".to_string(),
                "AskUser".to_string(),
                "CodeMaintainerWrite".to_string(),
            ],
            ..TaskMcpConfig::default()
        };

        let sanitized = super::sanitize_task_mcp_config(config);

        assert!(!sanitized
            .enabled_builtin_kinds
            .contains(&"ProjectManagement".to_string()));
        assert!(!sanitized
            .enabled_builtin_kinds
            .contains(&"TaskManager".to_string()));
        assert!(!sanitized
            .enabled_builtin_kinds
            .contains(&"AskUser".to_string()));
        assert!(sanitized
            .enabled_builtin_kinds
            .contains(&"CodeMaintainerWrite".to_string()));
        assert!(sanitized
            .enabled_builtin_kinds
            .contains(&"CodeMaintainerRead".to_string()));
    }

    fn sample_task(task_profile: &str, enabled_builtin_kinds: Vec<String>) -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: "task-1".to_string(),
            title: "task".to_string(),
            description: None,
            objective: "objective".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "memory-1".to_string(),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: "project-1".to_string(),
            task_profile: task_profile.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: Default::default(),
            mcp_config: TaskMcpConfig {
                enabled_builtin_kinds,
                ..TaskMcpConfig::default()
            },
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }

    fn local_connector_server() -> TaskEphemeralHttpMcpServer {
        TaskEphemeralHttpMcpServer {
            name: "local_connector".to_string(),
            url: "http://127.0.0.1:39230/internal/mcp".to_string(),
            headers: Default::default(),
            auth_mode: Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL.to_string()),
        }
    }

    fn harness_code_server() -> TaskEphemeralHttpMcpServer {
        let mut headers = std::collections::BTreeMap::new();
        headers.insert(
            super::HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
            "CodeMaintainerRead,CodeMaintainerWrite".to_string(),
        );
        TaskEphemeralHttpMcpServer {
            name: "harness_code".to_string(),
            url: "http://127.0.0.1:39210/api/chatos-sync/projects/project-1/harness/mcp"
                .to_string(),
            headers,
            auth_mode: Some(TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC.to_string()),
        }
    }
}
