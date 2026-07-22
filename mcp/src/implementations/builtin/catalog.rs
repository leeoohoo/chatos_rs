// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp_runtime::BuiltinMcpKind;
use serde_json::Value;

use crate::{
    build_shared_builtin_tool_service, AgentBuilderOptions, AgentBuilderService, AskUserDecision,
    AskUserOptions, AskUserPromptPayload, AskUserService, AskUserStore, AskUserStoreRef,
    AskUserStreamChunkCallback, BrowserToolsOptions, BrowserToolsService,
    MemoryCommandReaderOptions, MemoryCommandReaderService, MemoryFullPlugin, MemoryFullSkill,
    MemoryPluginReaderOptions, MemoryPluginReaderService, MemoryReaderStore, MemoryReaderStoreRef,
    MemoryRuntimeContext, MemorySkillReaderOptions, MemorySkillReaderService,
    NotepadBuiltinService, NotepadOptions, NotepadStore, NotepadStoreRef,
    RemoteConnectionControllerContext, RemoteConnectionControllerOptions,
    RemoteConnectionControllerService, RemoteConnectionControllerStore,
    RemoteConnectionControllerStoreRef, TaskDraft, TaskManagerOptions, TaskManagerService,
    TaskManagerStore, TaskManagerStoreRef, TaskStreamChunkCallback, TaskUpdatePatch,
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStore, TerminalControllerStoreRef, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
    DEFAULT_COMMAND_TIMEOUT_SECONDS, DEFAULT_MAX_OUTPUT_CHARS, DEFAULT_MAX_READ_FILE_BYTES,
    MAX_COMMAND_TIMEOUT_SECONDS, REVIEW_TIMEOUT_MS_DEFAULT,
};

#[derive(Debug, Default)]
struct SchemaOnlyStore;

fn schema_only_error() -> String {
    "schema-only tool catalog cannot execute tools".to_string()
}

pub fn builtin_tool_catalog(kind: BuiltinMcpKind) -> Result<Vec<Value>, String> {
    let store = Arc::new(SchemaOnlyStore);
    let server_name = kind.server_name().to_string();
    match kind {
        BuiltinMcpKind::CodeMaintainerRead
        | BuiltinMcpKind::CodeMaintainerWrite
        | BuiltinMcpKind::WebTools => {
            let server = kind.default_server(".");
            build_shared_builtin_tool_service(&server)?
                .map(|service| service.list_tools())
                .ok_or_else(|| format!("builtin tool service is unavailable: {}", kind.kind_name()))
        }
        BuiltinMcpKind::BrowserTools => BrowserToolsService::new(BrowserToolsOptions {
            server_name,
            workspace_dir: std::env::current_dir()
                .map_err(|err| format!("resolve schema catalog cwd failed: {err}"))?,
            schema_catalog_only: true,
            ..BrowserToolsOptions::default()
        })
        .map(|service| service.list_tools()),
        BuiltinMcpKind::TerminalController => {
            TerminalControllerService::new(TerminalControllerOptions {
                root: std::env::current_dir()
                    .map_err(|err| format!("resolve schema catalog cwd failed: {err}"))?,
                user_id: Some("schema-catalog".to_string()),
                project_id: Some("schema-catalog".to_string()),
                idle_timeout_ms: 1_000,
                max_wait_ms: 5_000,
                max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
                store: TerminalControllerStoreRef::new(store),
            })
            .map(|service| service.list_tools())
        }
        BuiltinMcpKind::TaskManager => TaskManagerService::new(TaskManagerOptions {
            server_name,
            review_timeout_ms: REVIEW_TIMEOUT_MS_DEFAULT,
            auto_create_task: false,
            expose_context_ids: false,
            store: TaskManagerStoreRef::new(store),
        })
        .map(|service| service.list_tools()),
        BuiltinMcpKind::ProjectManagement => {
            Ok(crate::project_management_contract::schemas::task_runner_builtin_tool_definitions())
        }
        BuiltinMcpKind::Notepad => NotepadBuiltinService::new(NotepadOptions {
            server_name,
            store: NotepadStoreRef::new(store),
        })
        .map(|service| service.list_tools()),
        BuiltinMcpKind::AgentBuilder => AgentBuilderService::new(AgentBuilderOptions {
            server_name,
            user_id: Some("schema-catalog".to_string()),
            store: None,
        })
        .map(|service| service.list_tools()),
        BuiltinMcpKind::AskUser => AskUserService::new(AskUserOptions {
            server_name,
            prompt_timeout_ms: ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
            store: AskUserStoreRef::new(store),
        })
        .map(|service| service.list_tools()),
        BuiltinMcpKind::RemoteConnectionController => {
            RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
                server_name,
                user_id: Some("schema-catalog".to_string()),
                default_remote_connection_id: None,
                command_timeout_seconds: DEFAULT_COMMAND_TIMEOUT_SECONDS,
                max_command_timeout_seconds: MAX_COMMAND_TIMEOUT_SECONDS,
                max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
                max_read_file_bytes: DEFAULT_MAX_READ_FILE_BYTES,
                store: RemoteConnectionControllerStoreRef::new(store),
            })
            .map(|service| service.list_tools())
        }
        BuiltinMcpKind::MemorySkillReader => {
            MemorySkillReaderService::new(MemorySkillReaderOptions {
                server_name,
                agent_id: "schema-catalog".to_string(),
                store: MemoryReaderStoreRef::new(store),
            })
            .map(|service| service.list_tools())
        }
        BuiltinMcpKind::MemoryCommandReader => {
            MemoryCommandReaderService::new(MemoryCommandReaderOptions {
                server_name,
                agent_id: "schema-catalog".to_string(),
                store: MemoryReaderStoreRef::new(store),
            })
            .map(|service| service.list_tools())
        }
        BuiltinMcpKind::MemoryPluginReader => {
            MemoryPluginReaderService::new(MemoryPluginReaderOptions {
                server_name,
                agent_id: "schema-catalog".to_string(),
                store: MemoryReaderStoreRef::new(store),
            })
            .map(|service| service.list_tools())
        }
    }
}

#[async_trait]
impl TerminalControllerStore for SchemaOnlyStore {
    async fn execute_command(
        &self,
        _context: TerminalControllerContext,
        _path: String,
        _command: String,
        _background: bool,
        _permissions: crate::TerminalCommandPermissions,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn get_recent_logs(
        &self,
        _context: TerminalControllerContext,
        _per_terminal_limit: i64,
        _terminal_limit: usize,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_list(
        &self,
        _context: TerminalControllerContext,
        _include_exited: bool,
        _limit: usize,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_poll(
        &self,
        _context: TerminalControllerContext,
        _terminal_id: String,
        _offset: Option<i64>,
        _limit: i64,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_log(
        &self,
        _context: TerminalControllerContext,
        _terminal_id: String,
        _offset: Option<i64>,
        _limit: i64,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_wait(
        &self,
        _context: TerminalControllerContext,
        _terminal_id: String,
        _timeout_ms: u64,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_write(
        &self,
        _context: TerminalControllerContext,
        _terminal_id: String,
        _data: String,
        _submit: bool,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn process_kill(
        &self,
        _context: TerminalControllerContext,
        _terminal_id: String,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
}

#[async_trait]
impl TaskManagerStore for SchemaOnlyStore {
    async fn create_tasks_for_turn(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
        _draft_tasks: Vec<TaskDraft>,
    ) -> Result<Vec<Value>, String> {
        Err(schema_only_error())
    }

    async fn review_and_create_tasks(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
        _draft_tasks: Vec<TaskDraft>,
        _timeout_ms: u64,
        _on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn list_tasks_for_context(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: Option<&str>,
        _include_done: bool,
        _limit: usize,
    ) -> Result<Vec<Value>, String> {
        Err(schema_only_error())
    }

    async fn update_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
        _patch: TaskUpdatePatch,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn complete_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
        _patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }

    async fn delete_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
    ) -> Result<bool, String> {
        Err(schema_only_error())
    }

    async fn task_board_updated_event(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
    ) -> Option<Value> {
        None
    }
}

#[async_trait]
impl NotepadStore for SchemaOnlyStore {
    async fn init(&self) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn list_folders(&self) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn create_folder(&self, _folder: &str) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn rename_folder(&self, _from: &str, _to: &str) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn delete_folder(&self, _folder: &str, _recursive: bool) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn list_notes(&self, _params: Value) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn create_note(&self, _params: Value) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn read_note(&self, _id: &str) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn update_note(&self, _params: Value) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn delete_note(&self, _id: &str) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn list_tags(&self) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn search_notes(&self, _params: Value) -> Result<Value, String> {
        Err(schema_only_error())
    }
}

#[async_trait]
impl AskUserStore for SchemaOnlyStore {
    async fn execute_prompt(
        &self,
        _payload: AskUserPromptPayload,
        _on_stream_chunk: Option<AskUserStreamChunkCallback>,
    ) -> Result<AskUserDecision, String> {
        Err(schema_only_error())
    }
}

#[async_trait]
impl RemoteConnectionControllerStore for SchemaOnlyStore {
    async fn list_connections(
        &self,
        _context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn test_connection(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn run_command(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _command: String,
        _timeout_seconds: Option<u64>,
        _allow_dangerous: bool,
        _max_output_chars: Option<usize>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn list_directory(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _path: Option<String>,
        _limit: Option<usize>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn read_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _path: String,
        _max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn download_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _path: String,
        _encoding: String,
        _max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
    async fn upload_file(
        &self,
        _context: RemoteConnectionControllerContext,
        _connection_id: Option<String>,
        _path: String,
        _content: String,
        _encoding: String,
        _create_parent_dirs: bool,
        _overwrite: bool,
    ) -> Result<Value, String> {
        Err(schema_only_error())
    }
}

#[async_trait]
impl MemoryReaderStore for SchemaOnlyStore {
    async fn get_agent_runtime_context(
        &self,
        _agent_id: &str,
    ) -> Result<Option<MemoryRuntimeContext>, String> {
        Err(schema_only_error())
    }
    async fn get_skill(
        &self,
        _user_id: &str,
        _skill_id: &str,
    ) -> Result<Option<MemoryFullSkill>, String> {
        Err(schema_only_error())
    }
    async fn get_skill_plugin(
        &self,
        _user_id: &str,
        _source: &str,
    ) -> Result<Option<MemoryFullPlugin>, String> {
        Err(schema_only_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_kind_has_a_non_empty_real_catalog() {
        use BuiltinMcpKind::*;
        for kind in [
            CodeMaintainerRead,
            CodeMaintainerWrite,
            TerminalController,
            TaskManager,
            ProjectManagement,
            Notepad,
            AgentBuilder,
            AskUser,
            RemoteConnectionController,
            WebTools,
            BrowserTools,
            MemorySkillReader,
            MemoryCommandReader,
            MemoryPluginReader,
        ] {
            let tools = builtin_tool_catalog(kind)
                .unwrap_or_else(|err| panic!("{}: {err}", kind.kind_name()));
            assert!(!tools.is_empty(), "{}", kind.kind_name());
            assert!(tools
                .iter()
                .all(|tool| tool.get("name").and_then(Value::as_str).is_some()));
        }
    }
}
