// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub use chatos_ai_runtime::{
    base_url_disallows_system_messages, base_url_requires_responses_input_list,
    build_responses_text_input, run_compatible_prompt_with, select_preferred_response_text,
    should_retry_transport_error, wrap_prompt_with_system_context, AiRuntime, AiRuntimeBuilder,
    AiRuntimeOptions, AiRuntimeResult, AiTurnReport, AiTurnStatus, ContextualTurnRequest,
    ContextualTurnRunner, McpRuntimeToolExecutor, MemoryRecordScope, MemoryScope, ModelRequest,
    ModelRuntimeConfig, RuntimeCallbacks, RuntimeRecordOptions, RuntimeTurnSpec, SaveRecordInput,
    SimplePromptOptions, TaskBuiltinMcpPromptMode, TaskBuiltinMcpPromptSnapshot, TaskMcpInitMode,
    TaskMemoryRuntimeConfig, TaskRunExecution, TaskRunReport, TaskRunSpec, TaskRuntime,
    TaskRuntimeBuilder, TaskRuntimeConfig,
};
pub use chatos_mcp::{
    build_shared_builtin_provider, build_shared_builtin_registry,
    build_shared_builtin_tool_service, AgentBuilderAgentSnapshot, AgentBuilderOptions,
    AgentBuilderService, AgentBuilderSkill, AgentBuilderStore, AgentBuilderStoreRef,
    AskUserDecision, AskUserOptions, AskUserPromptPayload, AskUserResponseSubmission,
    AskUserService, AskUserStore, AskUserStoreRef, AskUserStreamChunkCallback, BrowserToolsOptions,
    BrowserToolsService, CodeMaintainerOptions, CodeMaintainerService, MemoryCommandReaderOptions,
    MemoryCommandReaderService, MemoryFullPlugin, MemoryFullSkill, MemoryInlineSkill,
    MemoryPluginReaderOptions, MemoryPluginReaderService, MemoryReaderStore, MemoryReaderStoreRef,
    MemoryRuntimeCommand, MemoryRuntimeContext, MemoryRuntimePlugin, MemoryRuntimeSkill,
    MemorySkillReaderOptions, MemorySkillReaderService, NotepadBuiltinService, NotepadOptions,
    NotepadStore, NotepadStoreRef, RemoteConnectionControllerContext,
    RemoteConnectionControllerOptions, RemoteConnectionControllerService,
    RemoteConnectionControllerStore, RemoteConnectionControllerStoreRef, SharedBuiltinProvider,
    SharedBuiltinToolService, TaskDraft, TaskManagerOptions, TaskManagerService, TaskManagerStore,
    TaskManagerStoreRef, TaskOutcomeItem, TaskStreamChunkCallback, TaskUpdatePatch,
    TerminalControllerContext, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStore, TerminalControllerStoreRef, WebToolsOptions, WebToolsService,
};
pub use chatos_mcp_runtime::{
    builtin_servers_from_kinds, configurable_builtin_kinds, default_runtime_builtin_kinds,
    BuiltinMcpKind as SharedBuiltinMcpKind, BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale,
    BuiltinMcpServerOptions, McpBuiltinServer as SharedMcpBuiltinServer,
    McpExecutor as SharedMcpExecutor, McpExecutorBuilder as SharedMcpExecutorBuilder,
};

pub use crate::config::Config;
pub use crate::core::ai_model_config::ResolvedChatModelConfig;
pub use crate::services::agent_runtime::mcp_tool_execute::{McpToolExecute, ToolInfo, ToolResult};
pub use crate::services::agent_runtime::message_manager::MessageManager;
pub use crate::services::builtin_mcp::BuiltinMcpKind;
pub use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

pub fn build_mcp_tool_execute(
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
) -> McpToolExecute {
    McpToolExecute::new(http_servers, stdio_servers, builtin_servers)
}

pub fn build_shared_mcp_executor(
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
) -> SharedMcpExecutor {
    crate::services::shared_mcp_runtime::build_shared_mcp_executor(
        http_servers,
        stdio_servers,
        builtin_servers,
    )
}

pub async fn build_initialized_shared_mcp_executor(
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
) -> Result<SharedMcpExecutor, String> {
    let mut executor = build_shared_mcp_executor(http_servers, stdio_servers, builtin_servers);
    executor.init().await?;
    Ok(executor)
}

pub fn build_shared_mcp_executor_from_shared_builtin_servers(
    builtin_servers: Vec<SharedMcpBuiltinServer>,
) -> Result<SharedMcpExecutor, String> {
    let mut converted = Vec::with_capacity(builtin_servers.len());
    for server in builtin_servers {
        converted.push(crate::services::shared_mcp_runtime::chatos_builtin_server(
            server,
        )?);
    }
    Ok(build_shared_mcp_executor(Vec::new(), Vec::new(), converted))
}

pub fn build_shared_mcp_executor_from_builtin_kinds(
    kinds: Vec<SharedBuiltinMcpKind>,
    options: BuiltinMcpServerOptions,
) -> Result<SharedMcpExecutor, String> {
    build_shared_mcp_executor_from_shared_builtin_servers(builtin_servers_from_kinds(
        kinds, &options,
    ))
}

pub fn build_initialized_shared_builtin_mcp_executor(
    kinds: Vec<SharedBuiltinMcpKind>,
    options: BuiltinMcpServerOptions,
) -> Result<SharedMcpExecutor, String> {
    let mut executor = build_shared_mcp_executor_from_builtin_kinds(kinds, options)?;
    executor.init_builtin_only()?;
    Ok(executor)
}

pub fn build_mcp_tool_execute_from_shared_builtin_servers(
    builtin_servers: Vec<SharedMcpBuiltinServer>,
) -> Result<McpToolExecute, String> {
    let mut converted = Vec::with_capacity(builtin_servers.len());
    for server in builtin_servers {
        converted.push(crate::services::shared_mcp_runtime::chatos_builtin_server(
            server,
        )?);
    }
    Ok(build_mcp_tool_execute(Vec::new(), Vec::new(), converted))
}

pub fn build_mcp_tool_execute_from_builtin_kinds(
    kinds: Vec<SharedBuiltinMcpKind>,
    options: BuiltinMcpServerOptions,
) -> Result<McpToolExecute, String> {
    build_mcp_tool_execute_from_shared_builtin_servers(builtin_servers_from_kinds(kinds, &options))
}

pub fn compose_shared_builtin_mcp_system_prompt(
    builtin_servers: &[SharedMcpBuiltinServer],
    locale: BuiltinMcpPromptLocale,
) -> Option<String> {
    chatos_mcp_runtime::compose_builtin_mcp_system_prompt(builtin_servers, locale)
}

pub fn inspect_shared_builtin_mcp_system_prompt(
    builtin_servers: &[SharedMcpBuiltinServer],
    locale: BuiltinMcpPromptLocale,
) -> BuiltinMcpPromptBuildResult {
    chatos_mcp_runtime::inspect_builtin_mcp_system_prompt(builtin_servers, locale)
}

pub fn compose_effective_shared_builtin_mcp_system_prompt(
    executor: &SharedMcpExecutor,
    locale: BuiltinMcpPromptLocale,
) -> Option<String> {
    executor.compose_effective_builtin_mcp_system_prompt(locale)
}

pub fn inspect_effective_shared_builtin_mcp_system_prompt(
    executor: &SharedMcpExecutor,
    locale: BuiltinMcpPromptLocale,
) -> BuiltinMcpPromptBuildResult {
    executor.inspect_effective_builtin_mcp_system_prompt(locale)
}

pub fn build_ai_runtime(tool_executor: Option<McpToolExecute>) -> AiRuntime {
    crate::services::shared_ai_runtime::build_shared_ai_runtime(tool_executor)
}

pub fn build_ai_runtime_from_shared_mcp_executor(
    executor: chatos_mcp_runtime::McpExecutor,
) -> AiRuntime {
    AiRuntime::from_mcp_executor(executor)
}

pub fn build_ai_runtime_with_chatos_records(
    tool_executor: Option<McpToolExecute>,
    message_manager: MessageManager,
) -> AiRuntime {
    crate::services::shared_ai_runtime::build_shared_ai_runtime_with_chatos_records(
        tool_executor,
        message_manager,
    )
}

pub fn build_contextual_turn_runner(
    tool_executor: Option<McpToolExecute>,
    message_manager: MessageManager,
) -> Result<ContextualTurnRunner, String> {
    crate::services::shared_ai_runtime::build_shared_contextual_turn_runner(
        tool_executor,
        message_manager,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_model_request(
    base_url: String,
    api_key: String,
    model: String,
    provider: String,
    input: serde_json::Value,
    tools: Vec<serde_json::Value>,
    supports_responses: bool,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    thinking_level: Option<String>,
) -> ModelRequest {
    crate::services::shared_ai_runtime::shared_model_request(
        base_url,
        api_key,
        model,
        provider,
        input,
        tools,
        supports_responses,
        instructions,
        temperature,
        max_output_tokens,
        thinking_level,
    )
}

pub fn build_model_request_from_config(
    config: &ModelRuntimeConfig,
    input: serde_json::Value,
    tools: Vec<serde_json::Value>,
) -> ModelRequest {
    ModelRequest::from_runtime_config(config, input, tools)
}

pub fn build_model_runtime_config_from_resolved(
    resolved: &ResolvedChatModelConfig,
) -> ModelRuntimeConfig {
    crate::services::shared_ai_runtime::shared_model_runtime_config_from_resolved(resolved)
}

pub async fn resolve_model_runtime_config_for_request(
    requested_model_config_id: Option<&str>,
    request_model_cfg: Option<&serde_json::Value>,
    session_id: Option<&str>,
    user_id: Option<&str>,
    default_model: &str,
    request_reasoning_enabled: Option<bool>,
    respect_model_flags: bool,
) -> Result<ModelRuntimeConfig, String> {
    crate::services::shared_ai_runtime::resolve_shared_model_runtime_config_for_request(
        requested_model_config_id,
        request_model_cfg,
        session_id,
        user_id,
        default_model,
        request_reasoning_enabled,
        respect_model_flags,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub fn build_model_request_with_options(
    base_url: String,
    api_key: String,
    model: String,
    provider: String,
    input: serde_json::Value,
    tools: Vec<serde_json::Value>,
    supports_responses: bool,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    thinking_level: Option<String>,
    prompt_cache_key: Option<String>,
    request_cwd: Option<String>,
    include_prompt_cache_retention: bool,
    request_body_limit_bytes: Option<usize>,
) -> ModelRequest {
    crate::services::shared_ai_runtime::shared_model_request_with_options(
        base_url,
        api_key,
        model,
        provider,
        input,
        tools,
        supports_responses,
        instructions,
        temperature,
        max_output_tokens,
        thinking_level,
        prompt_cache_key,
        request_cwd,
        include_prompt_cache_retention,
        request_body_limit_bytes,
    )
}
