use super::*;

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
) -> Result<LoadedExternalMcpServers, String> {
    if !task.mcp_config.enabled || task.mcp_config.external_mcp_config_ids.is_empty() {
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
        } else if let Some(server) = config.to_stdio_server() {
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
    Ok(LoadedExternalMcpServers {
        http_servers,
        stdio_servers,
        summaries,
    })
}

pub(super) fn load_system_http_mcp_servers(
    service: &RunService,
    task: &TaskRecord,
) -> Result<Vec<McpHttpServer>, String> {
    let _ = (service, task);
    Ok(Vec::new())
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
    let text = if locale.is_english() {
        format!(
            "[External MCP]\nTask Runner has loaded these user-configured external MCP servers for this task:\n{list}\n\nIf the task objective asks you to use these external systems, directly call the corresponding tools currently exposed to you. External MCP tool names usually use the config name as their prefix. Do not inspect local Gemini/Codex/Claude MCP config files to decide whether these MCP servers exist; they are injected by Task Runner for this run. Use builtin tools only when the task also needs local code, terminal, browser, or other builtin capabilities."
        )
    } else {
        format!(
            "[外部 MCP]\nTask Runner 已为当前任务加载这些用户配置的外部 MCP：\n{list}\n\n如果任务目标要求使用这些外部系统，请直接调用当前暴露给你的对应工具。外部 MCP 工具名通常会以配置名称作为前缀。不要检查本机 Gemini/Codex/Claude 的 MCP 配置文件来判断这些 MCP 是否存在；它们已经由 Task Runner 在本次运行中注入。只有当任务同时需要本地代码、终端、浏览器或其他 builtin 能力时，才使用 builtin 工具。"
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
