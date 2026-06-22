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

mod builders;
mod provider;
mod registry;

pub(super) use self::builders::build_task_runner_builtin_provider;
pub(super) use self::registry::build_builtin_registry;

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
            user_service_jwt_secret: None,
            user_service_jwt_issuer: "user_service".to_string(),
            user_service_task_runner_audience: "task_runner".to_string(),
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
