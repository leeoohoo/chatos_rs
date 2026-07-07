// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
        }
        let mut http_server = McpHttpServer::new(server.name.clone(), server.url.clone());
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
        servers.push(context.to_mcp_server(task, run));
    }
    Ok(servers)
}

pub(super) fn external_mcp_prefixed_input_items(
    summaries: &[ExternalMcpRuntimeSummary],
    locale: BuiltinMcpPromptLocale,
) -> Vec<Value> {
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
    let has_local_connector = summaries
        .iter()
        .any(|summary| summary.name.trim().eq_ignore_ascii_case("local_connector"));
    let text = if locale.is_english() {
        let local_note = if has_local_connector {
            "\n\n[Local Connector]\nThis task is bound to the user's authorized local project through `local_connector`. Only the selected local capabilities are exposed. For project files, local commands, and browser automation, use the currently available `local_connector_*` tools with the same names and arguments as the builtin MCP tools, such as `local_connector_read_file_raw`, `local_connector_list_dir`, `local_connector_search_text`, `local_connector_write_file`, `local_connector_edit_file`, `local_connector_execute_command`, `local_connector_get_recent_logs`, `local_connector_process_list`, `local_connector_process_poll`, `local_connector_process_log`, `local_connector_process_wait`, `local_connector_process_write`, `local_connector_process_kill`, `local_connector_browser_navigate`, `local_connector_browser_snapshot`, `local_connector_browser_click`, `local_connector_browser_type`, `local_connector_browser_console`, `local_connector_browser_inspect`, and `local_connector_browser_research`. Use `local_connector_execute_command` for git commands such as `git status` or `git diff`; normal foreground commands reuse the task's primary local shell. For long-running commands such as dev servers, watchers, Docker Compose, or service startup, call `local_connector_execute_command` with `background=true`, then use the `local_connector_process_*` tools to inspect or control them. Browser tools operate on the user's paired local browser backend, not on the Task Runner server. Do not use server-local code, terminal, or browser tools for the project workspace unless the objective explicitly asks about the Task Runner server."
        } else {
            ""
        };
        format!(
            "[External MCP]\nTask Runner has loaded these user-configured external MCP servers for this task:\n{list}\n\nIf the task objective asks you to use these external systems, directly call the corresponding tools currently exposed to you. External MCP tool names usually use the config name as their prefix. Do not inspect local Gemini/Codex/Claude MCP config files to decide whether these MCP servers exist; they are injected by Task Runner for this run. Use builtin tools only when the task also needs local code, terminal, browser, or other builtin capabilities.{local_note}"
        )
    } else {
        let local_note = if has_local_connector {
            "\n\n[Local Connector]\n当前任务已通过 `local_connector` 绑定到用户授权的本地项目。只会暴露本次任务已选择的本地能力。涉及项目文件、本地命令和浏览器自动化时，使用当前可用的 `local_connector_*` 工具；这些工具名和入参与 builtin MCP 保持一致，例如 `local_connector_read_file_raw`、`local_connector_list_dir`、`local_connector_search_text`、`local_connector_write_file`、`local_connector_edit_file`、`local_connector_execute_command`、`local_connector_get_recent_logs`、`local_connector_process_list`、`local_connector_process_poll`、`local_connector_process_log`、`local_connector_process_wait`、`local_connector_process_write`、`local_connector_process_kill`、`local_connector_browser_navigate`、`local_connector_browser_snapshot`、`local_connector_browser_click`、`local_connector_browser_type`、`local_connector_browser_console`、`local_connector_browser_inspect`、`local_connector_browser_research`。Git 状态或 diff 请通过 `local_connector_execute_command` 执行 `git status`、`git diff` 等命令；普通前台命令会复用当前任务的本地主 shell。对于 dev server、watch、Docker Compose、服务启动等长时间运行命令，调用 `local_connector_execute_command` 时必须设置 `background=true`，然后使用 `local_connector_process_*` 工具查看或控制它们。浏览器工具操作的是用户配对的本地浏览器后端，不是 Task Runner 服务器上的浏览器。除非任务明确要求检查 Task Runner 服务器本身，否则不要使用服务器本机的代码、终端或浏览器工具操作项目工作区。"
        } else {
            ""
        };
        format!(
            "[外部 MCP]\nTask Runner 已为当前任务加载这些用户配置的外部 MCP：\n{list}\n\n如果任务目标要求使用这些外部系统，请直接调用当前暴露给你的对应工具。外部 MCP 工具名通常会以配置名称作为前缀。不要检查本机 Gemini/Codex/Claude 的 MCP 配置文件来判断这些 MCP 是否存在；它们已经由 Task Runner 在本次运行中注入。只有当任务同时需要本地代码、终端、浏览器或其他 builtin 能力时，才使用 builtin 工具。{local_note}"
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
    fn external_mcp_prompt_mentions_local_connector_tools() {
        let items = external_mcp_prefixed_input_items(
            &[ExternalMcpRuntimeSummary {
                id: "ephemeral:local_connector".to_string(),
                name: "local_connector".to_string(),
                transport: "http".to_string(),
            }],
            BuiltinMcpPromptLocale::ZhCn,
        );

        let text = items[0]
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("prompt text");

        assert!(text.contains("local_connector_read_file_raw"));
        assert!(text.contains("local_connector_execute_command"));
        assert!(text.contains("local_connector_browser_navigate"));
        assert!(text.contains("不要使用服务器本机"));
    }
}
