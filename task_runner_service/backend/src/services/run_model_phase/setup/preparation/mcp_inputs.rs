// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

use std::collections::BTreeSet;

use chatos_mcp::{
    system_mcp_descriptor_for_record, ResolvedSystemMcpBackend, SystemMcpHostAdapter,
    SystemMcpResolveContext,
};
use chatos_mcp_runtime::{
    BuiltinMcpKind, McpToolNameAlias, BROWSER_TOOLS_SERVER_NAME, CODE_MAINTAINER_READ_SERVER_NAME,
    CODE_MAINTAINER_WRITE_SERVER_NAME, TERMINAL_CONTROLLER_SERVER_NAME,
};

use crate::models::ExternalMcpConfigRecord;
use crate::services::system_mcp_adapter::TaskRunnerSystemMcpAdapter;

#[derive(Debug, Clone)]
pub(super) struct LoadedExternalMcpServers {
    pub(super) http_servers: Vec<McpHttpServer>,
    pub(super) stdio_servers: Vec<McpStdioServer>,
    pub(super) summaries: Vec<ExternalMcpRuntimeSummary>,
}

#[derive(Debug, Clone)]
pub(super) struct ExternalMcpRuntimeSummary {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) transport: String,
}

pub(super) async fn load_external_mcp_servers(
    service: &RunService,
    task: &TaskRecord,
    effective_workspace_dir: &str,
    capability_policy: Option<&TaskRunnerCapabilityPolicy>,
) -> Result<LoadedExternalMcpServers, String> {
    if !task.mcp_config.enabled
        || (task.mcp_config.external_mcp_config_ids.is_empty()
            && task.mcp_config.ephemeral_http_servers.is_empty())
    {
        return Ok(LoadedExternalMcpServers {
            http_servers: Vec::new(),
            stdio_servers: Vec::new(),
            summaries: Vec::new(),
        });
    }

    let mut http_servers = Vec::new();
    let mut stdio_servers = Vec::new();
    let mut summaries = Vec::new();
    if let Some(policy) = capability_policy {
        for resource in policy.effective_external_mcps(task)? {
            let loaded =
                plugin_mcp_server_for_resource(service, task, resource, effective_workspace_dir)
                    .await?;
            if let Some(server) = loaded.http_server {
                http_servers.push(server);
            }
            if let Some(server) = loaded.stdio_server {
                stdio_servers.push(server);
            }
            summaries.push(ExternalMcpRuntimeSummary {
                id: resource.id.clone(),
                name: loaded.name,
                transport: loaded.transport,
            });
        }
    } else if !task.mcp_config.external_mcp_config_ids.is_empty() {
        return Err(
            "legacy external MCP execution is disabled; cloud MCPs must be resolved through Plugin Management"
                .to_string(),
        );
    }
    for server in &task.mcp_config.ephemeral_http_servers {
        if server.auth_mode.as_deref()
            == Some(crate::models::TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
        {
            return Err(format!(
                "Local Connector MCP is unavailable in cloud Task Runner: {}",
                server.name
            ));
        }
        let mut headers = server.headers.clone();
        if server.auth_mode.as_deref()
            == Some(crate::models::TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC)
        {
            let secret = service
                .config
                .project_service_sync_secret
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "ephemeral MCP server {} requires TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET",
                        server.name
                    )
                })?;
            crate::services::project_management_api_client::insert_project_service_mcp_signing_headers(
                &mut headers,
                secret,
                crate::services::project_management_api_client::PROJECT_HARNESS_SCOPE,
            )?;
            if let Some(owner_user_id) = normalized_task_owner_user_id(task) {
                headers.insert("x-task-runner-owner-user-id".to_string(), owner_user_id);
            }
            headers.insert("x-task-runner-task-id".to_string(), task.id.clone());
            headers.insert(
                "x-task-runner-project-id".to_string(),
                task.project_id.clone(),
            );
        }
        let tool_name_aliases = hosted_builtin_tool_name_aliases(server.name.as_str(), &headers);
        let mut http_server = McpHttpServer::new(server.name.clone(), server.url.clone())
            .with_tool_name_aliases(tool_name_aliases);
        if !headers.is_empty() {
            http_server = http_server.with_headers(headers.into_iter().collect());
        }
        http_servers.push(http_server);
        summaries.push(ExternalMcpRuntimeSummary {
            id: format!("ephemeral:{}", server.name),
            name: server.name.clone(),
            transport: "http".to_string(),
        });
    }
    Ok(LoadedExternalMcpServers {
        http_servers,
        stdio_servers,
        summaries,
    })
}

struct LoadedPluginMcpServer {
    name: String,
    transport: String,
    http_server: Option<McpHttpServer>,
    stdio_server: Option<McpStdioServer>,
}

async fn plugin_mcp_server_for_resource(
    service: &RunService,
    task: &TaskRecord,
    resource: &chatos_plugin_management_sdk::McpRecord,
    effective_workspace_dir: &str,
) -> Result<LoadedPluginMcpServer, String> {
    ensure_cloud_mcp_runtime(resource)?;
    let server_name = resource
        .runtime
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(resource.name.as_str())
        .to_string();
    if let Some(descriptor) = system_mcp_descriptor_for_record(resource) {
        let context = SystemMcpResolveContext {
            workspace_dir: Some(effective_workspace_dir.to_string()),
            owner_user_id: normalized_task_owner_user_id(task),
            project_id: Some(crate::models::normalize_project_id(Some(
                task.project_id.clone(),
            ))),
            task_id: Some(task.id.clone()),
            headers: resource.runtime.headers.clone(),
            ..SystemMcpResolveContext::default()
        };
        return match TaskRunnerSystemMcpAdapter::new(&service.config)
            .resolve(descriptor.key, &context)
            .await?
        {
            ResolvedSystemMcpBackend::Http(server) => Ok(LoadedPluginMcpServer {
                name: server_name,
                transport: "http".to_string(),
                http_server: Some(server),
                stdio_server: None,
            }),
            ResolvedSystemMcpBackend::Unavailable(reason) => Err(reason),
            ResolvedSystemMcpBackend::Embedded { .. } => Err(format!(
                "embedded system MCP cannot be loaded as an external MCP: {}",
                descriptor.server_name
            )),
        };
    }
    match resource.runtime.kind.as_str() {
        "http" => {
            let url = resource
                .runtime
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("plugin MCP {} is missing HTTP URL", resource.id))?;
            let mut server = McpHttpServer::new(server_name.clone(), url.to_string());
            if !resource.runtime.headers.is_empty() {
                server =
                    server.with_headers(resource.runtime.headers.clone().into_iter().collect());
            }
            Ok(LoadedPluginMcpServer {
                name: server_name,
                transport: "http".to_string(),
                http_server: Some(server),
                stdio_server: None,
            })
        }
        "stdio_cloud" => {
            let command = resource
                .runtime
                .command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("plugin MCP {} is missing stdio command", resource.id))?;
            let config = ExternalMcpConfigRecord {
                id: resource.id.clone(),
                name: server_name.clone(),
                transport: "stdio".to_string(),
                command: Some(command.to_string()),
                args: resource.runtime.args.clone(),
                url: None,
                headers: Default::default(),
                env: resource.runtime.env.clone(),
                cwd: resource.runtime.cwd.clone(),
                enabled: true,
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: Some(resource.owner_user_id.clone()),
                owner_username: None,
                owner_display_name: None,
                created_at: resource.created_at.clone(),
                updated_at: resource.updated_at.clone(),
            };
            let server = task_stdio_server_for_config(
                &config,
                task.subject_id.as_str(),
                effective_workspace_dir,
            )?
            .ok_or_else(|| format!("plugin MCP {} stdio config is invalid", resource.id))?;
            Ok(LoadedPluginMcpServer {
                name: server_name,
                transport: "stdio".to_string(),
                http_server: None,
                stdio_server: Some(server),
            })
        }
        other => Err(format!(
            "plugin MCP {} uses unsupported Task Runner runtime kind: {other}",
            resource.id
        )),
    }
}

fn ensure_cloud_mcp_runtime(
    resource: &chatos_plugin_management_sdk::McpRecord,
) -> Result<(), String> {
    ensure_cloud_mcp_runtime_allowed(
        resource.id.as_str(),
        resource.source_kind.as_str(),
        resource.runtime.kind.as_str(),
        resource.runtime.local_connector.is_some(),
    )
}

fn ensure_cloud_mcp_runtime_allowed(
    resource_id: &str,
    source_kind: &str,
    runtime_kind: &str,
    has_local_connector_ref: bool,
) -> Result<(), String> {
    if source_kind == "local_connector_discovered"
        || runtime_kind.starts_with("local_connector_")
        || has_local_connector_ref
    {
        return Err(format!(
            "Local Connector MCP is unavailable in cloud Task Runner: {}",
            resource_id
        ));
    }
    Ok(())
}

fn normalized_task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn task_stdio_server_for_config(
    config: &ExternalMcpConfigRecord,
    task_subject_id: &str,
    effective_workspace_dir: &str,
) -> Result<Option<McpStdioServer>, String> {
    let Some(server) = config.to_stdio_server() else {
        return Ok(None);
    };
    let user_id = task_subject_id.trim();
    if user_id.is_empty() {
        return Err(format!(
            "external MCP stdio config {} cannot run without a task subject user id",
            config.id
        ));
    }
    let workspace_dir = effective_workspace_dir.trim();
    if workspace_dir.is_empty() {
        return Err(format!(
            "external MCP stdio config {} cannot run without an effective workspace",
            config.id
        ));
    }
    Ok(Some(
        server
            .with_user_id(user_id.to_string())
            .with_cwd(workspace_dir.to_string()),
    ))
}

pub(super) fn load_system_http_mcp_servers(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
    sandbox_context: Option<&crate::services::sandbox_runtime::SandboxRuntimeContext>,
) -> Result<Vec<McpHttpServer>, String> {
    let _ = service;
    let mut servers = Vec::new();
    if let Some(context) = sandbox_context {
        let aliases = sandbox_tool_name_aliases(task);
        let allowed_tool_names = aliases
            .iter()
            .map(|alias| alias.tool_name.clone())
            .collect::<Vec<_>>();
        servers.push(
            context
                .to_mcp_server(task, run)
                .with_tool_name_aliases(aliases)
                .with_allowed_tool_names(allowed_tool_names),
        );
    }
    Ok(servers)
}

pub(super) fn external_mcp_prefixed_input_items(
    summaries: &[ExternalMcpRuntimeSummary],
    _locale: BuiltinMcpPromptLocale,
) -> Vec<Value> {
    let summaries = summaries
        .iter()
        .filter(|summary| !is_internal_host_mcp_summary(summary))
        .collect::<Vec<_>>();
    if summaries.is_empty() {
        return Vec::new();
    }

    let list = summaries
        .iter()
        .map(|summary| {
            format!(
                "- {} (id: {}, transport: {})",
                summary.name, summary.id, summary.transport
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        "[External MCP]\nTask Runner has loaded these user-configured external MCP servers for this task:\n{list}\n\nIf the task objective asks you to use these external systems, directly call the corresponding tools currently exposed to you. External MCP tool names usually use the config name as their prefix. Do not inspect local Gemini/Codex/Claude MCP config files to decide whether these MCP servers exist; they are injected by Task Runner for this run. Use builtin tools only when the task also needs code, terminal, browser, or other builtin capabilities."
    );
    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    })]
}

pub(super) fn mcp_provider_skills_prefixed_input_items(prompt: Option<String>) -> Vec<Value> {
    let Some(prompt) = prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };
    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": prompt
        }]
    })]
}

fn is_internal_host_mcp_summary(summary: &ExternalMcpRuntimeSummary) -> bool {
    summary.id.trim().starts_with("ephemeral:")
        && ["local_connector", "harness_code"]
            .iter()
            .any(|name| summary.name.trim().eq_ignore_ascii_case(name))
}

fn hosted_builtin_tool_name_aliases(
    server_name: &str,
    headers: &std::collections::BTreeMap<String, String>,
) -> Vec<McpToolNameAlias> {
    let header_value = if server_name.trim().eq_ignore_ascii_case("local_connector") {
        headers.get(chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
    } else if server_name.trim().eq_ignore_ascii_case("harness_code") {
        headers.get(chatos_mcp_service::HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER)
    } else {
        None
    };
    let Some(header_value) = header_value else {
        return Vec::new();
    };
    let kinds = chatos_mcp_service::split_builtin_kind_header(header_value)
        .filter_map(chatos_mcp_runtime::builtin_kind_by_any)
        .collect::<Vec<_>>();
    tool_name_aliases_for_builtin_kinds(kinds.as_slice())
}

fn tool_name_aliases_for_builtin_kinds(kinds: &[BuiltinMcpKind]) -> Vec<McpToolNameAlias> {
    let mut aliases = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push_aliases = |tool_names: &[&str], public_server_name: &str| {
        for tool_name in tool_names {
            let key = ((*tool_name).to_string(), public_server_name.to_string());
            if seen.insert(key) {
                aliases.push(McpToolNameAlias {
                    tool_name: (*tool_name).to_string(),
                    public_server_name: public_server_name.to_string(),
                });
            }
        }
    };

    if kinds.contains(&BuiltinMcpKind::CodeMaintainerRead) {
        push_aliases(
            CODE_MAINTAINER_READ_TOOL_NAMES,
            CODE_MAINTAINER_READ_SERVER_NAME,
        );
    }
    if kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite) {
        push_aliases(
            CODE_MAINTAINER_WRITE_TOOL_NAMES,
            CODE_MAINTAINER_WRITE_SERVER_NAME,
        );
    }
    if kinds.contains(&BuiltinMcpKind::TerminalController) {
        push_aliases(
            TERMINAL_CONTROLLER_TOOL_NAMES,
            TERMINAL_CONTROLLER_SERVER_NAME,
        );
    }
    if kinds.contains(&BuiltinMcpKind::BrowserTools) {
        push_aliases(BROWSER_TOOL_NAMES, BROWSER_TOOLS_SERVER_NAME);
    }
    aliases
}

fn sandbox_tool_name_aliases(task: &TaskRecord) -> Vec<McpToolNameAlias> {
    let kinds = runtime_selected_builtin_kinds(task)
        .into_iter()
        .filter(|kind| crate::services::sandbox_runtime::sandbox_replaces_builtin_kind(*kind))
        .collect::<Vec<_>>();
    tool_name_aliases_for_builtin_kinds(kinds.as_slice())
}

const CODE_MAINTAINER_READ_TOOL_NAMES: &[&str] = &[
    "read_file_raw",
    "read_file_range",
    "read_file",
    "list_dir",
    "search_text",
    "search_files",
];
const CODE_MAINTAINER_WRITE_TOOL_NAMES: &[&str] = &[
    "write_file",
    "edit_file",
    "append_file",
    "delete_path",
    "apply_patch",
    "patch",
];
const TERMINAL_CONTROLLER_TOOL_NAMES: &[&str] = &[
    "execute_command",
    "get_recent_logs",
    "process_list",
    "process_poll",
    "process_log",
    "process_wait",
    "process_write",
    "process_kill",
    "process",
];
const BROWSER_TOOL_NAMES: &[&str] = &[
    "browser_navigate",
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_back",
    "browser_press",
    "browser_console",
    "browser_get_images",
    "browser_inspect",
    "browser_research",
    "browser_vision",
];

#[cfg(test)]
#[path = "mcp_inputs/tests.rs"]
mod tests;
