use super::*;

pub(in crate::services) fn build_builtin_registry(
    servers: &[McpBuiltinServer],
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
) -> (BuiltinToolRegistry, Vec<String>) {
    let mut registry = BuiltinToolRegistry::new();
    let mut errors = Vec::new();
    for server in servers {
        match build_task_runner_builtin_provider(
            server,
            task_service.clone(),
            ui_prompt_service.clone(),
        ) {
            Ok(Some(provider)) => registry.register(provider),
            Ok(None) => {}
            Err(err) => errors.push(format!("{} 初始化失败: {err}", server.name)),
        }
    }
    (registry, errors)
}
