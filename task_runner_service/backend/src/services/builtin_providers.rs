use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use chatos_builtin_tools::{
    build_shared_builtin_tool_service, NotepadBuiltinService, NotepadOptions, NotepadStoreRef,
    RemoteConnectionControllerOptions, RemoteConnectionControllerService,
    RemoteConnectionControllerStoreRef, SharedBuiltinToolService, TaskManagerOptions,
    TaskManagerService, TaskManagerStoreRef, TaskStreamChunkCallback, TerminalControllerOptions,
    TerminalControllerService, TerminalControllerStoreRef, UiPrompterOptions, UiPrompterService,
    UiPrompterStoreRef, REVIEW_TIMEOUT_MS_DEFAULT, UI_PROMPT_TIMEOUT_MS_DEFAULT,
};
use chatos_mcp_runtime::{
    builtin_kind_by_any, BuiltinToolProvider, BuiltinToolRegistry, McpBuiltinServer,
    ToolCallContext, ToolStreamChunkCallback,
};

use crate::notepad_store::TaskRunnerNotepadStore;
use crate::remote_server_runtime::TaskRunnerRemoteConnectionStore;
use crate::terminal_store::TaskRunnerTerminalControllerStore;
use crate::ui_prompt_service::UiPromptService;

use super::task_manager_bridge::TaskRunnerTaskManagerStore;
use super::TaskService;

pub(super) fn build_builtin_registry(
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

#[derive(Clone)]
enum TaskRunnerBuiltinToolService {
    Shared(SharedBuiltinToolService),
    Notepad(NotepadBuiltinService),
    TaskManager(TaskManagerService),
    TerminalController(TerminalControllerService),
    UiPrompter(UiPrompterService),
}

impl TaskRunnerBuiltinToolService {
    fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::Shared(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::UiPrompter(service) => service.list_tools(),
        }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: &ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::Shared(service) => service.call_tool(name, args, context, on_stream_chunk),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::TaskManager(service) => {
                let callback = on_stream_chunk.map(|callback| -> TaskStreamChunkCallback {
                    Arc::new(move |chunk| callback(chunk))
                });
                service.call_tool(
                    name,
                    args,
                    context.conversation_id.as_deref(),
                    context.conversation_turn_id.as_deref(),
                    callback,
                )
            }
            Self::TerminalController(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::UiPrompter(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk.map(|callback| {
                    Arc::new(move |chunk| callback(chunk))
                        as chatos_builtin_tools::UiPromptStreamChunkCallback
                }),
            ),
        }
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::Shared(service) => service.unavailable_tools(),
            Self::Notepad(_) => Vec::new(),
            Self::TaskManager(_) => Vec::new(),
            Self::TerminalController(_) => Vec::new(),
            Self::UiPrompter(_) => Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct TaskRunnerBuiltinProvider {
    server_name: String,
    service: TaskRunnerBuiltinToolService,
}

impl TaskRunnerBuiltinProvider {
    fn new(server_name: impl Into<String>, service: TaskRunnerBuiltinToolService) -> Self {
        Self {
            server_name: server_name.into(),
            service,
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskRunnerBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.service.list_tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service
            .call_tool(name, args, &context, on_stream_chunk)
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}

pub(super) fn build_task_runner_builtin_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
) -> Result<Option<TaskRunnerBuiltinProvider>, String> {
    let Some(kind) = builtin_kind_by_any(server.kind.as_str()) else {
        return Ok(None);
    };
    match kind {
        chatos_mcp_runtime::BuiltinMcpKind::TaskManager => {
            let service = TaskManagerService::new(TaskManagerOptions {
                server_name: server.name.clone(),
                review_timeout_ms: REVIEW_TIMEOUT_MS_DEFAULT,
                auto_create_task: true,
                store: TaskManagerStoreRef::new(Arc::new(TaskRunnerTaskManagerStore::new(
                    task_service,
                ))),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::TaskManager(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::Notepad => {
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
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Notepad(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::TerminalController => {
            let service = TerminalControllerService::new(TerminalControllerOptions {
                root: PathBuf::from(&server.workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                idle_timeout_ms: 5_000,
                max_wait_ms: 60_000,
                max_output_chars: 20_000,
                store: TerminalControllerStoreRef::new(Arc::new(TaskRunnerTerminalControllerStore)),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::TerminalController(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
            let service =
                RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
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
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Shared(
                    SharedBuiltinToolService::RemoteConnectionController(service),
                ),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::UiPrompter => {
            let service = UiPrompterService::new(UiPrompterOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms: UI_PROMPT_TIMEOUT_MS_DEFAULT,
                store: UiPrompterStoreRef::new(Arc::new(ui_prompt_service)),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::UiPrompter(service),
            )))
        }
        _ => Ok(build_shared_builtin_tool_service(server)?.map(|service| {
            TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Shared(service),
            )
        })),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use chatos_mcp_runtime::McpBuiltinServer;

    use crate::config::{AppConfig, StoreMode};
    use crate::store::AppStore;

    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "task-runner-builtin-provider-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn test_config(default_workspace_dir: PathBuf) -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            store_mode: StoreMode::Memory,
            database_url: "memory://task_runner_service_test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: default_workspace_dir.to_string_lossy().to_string(),
            memory_timeout: Duration::from_millis(30_000),
            execution_timeout: Duration::from_millis(30_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 1_000,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
        }
    }

    #[tokio::test]
    async fn terminal_controller_uses_server_workspace_dir() {
        let default_workspace = unique_temp_dir("default");
        let task_workspace = unique_temp_dir("task");
        std::fs::create_dir_all(&default_workspace).expect("create default workspace");
        std::fs::create_dir_all(&task_workspace).expect("create task workspace");

        let config = test_config(default_workspace.clone());
        let store = AppStore::new(&config).await.expect("create store");
        let task_service = TaskService::new(config, store.clone());
        let ui_prompt_service = UiPromptService::new(store);
        let server = McpBuiltinServer {
            name: "terminal_controller".to_string(),
            kind: "TerminalController".to_string(),
            workspace_dir: task_workspace.to_string_lossy().to_string(),
            user_id: Some("user".to_string()),
            project_id: Some("task".to_string()),
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: true,
            allow_writes: true,
            max_file_bytes: 1_000,
            max_write_bytes: 1_000,
            search_limit: 10,
        };

        let provider = build_task_runner_builtin_provider(&server, task_service, ui_prompt_service)
            .expect("build provider")
            .expect("terminal provider");
        let tools = provider.list_tools();
        let execute = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("execute_command"))
            .expect("execute_command tool");
        let description = execute
            .get("description")
            .and_then(Value::as_str)
            .expect("tool description");

        assert!(description.contains(task_workspace.to_string_lossy().as_ref()));
        assert!(!description.contains(default_workspace.to_string_lossy().as_ref()));
    }
}
