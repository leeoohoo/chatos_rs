use super::provider::{TaskRunnerBuiltinProvider, TaskRunnerBuiltinToolService};
use super::*;

pub(in crate::services) fn build_task_runner_builtin_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
) -> Result<Option<TaskRunnerBuiltinProvider>, String> {
    let Some(kind) = builtin_kind_by_any(server.kind.as_str()) else {
        return Ok(None);
    };
    let provider = match kind {
        chatos_mcp_runtime::BuiltinMcpKind::TaskManager => {
            build_task_manager_provider(server, task_service)?
        }
        chatos_mcp_runtime::BuiltinMcpKind::Notepad => {
            build_notepad_provider(server, task_service)?
        }
        chatos_mcp_runtime::BuiltinMcpKind::TerminalController => {
            build_terminal_controller_provider(server)?
        }
        chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
            build_remote_connection_controller_provider(server, task_service)?
        }
        chatos_mcp_runtime::BuiltinMcpKind::UiPrompter => {
            build_ui_prompter_provider(server, ui_prompt_service)?
        }
        _ => return Ok(build_shared_provider(server)?),
    };
    Ok(Some(provider))
}

fn build_task_manager_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
) -> Result<TaskRunnerBuiltinProvider, String> {
    let service = TaskManagerService::new(TaskManagerOptions {
        server_name: server.name.clone(),
        review_timeout_ms: REVIEW_TIMEOUT_MS_DEFAULT,
        auto_create_task: true,
        store: TaskManagerStoreRef::new(Arc::new(TaskRunnerTaskManagerStore::new(task_service))),
    })?;
    Ok(TaskRunnerBuiltinProvider::new(
        server.name.clone(),
        TaskRunnerBuiltinToolService::TaskManager(service),
    ))
}

fn build_notepad_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
) -> Result<TaskRunnerBuiltinProvider, String> {
    let user_id = server
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task_runner");
    let root = PathBuf::from(&task_service.config.default_workspace_dir)
        .join(".task_runner")
        .join("notepad");
    let store = TaskRunnerNotepadStore::new(root, user_id)?;
    let service = NotepadBuiltinService::new(NotepadOptions {
        server_name: server.name.clone(),
        store: NotepadStoreRef::new(Arc::new(store)),
    })?;
    Ok(TaskRunnerBuiltinProvider::new(
        server.name.clone(),
        TaskRunnerBuiltinToolService::Notepad(service),
    ))
}

fn build_terminal_controller_provider(
    server: &McpBuiltinServer,
) -> Result<TaskRunnerBuiltinProvider, String> {
    let service = TerminalControllerService::new(TerminalControllerOptions {
        root: PathBuf::from(&server.workspace_dir),
        user_id: server.user_id.clone(),
        project_id: server.project_id.clone(),
        idle_timeout_ms: 5_000,
        max_wait_ms: 60_000,
        max_output_chars: 20_000,
        store: TerminalControllerStoreRef::new(Arc::new(TaskRunnerTerminalControllerStore)),
    })?;
    Ok(TaskRunnerBuiltinProvider::new(
        server.name.clone(),
        TaskRunnerBuiltinToolService::TerminalController(service),
    ))
}

fn build_remote_connection_controller_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
) -> Result<TaskRunnerBuiltinProvider, String> {
    let service = RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
        server_name: server.name.clone(),
        user_id: server
            .user_id
            .clone()
            .or_else(|| Some(task_service.config.default_subject_id.clone())),
        default_remote_connection_id: server.remote_connection_id.clone(),
        command_timeout_seconds: 20,
        max_command_timeout_seconds: 120,
        max_output_chars: 20_000,
        max_read_file_bytes: 256 * 1024,
        store: RemoteConnectionControllerStoreRef::new(Arc::new(
            TaskRunnerRemoteConnectionStore::new(task_service.store.clone()),
        )),
    })?;
    Ok(TaskRunnerBuiltinProvider::new(
        server.name.clone(),
        TaskRunnerBuiltinToolService::Shared(SharedBuiltinToolService::RemoteConnectionController(
            service,
        )),
    ))
}

fn build_ui_prompter_provider(
    server: &McpBuiltinServer,
    ui_prompt_service: UiPromptService,
) -> Result<TaskRunnerBuiltinProvider, String> {
    let service = UiPrompterService::new(UiPrompterOptions {
        server_name: server.name.clone(),
        prompt_timeout_ms: UI_PROMPT_TIMEOUT_MS_DEFAULT,
        store: UiPrompterStoreRef::new(Arc::new(ui_prompt_service)),
    })?;
    Ok(TaskRunnerBuiltinProvider::new(
        server.name.clone(),
        TaskRunnerBuiltinToolService::UiPrompter(service),
    ))
}

fn build_shared_provider(
    server: &McpBuiltinServer,
) -> Result<Option<TaskRunnerBuiltinProvider>, String> {
    Ok(build_shared_builtin_tool_service(server)?.map(|service| {
        TaskRunnerBuiltinProvider::new(
            server.name.clone(),
            TaskRunnerBuiltinToolService::Shared(service),
        )
    }))
}
