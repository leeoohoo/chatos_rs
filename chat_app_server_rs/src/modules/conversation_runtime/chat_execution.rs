use serde_json::{Value, json};

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::services::agent_runtime::ai_server::{
    AiServer as AgentAiServer, ChatOptions as AgentChatOptions,
};
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute as AgentMcpToolExecute;
use crate::services::ai_common::TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE;
use crate::services::user_settings::apply_settings_to_ai_client;
use crate::utils::attachments::Attachment;

use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};

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
    let _ = (session_id, turn_id);
    let mut prefixed_input_items = Vec::new();
    push_optional_system_prompt(
        &mut prefixed_input_items,
        runtime_context.contact_system_prompt.as_deref(),
    );
    push_optional_system_prompt(
        &mut prefixed_input_items,
        runtime_context.task_runner_skill_prompt.as_deref(),
    );
    if let Some(workspace_prompt) = build_workspace_global_prompt(runtime_context) {
        prefixed_input_items.push(system_input_item(workspace_prompt.as_str()));
    }
    let tool_metadata = executor.tool_metadata().clone();

    PreparedMcpExecution {
        executor,
        unavailable_tools,
        prefixed_input_items,
        tool_metadata,
    }
}

fn push_optional_system_prompt(items: &mut Vec<Value>, content: Option<&str>) {
    let Some(content) = normalize_prompt_text(content) else {
        return;
    };
    items.push(system_input_item(content));
}

fn build_workspace_global_prompt(
    runtime_context: &ResolvedConversationRuntimeContext,
) -> Option<String> {
    let workspace_root = normalize_prompt_text(runtime_context.workspace_root.as_deref());
    let project_root = normalize_prompt_text(runtime_context.resolved_project_root.as_deref());
    if workspace_root.is_none() && project_root.is_none() {
        return None;
    }

    let mut lines = vec!["[Runtime Workspace]".to_string()];
    if let Some(workspace_root) = workspace_root {
        lines.push(format!("Current workspace root: {workspace_root}"));
    }
    if let Some(project_root) = project_root {
        if Some(project_root) != normalize_prompt_text(runtime_context.workspace_root.as_deref()) {
            lines.push(format!("Current project root: {project_root}"));
        }
    }
    lines.push(
        "Use the current workspace as the default context for relative project and file references unless the user says otherwise."
            .to_string(),
    );
    Some(lines.join("\n"))
}

fn normalize_prompt_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn system_input_item(text: &str) -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{ "type": "input_text", "text": text }],
    })
}

pub fn configure_agent_ai_server(
    ai_server: &mut AgentAiServer,
    _session_id: &str,
    _turn_id: &str,
    runtime_context: &ResolvedConversationRuntimeContext,
    effective_settings: &Value,
    executor: AgentMcpToolExecute,
) {
    if runtime_context.base_system_prompt.is_some() {
        ai_server.set_system_prompt(runtime_context.base_system_prompt.clone());
    }
    ai_server.set_mcp_tool_execute(executor);
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
        message_mode: Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string()),
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
