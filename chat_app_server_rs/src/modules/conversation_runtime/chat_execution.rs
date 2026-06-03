use serde_json::Value;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::services::agent_runtime::ai_server::{
    AiServer as AgentAiServer, ChatOptions as AgentChatOptions,
};
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute as AgentMcpToolExecute;
use crate::services::user_settings::apply_settings_to_ai_client;
use crate::utils::attachments::Attachment;

use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};
use super::task_board::{
    build_runtime_context, load_prefixed_input_items, TaskBoardRuntimeContext,
};

pub struct PreparedMcpExecution {
    pub executor: AgentMcpToolExecute,
    pub unavailable_tools: Vec<Value>,
    pub prefixed_input_items: Vec<Value>,
    pub tool_metadata: ToolMetadataMap,
}

pub struct ChatExecutionInput {
    pub use_tools: bool,
    pub max_tokens: Option<i64>,
    pub attachments: Vec<Attachment>,
    pub callbacks: crate::services::ai_client_common::AiClientCallbacks,
    pub turn_id: String,
    pub user_message_id: String,
    pub message_source: String,
}

pub fn init_agent_ai_server(model_runtime: &ResolvedChatModelConfig) -> AgentAiServer {
    AgentAiServer::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        AgentMcpToolExecute::new(Vec::new(), Vec::new(), Vec::new()),
    )
}

pub async fn prepare_mcp_execution(
    session_id: &str,
    turn_id: &str,
    runtime_context: &mut ResolvedConversationRuntimeContext,
    use_codex_gateway_mcp_passthrough: bool,
) -> PreparedMcpExecution {
    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle.clone();
    let mut executor =
        AgentMcpToolExecute::new(http_servers, stdio_servers, builtin_servers.clone());
    if runtime_context.use_tools {
        let _ = if use_codex_gateway_mcp_passthrough {
            executor.init_builtin_only().await
        } else {
            executor.init().await
        };
    }

    let unavailable_tools = executor.get_unavailable_tools();
    runtime_context.builtin_mcp_system_prompt = compose_effective_builtin_mcp_system_prompt(
        builtin_servers.as_slice(),
        executor.tool_metadata(),
        unavailable_tools.as_slice(),
        runtime_context.internal_context_locale,
    );
    let prefixed_input_items = if let Some(context) =
        build_task_board_runtime_context(session_id, turn_id, runtime_context)
    {
        load_prefixed_input_items(&context)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let tool_metadata = executor.tool_metadata().clone();

    PreparedMcpExecution {
        executor,
        unavailable_tools,
        prefixed_input_items,
        tool_metadata,
    }
}

pub fn build_task_board_runtime_context(
    session_id: &str,
    turn_id: &str,
    runtime_context: &ResolvedConversationRuntimeContext,
) -> Option<TaskBoardRuntimeContext> {
    build_runtime_context(
        Some(session_id.to_string()),
        Some(turn_id.to_string()),
        runtime_context.internal_context_locale,
        runtime_context.contact_system_prompt.clone(),
        runtime_context.builtin_mcp_system_prompt.clone(),
        runtime_context.command_system_prompt.clone(),
    )
}

pub fn configure_agent_ai_server(
    ai_server: &mut AgentAiServer,
    session_id: &str,
    turn_id: &str,
    runtime_context: &ResolvedConversationRuntimeContext,
    effective_settings: &Value,
    executor: AgentMcpToolExecute,
) {
    if runtime_context.base_system_prompt.is_some() {
        ai_server.set_system_prompt(runtime_context.base_system_prompt.clone());
    }
    ai_server.set_mcp_tool_execute(executor);
    let Some(refresh_context) =
        build_task_board_runtime_context(session_id, turn_id, runtime_context)
    else {
        apply_settings_to_ai_client(&mut ai_server.ai_client, effective_settings);
        return;
    };
    ai_server.ai_client.set_task_board_refresh_context(
        Some(refresh_context.session_id),
        refresh_context.turn_id,
        refresh_context.locale,
        refresh_context.contact_system_prompt,
        refresh_context.builtin_mcp_system_prompt,
        refresh_context.command_system_prompt,
    );
    apply_settings_to_ai_client(&mut ai_server.ai_client, effective_settings);
}

pub fn build_agent_chat_options(
    model_runtime: &ResolvedChatModelConfig,
    runtime_context: &ResolvedConversationRuntimeContext,
    prefixed_input_items: Vec<Value>,
    input: ChatExecutionInput,
) -> AgentChatOptions {
    AgentChatOptions {
        model: Some(model_runtime.model.clone()),
        provider: Some(model_runtime.provider.clone()),
        thinking_level: model_runtime.thinking_level.clone(),
        supports_responses: Some(model_runtime.supports_responses),
        temperature: Some(model_runtime.temperature),
        max_tokens: input.max_tokens,
        use_tools: Some(input.use_tools),
        attachments: Some(input.attachments),
        supports_images: Some(model_runtime.supports_images),
        reasoning_enabled: Some(model_runtime.effective_reasoning),
        callbacks: Some(input.callbacks),
        turn_id: Some(input.turn_id),
        user_message_id: Some(input.user_message_id),
        message_mode: Some("model".to_string()),
        message_source: Some(input.message_source),
        prefixed_input_items: Some(prefixed_input_items),
        request_cwd: if model_runtime.use_codex_gateway_mcp_passthrough {
            runtime_context.resolved_project_root.clone()
        } else {
            None
        },
        use_codex_gateway_mcp_passthrough: Some(model_runtime.use_codex_gateway_mcp_passthrough),
    }
}
