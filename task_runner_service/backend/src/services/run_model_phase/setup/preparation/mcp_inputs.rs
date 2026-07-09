// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
    for config_id in &task.mcp_config.external_mcp_config_ids {
        let config = service
            .store
            .get_external_mcp_config(config_id)
            .await?
            .ok_or_else(|| format!("外部 MCP 配置不存在: {config_id}"))?;
        if !config.enabled {
            return Err(format!("外部 MCP 配置未启用: {config_id}"));
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
            return Err(format!("外部 MCP 配置无效: {config_id}"));
        }
        summaries.push(ExternalMcpRuntimeSummary {
            id: config.id,
            name: config.name,
            transport: config.transport,
        });
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
    locale: BuiltinMcpPromptLocale,
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
    let text = if locale.is_english() {
        format!(
            "[External MCP]\nTask Runner has loaded these user-configured external MCP servers for this task:\n{list}\n\nIf the task objective asks you to use these external systems, directly call the corresponding tools currently exposed to you. External MCP tool names usually use the config name as their prefix. Do not inspect local Gemini/Codex/Claude MCP config files to decide whether these MCP servers exist; they are injected by Task Runner for this run. Use builtin tools only when the task also needs code, terminal, browser, or other builtin capabilities."
        )
    } else {
        format!(
            "[外部 MCP]\nTask Runner 已为当前任务加载这些用户配置的外部 MCP：\n{list}\n\n如果任务目标要求使用这些外部系统，请直接调用当前暴露给你的对应工具。外部 MCP 工具名通常会以配置名称作为前缀。不要检查本机 Gemini/Codex/Claude 的 MCP 配置文件来判断这些 MCP 是否存在；它们已经由 Task Runner 在本次运行中注入。只有当任务同时需要代码、终端、浏览器或其他 builtin 能力时，才使用 builtin 工具。"
        )
    };

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

pub(super) async fn project_management_skill_prefixed_input_items(
    service: &RunService,
    task: &TaskRecord,
    locale: BuiltinMcpPromptLocale,
) -> Vec<Value> {
    if !super::is_chatos_plan_task(task) {
        return Vec::new();
    }
    match crate::services::project_management_api_client::get_project_management_skill(
        &service.config,
        locale,
    )
    .await
    {
        Ok(Some(skill)) => project_management_skill_prefixed_input_item(skill, locale)
            .into_iter()
            .collect(),
        Ok(None) => Vec::new(),
        Err(err) => {
            warn!(
                task_id = task.id.as_str(),
                "failed to load project management skill for plan task: {err}"
            );
            Vec::new()
        }
    }
}

pub(super) fn project_management_skill_prefixed_input_item(
    skill: crate::services::project_management_api_client::ProjectManagementSkillDocument,
    locale: BuiltinMcpPromptLocale,
) -> Option<Value> {
    let crate::services::project_management_api_client::ProjectManagementSkillDocument {
        name,
        locale: skill_locale,
        content,
    } = skill;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    let text = if locale.is_english() {
        format!(
            "[Project Management MCP Skill]\nTask Runner loaded this skill from the Project Management service. Follow it whenever you use `project_management_service_*` tools. The skill may mention unprefixed tool names such as `list_project_tasks`; in this Task Runner run, call the exposed prefixed form such as `project_management_service_list_project_tasks`. Skill: {name} ({skill_locale}).\n\n{content}"
        )
    } else {
        format!(
            "[Project Management MCP Skill]\nTask Runner 已从 Project Management 服务加载以下 skill。只要使用 `project_management_service_*` 工具，就必须遵循它。skill 中可能写的是 `list_project_tasks` 这类未加服务前缀的工具名；在本次 Task Runner 运行里，要调用实际暴露的前缀形式，例如 `project_management_service_list_project_tasks`。Skill: {name} ({skill_locale})。\n\n{content}"
        )
    };

    Some(json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    }))
}

pub(super) async fn user_skill_prefixed_input_items(
    service: &RunService,
    task: &TaskRecord,
    locale: BuiltinMcpPromptLocale,
    workspace_dir: &str,
) -> Vec<Value> {
    let skill_contexts = match service
        .runtime_skill_contexts_for_task(task, workspace_dir)
        .await
    {
        Ok(skill_contexts) => skill_contexts,
        Err(err) => {
            warn!(
                task_id = task.id.as_str(),
                "failed to load user skills for task run: {err}"
            );
            return Vec::new();
        }
    };
    user_skill_prefixed_input_item(skill_contexts.as_slice(), locale)
        .into_iter()
        .collect()
}

fn user_skill_prefixed_input_item(
    skill_contexts: &[crate::services::RuntimeSkillContext],
    locale: BuiltinMcpPromptLocale,
) -> Option<Value> {
    if skill_contexts.is_empty() {
        return None;
    }
    let body = skill_contexts
        .iter()
        .filter_map(|context| {
            let skill = &context.skill;
            let content = skill.content.trim();
            if content.is_empty() {
                return None;
            }
            let asset_note = context
                .package_runtime_path
                .as_deref()
                .map(|path| {
                    format!(
                        "\n\nAssets path: `{path}`\nPackage files: {} files, {} bytes.\nScripts and supporting files are available there, but Task Runner did not execute them during installation. Inspect scripts before running them.",
                        skill.package_file_count, skill.package_total_bytes
                    )
                })
                .unwrap_or_default();
            Some(format!(
                "## {} ({}, id: {}, locale: {}){}\n\n{}",
                skill.display_name, skill.name, skill.id, skill.locale, asset_note, content
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    if body.trim().is_empty() {
        return None;
    }
    let text = if locale.is_english() {
        format!(
            "[Task Runner Skills]\nTask Runner has loaded these skills for this run because they were explicitly selected on the task or are enabled with auto injection. Follow them when they are relevant to the task objective.\n\n{body}"
        )
    } else {
        format!(
            "[Task Runner Skills]\nTask Runner 已为本次运行加载以下 skills，因为它们被任务显式选择，或处于启用状态并开启了自动注入。当它们和任务目标相关时，请遵循这些说明。\n\n{body}"
        )
    };

    Some(json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    }))
}

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
