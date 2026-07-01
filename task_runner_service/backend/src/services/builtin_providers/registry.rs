// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::builders::build_task_runner_builtin_provider_with_project_management_options;
use super::*;

pub(in crate::services) fn build_builtin_registry(
    servers: &[McpBuiltinServer],
    task_service: TaskService,
    ask_user_prompt_service: AskUserPromptService,
) -> (BuiltinToolRegistry, Vec<String>) {
    build_builtin_registry_with_project_management_options(
        servers,
        task_service,
        ask_user_prompt_service,
        None,
    )
}

pub(in crate::services) fn build_builtin_registry_with_project_management_options(
    servers: &[McpBuiltinServer],
    task_service: TaskService,
    ask_user_prompt_service: AskUserPromptService,
    project_management_execution_options: Option<ProjectManagementExecutionOptions>,
) -> (BuiltinToolRegistry, Vec<String>) {
    let mut registry = BuiltinToolRegistry::new();
    let mut errors = Vec::new();
    for server in servers {
        let execution_options = if server.name == chatos_mcp_runtime::PROJECT_MANAGEMENT_SERVER_NAME
        {
            project_management_execution_options.clone()
        } else {
            None
        };
        match build_task_runner_builtin_provider_with_project_management_options(
            server,
            task_service.clone(),
            ask_user_prompt_service.clone(),
            execution_options,
        ) {
            Ok(Some(provider)) => registry.register(provider),
            Ok(None) => {}
            Err(err) => errors.push(format!("{} 初始化失败: {err}", server.name)),
        }
    }
    (registry, errors)
}
