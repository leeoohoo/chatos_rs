// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl McpCatalogService {
    pub fn new(
        task_service: TaskService,
        ask_user_prompt_service: crate::ask_user_prompt_service::AskUserPromptService,
    ) -> Self {
        Self {
            task_service,
            ask_user_prompt_service,
        }
    }

    pub fn list_catalog(&self) -> Vec<McpCatalogEntry> {
        let server_options =
            BuiltinMcpServerOptions::new(self.task_service.config.default_workspace_dir.clone())
                .with_auto_create_task(true);
        let runtime_defaults = default_runtime_builtin_kinds()
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect::<Vec<_>>();
        configurable_builtin_kinds()
            .into_iter()
            .map(|kind| {
                let server = kind.server_with_options(&server_options);
                let guide = mcp_builtin_kind_guide(kind);
                let description = guide.description.to_string();
                let use_cases = guide
                    .use_cases
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>();
                let capabilities = guide
                    .capabilities
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>();
                match build_task_runner_builtin_provider(
                    &server,
                    self.task_service.clone(),
                    self.ask_user_prompt_service.clone(),
                ) {
                    Ok(Some(provider)) => {
                        let available_tool_names = provider
                            .list_tools()
                            .into_iter()
                            .filter_map(|tool| {
                                tool.get("name")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            })
                            .collect::<Vec<_>>();
                        let unavailable_tools = provider
                            .unavailable_tools()
                            .into_iter()
                            .map(|(name, reason)| McpUnavailableTool { name, reason })
                            .collect::<Vec<_>>();
                        McpCatalogEntry {
                            kind: kind.kind_name().to_string(),
                            server_name: kind.server_name().to_string(),
                            config_id: kind.config_id().map(ToOwned::to_owned),
                            command: kind.command().map(ToOwned::to_owned),
                            description,
                            use_cases,
                            capabilities,
                            implemented: true,
                            runtime_default: runtime_defaults
                                .iter()
                                .any(|value| value == kind.kind_name()),
                            default_allow_writes: kind.default_allow_writes(),
                            available_tool_names,
                            unavailable_tools,
                            message: match kind {
                                chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
                                    Some("服务器列表来自 Task Runner 的“服务器”页面".to_string())
                                }
                                chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement => {
                                    Some("规划任务运行时会自动启用；普通任务不可选择。".to_string())
                                }
                                _ => None,
                            },
                        }
                    }
                    Ok(None) => McpCatalogEntry {
                        kind: kind.kind_name().to_string(),
                        server_name: kind.server_name().to_string(),
                        config_id: kind.config_id().map(ToOwned::to_owned),
                        command: kind.command().map(ToOwned::to_owned),
                        description,
                        use_cases,
                        capabilities,
                        implemented: false,
                        runtime_default: runtime_defaults
                            .iter()
                            .any(|value| value == kind.kind_name()),
                        default_allow_writes: kind.default_allow_writes(),
                        available_tool_names: Vec::new(),
                        unavailable_tools: Vec::new(),
                        message: Some(
                            "当前共享运行时尚未独立接线这个 builtin provider".to_string(),
                        ),
                    },
                    Err(err) => McpCatalogEntry {
                        kind: kind.kind_name().to_string(),
                        server_name: kind.server_name().to_string(),
                        config_id: kind.config_id().map(ToOwned::to_owned),
                        command: kind.command().map(ToOwned::to_owned),
                        description,
                        use_cases,
                        capabilities,
                        implemented: true,
                        runtime_default: runtime_defaults
                            .iter()
                            .any(|value| value == kind.kind_name()),
                        default_allow_writes: kind.default_allow_writes(),
                        available_tool_names: Vec::new(),
                        unavailable_tools: Vec::new(),
                        message: Some(err),
                    },
                }
            })
            .collect()
    }
}
