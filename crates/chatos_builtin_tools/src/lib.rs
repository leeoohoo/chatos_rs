pub mod agent_builder;
pub mod ask_user;
pub mod browser_command_support;
pub mod browser_runtime;
pub mod browser_tools;
pub mod bundled_tools;
pub mod code_maintainer;
pub mod memory_readers;
pub mod notepad;
pub mod provider;
pub mod remote_connection_controller;
pub mod task_manager;
pub mod terminal_controller;
pub mod web_tools;

pub(crate) mod browser_page_insights;
pub(crate) mod browser_page_state_view;
pub mod research_findings;
pub mod research_output;
pub mod research_payloads;
pub mod research_summary;
pub mod research_summary_view;
pub(crate) mod tool_registry;

pub use agent_builder::{
    AgentBuilderAgentSnapshot, AgentBuilderOptions, AgentBuilderService, AgentBuilderSkill,
    AgentBuilderStore, AgentBuilderStoreRef,
};
pub use ask_user::{
    AskUserDecision, AskUserOptions, AskUserPromptPayload, AskUserResponseSubmission,
    AskUserService, AskUserStore, AskUserStoreRef, AskUserStreamChunkCallback,
    ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
};
pub use browser_tools::{
    BrowserToolCallContext, BrowserToolsOptions, BrowserToolsService, BrowserVisionAdapter,
    BrowserVisionAdapterRef, BrowserVisionFailure, BrowserVisionRequest, BrowserVisionResponse,
};
pub use bundled_tools::{
    bundled_tool_path, discover_bundled_tool_dirs, path_with_bundled_tools,
    CHATOS_BUNDLED_TOOLS_DIR_ENV, CHATOS_BUNDLED_TOOLS_PATH_ENV,
};
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
    build_shared_builtin_provider, build_shared_builtin_registry,
    build_shared_builtin_tool_service, SharedBuiltinProvider, SharedBuiltinToolService,
};
pub use remote_connection_controller::{
    RemoteConnectionControllerContext, RemoteConnectionControllerOptions,
    RemoteConnectionControllerService, RemoteConnectionControllerStore,
    RemoteConnectionControllerStoreRef, DEFAULT_COMMAND_TIMEOUT_SECONDS, DEFAULT_MAX_OUTPUT_CHARS,
    DEFAULT_MAX_READ_FILE_BYTES, MAX_COMMAND_TIMEOUT_SECONDS,
};
pub use task_manager::{
    TaskDraft, TaskManagerOptions, TaskManagerService, TaskManagerStore, TaskManagerStoreRef,
    TaskOutcomeItem, TaskStreamChunkCallback, TaskUpdatePatch, REVIEW_TIMEOUT_MS_DEFAULT,
    TASK_NOT_FOUND_ERR,
};
pub use terminal_controller::{
    coerce_process_identifier, resolve_wait_timeout_ms, TerminalControllerContext,
    TerminalControllerOptions, TerminalControllerService, TerminalControllerStore,
    TerminalControllerStoreRef, PROCESS_LIST_MAX_LIMIT, PROCESS_POLL_MAX_LIMIT,
    PROCESS_WAIT_MAX_TIMEOUT_MS, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT,
    RECENT_LOGS_MAX_TERMINAL_LIMIT,
};
pub use web_tools::{WebToolsOptions, WebToolsService};
