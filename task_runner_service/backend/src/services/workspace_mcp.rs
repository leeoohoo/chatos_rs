// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_mcp_service::{
    builtin_kind_header_value, selected_host_builtin_kind_names, BuiltinHostBackend,
    HostCapabilityPolicy, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{
    TaskEphemeralHttpMcpServer, TaskMcpConfig, TaskMcpResolutionResponse, TaskRecord,
    PUBLIC_PROJECT_ID, TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL,
};
use crate::store::AppStore;

use super::mcp_resolution::{
    hosted_builtin_kinds_for, resolve_task_mcp, resolve_task_mcp_authoritative,
    selected_builtin_kinds_from_config,
    task_mcp_resolution_response as build_mcp_resolution_response,
};
use super::normalize_strings;
use super::normalized_optional;

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";
const LOCAL_CONNECTOR_MCP_SERVER_NAME: &str = "local_connector";
const HARNESS_CODE_MCP_SERVER_NAME: &str = "harness_code";

mod workspace_dirs;

#[cfg(test)]
mod tests;

#[cfg(test)]
use workspace_dirs::ensure_workspace_is_inside_base;
pub(super) use workspace_dirs::{
    default_user_workspace_dir, ensure_effective_task_workspace_dir,
    ensure_workspace_dir_available, resolve_workspace_dir_with_base,
};

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

pub(super) fn runtime_selected_builtin_kinds_authoritative(
    task: &TaskRecord,
) -> Vec<BuiltinMcpKind> {
    resolve_task_mcp_authoritative(task, active_host_backends_for_task(task).as_slice())
        .server_local_builtin_kinds
}

pub(super) fn task_mcp_resolution_response(task: &TaskRecord) -> TaskMcpResolutionResponse {
    build_mcp_resolution_response(task, active_host_backends_for_task(task).as_slice())
}

pub(super) async fn task_with_runtime_mcp_routing(
    config: &AppConfig,
    store: &AppStore,
    task: TaskRecord,
) -> Result<TaskRecord, String> {
    task_with_runtime_mcp_routing_impl(config, store, task, false).await
}

pub(super) async fn task_with_runtime_mcp_routing_authoritative(
    config: &AppConfig,
    store: &AppStore,
    task: TaskRecord,
) -> Result<TaskRecord, String> {
    task_with_runtime_mcp_routing_impl(config, store, task, true).await
}

async fn task_with_runtime_mcp_routing_impl(
    config: &AppConfig,
    store: &AppStore,
    mut task: TaskRecord,
    authoritative: bool,
) -> Result<TaskRecord, String> {
    if !task.mcp_config.enabled {
        return Ok(task);
    }

    if let Some(project_root) = resolve_project_root_for_task(config, store, &task).await? {
        if parse_local_connector_project_root(project_root.as_str()).is_some() {
            apply_local_connector_runtime_routing_to_task(
                &mut task,
                project_root.as_str(),
                authoritative,
            );
            return Ok(task);
        }
    }

    apply_harness_project_runtime_routing_to_task(config, store, &mut task, authoritative).await?;
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
    authoritative: bool,
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
    let Some(server) = harness_code_runtime_server(config, task, &project, authoritative)? else {
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
    authoritative: bool,
) -> bool {
    let Some(server) = local_connector_runtime_server(task, project_root, authoritative) else {
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

fn selected_harness_code_builtin_kinds_for_task(
    task: &TaskRecord,
    authoritative: bool,
) -> Vec<BuiltinMcpKind> {
    let resolution = if authoritative {
        resolve_task_mcp_authoritative(task, &[BuiltinHostBackend::HarnessCode])
    } else {
        resolve_task_mcp(task, &[BuiltinHostBackend::HarnessCode])
    };
    hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::HarnessCode)
}

fn harness_code_runtime_server(
    config: &AppConfig,
    task: &TaskRecord,
    project: &crate::models::TaskProjectRecord,
    authoritative: bool,
) -> Result<Option<TaskEphemeralHttpMcpServer>, String> {
    let harness_kinds = selected_harness_code_builtin_kinds_for_task(task, authoritative);
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
    authoritative: bool,
) -> Option<TaskEphemeralHttpMcpServer> {
    let project = parse_local_connector_project_root(project_root)?;
    let local_kinds = selected_local_connector_builtin_kinds_for_task(task, authoritative);
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

fn selected_local_connector_builtin_kinds_for_task(
    task: &TaskRecord,
    authoritative: bool,
) -> Vec<BuiltinMcpKind> {
    let resolution = if authoritative {
        resolve_task_mcp_authoritative(task, &[BuiltinHostBackend::LocalConnector])
    } else {
        resolve_task_mcp(task, &[BuiltinHostBackend::LocalConnector])
    };
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
    let mut kinds = Vec::new();
    for value in values {
        let Some(kind) = builtin_kind_by_any(&value) else {
            continue;
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
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
