// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod backend;
mod catalog;
mod contracts;
mod definition;
mod implementations;
mod provider;
mod skills;
mod system_tool_catalog;
mod tool_catalog;

pub use contracts::project_management as project_management_contract;
pub use implementations::builtin::{
    agent_browser_binary_path, build_shared_builtin_provider, build_shared_builtin_registry,
    build_shared_builtin_tool_service, builtin_tool_catalog, bundled_tool_path,
    coerce_process_identifier, configure_child_process_group, discover_bundled_tool_dirs,
    extract_patch_targets, normalize_kv_fields, path_with_bundled_tools, resolve_wait_timeout_ms,
    terminal_process_list_entry, terminal_process_list_response, terminal_process_log_response,
    terminal_process_poll_response, terminal_process_wait_response, terminal_recent_logs_entry,
    terminal_recent_logs_response, terminal_result_scope, terminate_child_process_tree,
    AgentBuilderAgentSnapshot, AgentBuilderOptions, AgentBuilderService, AgentBuilderSkill,
    AgentBuilderStore, AgentBuilderStoreRef, AskUserDecision, AskUserOptions, AskUserPromptPayload,
    AskUserResponseSubmission, AskUserService, AskUserStore, AskUserStoreRef,
    AskUserStreamChunkCallback, BrowserToolCallContext, BrowserToolsOptions, BrowserToolsService,
    BrowserVisionAdapter, BrowserVisionAdapterRef, BrowserVisionFailure, BrowserVisionRequest,
    BrowserVisionResponse, CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions,
    CodeMaintainerService, MemoryCommandReaderOptions, MemoryCommandReaderService,
    MemoryFullPlugin, MemoryFullSkill, MemoryInlineSkill, MemoryPluginReaderOptions,
    MemoryPluginReaderService, MemoryReaderStore, MemoryReaderStoreRef, MemoryRuntimeCommand,
    MemoryRuntimeContext, MemoryRuntimePlugin, MemoryRuntimeSkill, MemorySkillReaderOptions,
    MemorySkillReaderService, NotepadBuiltinService, NotepadOptions, NotepadStore, NotepadStoreRef,
    PatchTarget, RemoteConnectionControllerContext, RemoteConnectionControllerOptions,
    RemoteConnectionControllerService, RemoteConnectionControllerStore,
    RemoteConnectionControllerStoreRef, SharedBuiltinProvider, SharedBuiltinToolService, TaskDraft,
    TaskManagerOptions, TaskManagerService, TaskManagerStore, TaskManagerStoreRef, TaskOutcomeItem,
    TaskStreamChunkCallback, TaskUpdatePatch, TerminalCommandPermissions,
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStore, TerminalControllerStoreRef, TerminalProcessPollDetails,
    TerminalProcessSnapshot, TerminalProcessWaitResponse, TerminalRecentLogsEntry, WebToolsOptions,
    WebToolsService, AGENT_BROWSER_BIN_ENV, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
    CHATOS_BUNDLED_TOOLS_DIR_ENV, CHATOS_BUNDLED_TOOLS_PATH_ENV, DEFAULT_COMMAND_TIMEOUT_SECONDS,
    DEFAULT_MAX_OUTPUT_CHARS, DEFAULT_MAX_READ_FILE_BYTES, MAX_COMMAND_TIMEOUT_SECONDS,
    PROCESS_LIST_MAX_LIMIT, PROCESS_POLL_MAX_LIMIT, PROCESS_WAIT_MAX_TIMEOUT_MS,
    RECENT_LOGS_MAX_PER_TERMINAL_LIMIT, RECENT_LOGS_MAX_TERMINAL_LIMIT, REVIEW_TIMEOUT_MS_DEFAULT,
    TASK_NOT_FOUND_ERR,
};
pub use implementations::builtin::{
    agent_builder, ask_user, browser_command_support, browser_runtime, browser_tools,
    bundled_tools, code_maintainer, memory_readers, notepad, remote_connection_controller,
    research_findings, research_output, research_payloads, research_summary, research_summary_view,
    task_manager, terminal_controller, terminal_controller_response, web_tools,
};
pub(crate) use implementations::builtin::{
    browser_page_insights, browser_page_state_view, tool_registry,
};
pub use implementations::sandbox_images;

pub use backend::{SystemMcpBackend, SystemMcpHost};
pub use catalog::{
    system_mcp_catalog, system_mcp_descriptor, system_mcp_descriptor_by_any,
    system_mcp_descriptor_by_embedded_kind, system_mcp_descriptor_by_resource_id,
    system_mcp_descriptor_by_server_name, system_mcp_descriptor_for_record, SystemMcpDescriptor,
};
pub use chatos_plugin_management_sdk::{
    SystemMcpKey, LEGACY_BUILTIN_MCP_RUNTIME_KIND, SYSTEM_MCP_RUNTIME_KIND,
};
pub use definition::{CatalogSystemMcpDefinition, SystemMcpDefinition};
pub use implementations::{system_mcp_definition, system_mcp_definitions};
pub use provider::{ResolvedSystemMcpBackend, SystemMcpHostAdapter, SystemMcpResolveContext};
pub use skills::{system_mcp_provider_skills, SystemMcpProviderSkill};
pub use system_tool_catalog::local_command_approval_decision_tool_definition;
pub use tool_catalog::{system_mcp_static_tools, system_mcp_tool_catalog, SystemMcpToolCatalog};
