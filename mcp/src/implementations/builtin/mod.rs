// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod agent_builder;
pub mod ask_user;
pub mod bundled_tools;
pub mod catalog;
pub mod code_maintainer;
pub mod memory_readers;
pub mod notepad;
pub mod provider;
pub mod remote_connection_controller;
pub mod terminal_controller;
pub mod terminal_controller_response;
mod terminal_process;

pub mod research_summary_view;
pub(crate) mod tool_registry;

pub use agent_builder::{
    AgentBuilderOptions, AgentBuilderService, AgentBuilderSkill, AgentBuilderStore,
    AgentBuilderStoreRef,
};
pub use ask_user::{
    normalize_kv_fields, prepare_prompt, AskUserDecision, AskUserOptions, AskUserPromptPayload,
    AskUserResponseSubmission, AskUserService, AskUserStore, AskUserStoreRef,
    AskUserStreamChunkCallback, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
};
pub use bundled_tools::{
    bundled_tool_path, discover_bundled_tool_dirs, path_with_bundled_tools,
    CHATOS_BUNDLED_TOOLS_DIR_ENV, CHATOS_BUNDLED_TOOLS_PATH_ENV,
};
pub use catalog::builtin_tool_catalog;
pub use code_maintainer::{
    CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
};
pub use memory_readers::{
    MemoryCommandReaderOptions, MemoryCommandReaderService, MemoryFullPlugin, MemoryFullSkill,
    MemoryInlineSkill, MemoryPluginReaderOptions, MemoryPluginReaderService, MemoryReaderStore,
    MemoryReaderStoreRef, MemoryRuntimeCommand, MemoryRuntimeContext, MemoryRuntimePlugin,
    MemoryRuntimeSkill, MemorySkillReaderOptions, MemorySkillReaderService,
};
pub use notepad::{NotepadBuiltinService, NotepadOptions, NotepadStore, NotepadStoreRef};
pub use provider::{
    build_builtin_tool_service_with_dependencies, build_shared_builtin_provider,
    build_shared_builtin_registry, build_shared_builtin_tool_service,
    BuiltinToolServiceDependencies, SharedBuiltinProvider, SharedBuiltinToolService,
};
pub use remote_connection_controller::{
    RemoteConnectionControllerContext, RemoteConnectionControllerOptions,
    RemoteConnectionControllerService, RemoteConnectionControllerStore,
    RemoteConnectionControllerStoreRef, DEFAULT_COMMAND_TIMEOUT_SECONDS, DEFAULT_MAX_OUTPUT_CHARS,
    DEFAULT_MAX_READ_FILE_BYTES, MAX_COMMAND_TIMEOUT_SECONDS,
};
pub use terminal_controller::{
    coerce_process_identifier, resolve_wait_timeout_ms, TerminalCommandPermissions,
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStore, TerminalControllerStoreRef, PROCESS_LIST_MAX_LIMIT,
    PROCESS_POLL_MAX_LIMIT, PROCESS_WAIT_MAX_TIMEOUT_MS, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT,
    RECENT_LOGS_MAX_TERMINAL_LIMIT,
};
pub use terminal_controller_response::{
    terminal_process_list_entry, terminal_process_list_response, terminal_process_log_response,
    terminal_process_poll_response, terminal_process_wait_response, terminal_recent_logs_entry,
    terminal_recent_logs_response, terminal_result_scope, TerminalProcessPollDetails,
    TerminalProcessSnapshot, TerminalProcessWaitResponse, TerminalRecentLogsEntry,
};
pub use terminal_process::{configure_child_process_group, terminate_child_process_tree};
