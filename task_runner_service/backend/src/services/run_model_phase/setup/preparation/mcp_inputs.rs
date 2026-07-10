// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

use std::collections::BTreeSet;

use chatos_mcp_runtime::{
    BuiltinMcpKind, McpToolNameAlias, BROWSER_TOOLS_SERVER_NAME, CODE_MAINTAINER_READ_SERVER_NAME,
    CODE_MAINTAINER_WRITE_SERVER_NAME, TERMINAL_CONTROLLER_SERVER_NAME,
};

use crate::models::ExternalMcpConfigRecord;

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
                plugin_mcp_server_for_resource(service, task, resource, effective_workspace_dir)?;
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
    } else {
        for config_id in &task.mcp_config.external_mcp_config_ids {
            let config = service
                .store
                .get_external_mcp_config(config_id)
                .await?
                .ok_or_else(|| format!("external MCP config not found: {config_id}"))?;
            if !config.enabled {
                return Err(format!("external MCP config is disabled: {config_id}"));
            }
            if let Some(server) = config.to_http_server() {
                http_servers.push(server);
            } else if let Some(server) = task_stdio_server_for_config(
                &config,
                task.subject_id.as_str(),
                effective_workspace_dir,
            )? {
                stdio_servers.push(server);
            } else {
                return Err(format!("external MCP config is invalid: {config_id}"));
            }
            summaries.push(ExternalMcpRuntimeSummary {
                id: config.id,
                name: config.name,
                transport: config.transport,
            });
        }
    }
    for server in &task.mcp_config.ephemeral_http_servers {
        let mut headers = server.headers.clone();
        if server.auth_mode.as_deref()
            == Some(crate::models::TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
        {
            let secret = service
                .config
                .local_connector_internal_api_secret
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "ephemeral MCP server {} requires TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
                        server.name
                    )
                })?;
            let owner_user_id = normalized_task_owner_user_id(task).ok_or_else(|| {
                format!(
                    "ephemeral MCP server {} requires a task owner user id",
                    server.name
                )
            })?;
            headers.insert(
                "x-local-connector-internal-secret".to_string(),
                secret.to_string(),
            );
            headers.insert("x-local-connector-owner-user-id".to_string(), owner_user_id);
            headers.insert("x-task-runner-task-id".to_string(), task.id.clone());
        } else if server.auth_mode.as_deref()
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
            headers.insert(
                "x-project-service-sync-secret".to_string(),
                secret.to_string(),
            );
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

fn plugin_mcp_server_for_resource(
    service: &RunService,
    task: &TaskRecord,
    resource: &chatos_plugin_management_sdk::McpRecord,
    effective_workspace_dir: &str,
) -> Result<LoadedPluginMcpServer, String> {
    let server_name = resource
        .runtime
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(resource.name.as_str())
        .to_string();
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
        "local_connector_stdio" | "local_connector_http" | "local_connector_builtin_proxy" => {
            let local = resource.runtime.local_connector.as_ref().ok_or_else(|| {
                format!(
                    "plugin MCP {} is missing Local Connector reference",
                    resource.id
                )
            })?;
            let device_id = required_plugin_runtime_text(
                local.device_id.as_deref(),
                resource.id.as_str(),
                "device_id",
            )?;
            let builtin_proxy = resource.runtime.kind == "local_connector_builtin_proxy";
            let workspace_id = if builtin_proxy {
                Some(required_plugin_runtime_text(
                    local.workspace_id.as_deref(),
                    resource.id.as_str(),
                    "workspace_id",
                )?)
            } else {
                None
            };
            let manifest_id = if builtin_proxy {
                local
                    .manifest_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            } else {
                Some(required_plugin_runtime_text(
                    local.manifest_id.as_deref(),
                    resource.id.as_str(),
                    "manifest_id",
                )?)
            };
            let secret = service
                .config
                .local_connector_internal_api_secret
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required".to_string()
                })?;
            let owner_user_id = normalized_task_owner_user_id(task)
                .ok_or_else(|| "task owner user id is required".to_string())?;
            let mut url = format!(
                "{}/api/local-connectors/relay/{}/mcp",
                local_connector_service_base_url().trim_end_matches('/'),
                urlencoding::encode(device_id)
            );
            if let Some(workspace_id) = workspace_id {
                url.push_str("?workspace_id=");
                url.push_str(urlencoding::encode(workspace_id).as_ref());
                if let Some(cwd) = local
                    .relative_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    url.push_str("&cwd=");
                    url.push_str(urlencoding::encode(cwd).as_ref());
                }
            }
            let mut headers = resource.runtime.headers.clone();
            headers.insert(
                "x-local-connector-internal-secret".to_string(),
                secret.to_string(),
            );
            headers.insert("x-local-connector-owner-user-id".to_string(), owner_user_id);
            headers.insert(
                "x-plugin-management-resource-id".to_string(),
                resource.id.clone(),
            );
            if let Some(manifest_id) = manifest_id {
                headers.insert(
                    "x-local-connector-mcp-manifest-id".to_string(),
                    manifest_id.to_string(),
                );
            }
            let server = McpHttpServer::new(server_name.clone(), url)
                .with_headers(headers.into_iter().collect());
            Ok(LoadedPluginMcpServer {
                name: server_name,
                transport: "local_connector".to_string(),
                http_server: Some(server),
                stdio_server: None,
            })
        }
        other => Err(format!(
            "plugin MCP {} uses unsupported Task Runner runtime kind: {other}",
            resource.id
        )),
    }
}

fn required_plugin_runtime_text<'a>(
    value: Option<&'a str>,
    resource_id: &str,
    field: &str,
) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("plugin MCP {resource_id} is missing {field}"))
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
mod tests {
    use super::*;
    use crate::models::{TaskMcpConfig, TaskScheduleConfig};

    fn sample_task(enabled_builtin_kinds: Vec<&str>) -> TaskRecord {
        let now = now_rfc3339();
        let mut mcp_config = TaskMcpConfig::default();
        mcp_config.enabled = true;
        mcp_config.enabled_builtin_kinds = enabled_builtin_kinds
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
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
            task_profile: "default".to_string(),
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
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }

    fn external_stdio_config(cwd: Option<&str>) -> ExternalMcpConfigRecord {
        ExternalMcpConfigRecord {
            id: "external-stdio-1".to_string(),
            name: "Local Tool".to_string(),
            transport: "stdio".to_string(),
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            url: None,
            headers: Default::default(),
            env: Default::default(),
            cwd: cwd.map(ToOwned::to_owned),
            enabled: true,
            creator_user_id: Some("creator-user".to_string()),
            creator_username: Some("creator".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-user".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn task_stdio_server_binds_task_user_and_effective_workspace() {
        let config = external_stdio_config(Some("/opt/chatos/internal/workspace"));

        let server =
            task_stdio_server_for_config(&config, " user-123 ", "/srv/chatos/workspaces/user-123")
                .expect("stdio server should be valid")
                .expect("stdio server");

        assert_eq!(server.user_id.as_deref(), Some("user-123"));
        assert_eq!(
            server.cwd.as_deref(),
            Some("/srv/chatos/workspaces/user-123")
        );
    }

    #[test]
    fn task_stdio_server_rejects_missing_task_user() {
        let config = external_stdio_config(None);

        let err = task_stdio_server_for_config(&config, " ", "/srv/chatos/workspaces/user-123")
            .expect_err("stdio server should require task user");

        assert!(err.contains("task subject user id"));
    }

    #[test]
    fn task_stdio_server_rejects_missing_workspace() {
        let config = external_stdio_config(None);

        let err = task_stdio_server_for_config(&config, "user-123", " ")
            .expect_err("stdio server should require workspace");

        assert!(err.contains("effective workspace"));
    }

    #[test]
    fn internal_host_mcp_prompt_is_not_exposed_as_external_mcp() {
        let items = external_mcp_prefixed_input_items(
            &[ExternalMcpRuntimeSummary {
                id: "ephemeral:local_connector".to_string(),
                name: "local_connector".to_string(),
                transport: "http".to_string(),
            }],
            BuiltinMcpPromptLocale::ZhCn,
        );

        assert!(items.is_empty());
    }

    #[test]
    fn external_mcp_prompt_omits_internal_host_tool_names() {
        let items = external_mcp_prefixed_input_items(
            &[
                ExternalMcpRuntimeSummary {
                    id: "ephemeral:local_connector".to_string(),
                    name: "local_connector".to_string(),
                    transport: "http".to_string(),
                },
                ExternalMcpRuntimeSummary {
                    id: "external-1".to_string(),
                    name: "Issue Tracker".to_string(),
                    transport: "stdio".to_string(),
                },
            ],
            BuiltinMcpPromptLocale::ZhCn,
        );

        let text = items[0]
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("prompt text");

        assert!(text.contains("Issue Tracker"));
        assert!(!text.contains("local_connector"));
        assert!(!text.contains("harness_code"));
        assert!(!text.contains("local_connector_read_file_raw"));
        assert!(!text.contains("harness_code_read_file_raw"));
    }

    #[test]
    fn internal_host_tool_aliases_use_stable_builtin_server_prefixes() {
        let headers = std::collections::BTreeMap::from([(
            chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
            "CodeMaintainerRead,TerminalController,BrowserTools".to_string(),
        )]);

        let aliases = hosted_builtin_tool_name_aliases("local_connector", &headers);

        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "read_file_raw"
                && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_READ_SERVER_NAME
        }));
        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "execute_command"
                && alias.public_server_name == chatos_mcp_runtime::TERMINAL_CONTROLLER_SERVER_NAME
        }));
        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "browser_navigate"
                && alias.public_server_name == chatos_mcp_runtime::BROWSER_TOOLS_SERVER_NAME
        }));
    }

    #[test]
    fn sandbox_terminal_aliases_use_stable_builtin_server_prefixes() {
        let task = sample_task(vec!["TerminalController"]);

        let aliases = sandbox_tool_name_aliases(&task);

        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "execute_command"
                && alias.public_server_name == chatos_mcp_runtime::TERMINAL_CONTROLLER_SERVER_NAME
        }));
        assert!(!aliases
            .iter()
            .any(|alias| alias.tool_name == "read_file_raw"));
        assert!(!aliases.iter().any(|alias| alias.tool_name == "write_file"));
    }

    #[test]
    fn sandbox_write_aliases_include_read_dependency() {
        let task = sample_task(vec!["CodeMaintainerWrite"]);

        let aliases = sandbox_tool_name_aliases(&task);

        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "read_file_raw"
                && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_READ_SERVER_NAME
        }));
        assert!(aliases.iter().any(|alias| {
            alias.tool_name == "write_file"
                && alias.public_server_name == chatos_mcp_runtime::CODE_MAINTAINER_WRITE_SERVER_NAME
        }));
        assert!(!aliases
            .iter()
            .any(|alias| alias.tool_name == "execute_command"));
    }
}
