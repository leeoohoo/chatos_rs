// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::builders::build_task_runner_builtin_provider;
use super::*;

pub(in crate::services) fn build_builtin_registry(
    servers: &[McpBuiltinServer],
    task_service: TaskService,
    ask_user_prompt_service: AskUserPromptService,
) -> (BuiltinToolRegistry, Vec<String>) {
    let mut registry = BuiltinToolRegistry::new();
    let mut errors = Vec::new();
    for server in servers {
        match build_task_runner_builtin_provider(
            server,
            task_service.clone(),
            ask_user_prompt_service.clone(),
        ) {
            Ok(Some(provider)) => registry.register(provider),
            Ok(None) => {}
            Err(err) => errors.push(format!("{} 初始化失败: {err}", server.name)),
        }
    }
    (registry, errors)
}
