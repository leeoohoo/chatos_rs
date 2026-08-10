// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chatos_agent::{
    ChatosAgentProfile, ChatosStreamAgent, ChatosStreamRuntime, DEFAULT_AGENT_MAX_ITERATIONS,
};
#[cfg(test)]
use chatos_ai_runtime::{
    AiResponse, RuntimeFinalResponseAction, RuntimeFinalResponseContext, RuntimeIterationContext,
};
use chatos_ai_runtime::{
    AiRuntimeOptions, ContextualTurnRequest, ModelRuntimeConfig, RuntimeCallbacks,
    RuntimeLifecycleHook, RuntimeRecordOptions, SaveRecordInput,
};
use serde_json::{json, Value};
use tracing::info;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::ai_settings::request_body_limit_bytes_from_settings;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::core::internal_context_locale::InternalContextLocale;
#[cfg(test)]
use crate::modules::conversation_runtime::project_execution_planner::materialization_succeeded as project_execution_planner_terminal_tool_succeeded;
#[cfg(test)]
use crate::modules::conversation_runtime::task_board::TaskTurnFollowUpMode;
#[cfg(test)]
use crate::modules::conversation_runtime::task_board::TaskTurnReviewOutcome;
use crate::services::agent_runtime::ai_server::AiServer as AgentAiServer;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute as AgentMcpToolExecute;
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::{
    attach_ai_client_success_extra, build_ai_client_success_payload, build_user_content_parts,
    build_user_message_metadata, normalize_task_runner_async_plan_metadata,
    normalize_task_runner_async_tool_call_metadata, normalize_turn_id,
    TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
use crate::services::chatos_memory_engine::resolve_chatos_memory_scope;
use crate::services::shared_ai_runtime::{
    build_shared_contextual_turn_runner_with_max_iterations,
    shared_model_runtime_config_from_resolved,
};
use crate::utils::{abort_registry, attachments::Attachment};

use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};

#[path = "chat_execution/lifecycle.rs"]
mod lifecycle;

#[cfg(test)]
use lifecycle::assistant_response_input_item;
use lifecycle::{
    task_turn_review_metadata, track_project_execution_planner_completion,
    track_project_planning_integrity, ChatosRuntimeLifecycleHook, TaskTurnLifecycleState,
};

pub type ChatosAgentAiServer = ChatosStreamAgent<AgentAiServer>;

pub struct ChatosAgentExecutionOptions {
    use_tools: bool,
    attachments: Vec<Attachment>,
    turn_id: String,
    user_message_id: String,
    persisted_user_message_content: Option<String>,
    persisted_user_message_metadata: Option<Value>,
    message_mode: String,
    message_source: String,
    prefixed_input_items: Vec<Value>,
    shared_model_config: ModelRuntimeConfig,
    shared_max_iterations: usize,
    shared_runtime_callbacks: RuntimeCallbacks,
    shared_runtime_lifecycle: Arc<dyn RuntimeLifecycleHook>,
    task_turn: Arc<Mutex<TaskTurnLifecycleState>>,
    project_requirement_execution_planner: bool,
}

#[async_trait]
impl ChatosStreamRuntime for AgentAiServer {
    type Options = ChatosAgentExecutionOptions;
    type Output = Value;

    async fn execute(
        &mut self,
        conversation_id: &str,
        user_message: &str,
        options: Self::Options,
    ) -> Result<Self::Output, String> {
        let ChatosAgentExecutionOptions {
            use_tools,
            attachments,
            turn_id,
            user_message_id,
            persisted_user_message_content,
            persisted_user_message_metadata,
            message_mode,
            message_source,
            prefixed_input_items,
            shared_model_config,
            shared_max_iterations,
            shared_runtime_callbacks,
            shared_runtime_lifecycle,
            task_turn,
            project_requirement_execution_planner,
        } = options;
        let turn_id = normalize_turn_id(Some(turn_id.as_str()));
        let hidden_turn = persisted_user_message_metadata
            .as_ref()
            .and_then(|metadata| metadata.get("hidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let user_metadata = merge_user_record_metadata(
            persisted_user_message_metadata,
            build_user_message_metadata(&attachments, turn_id.as_deref()),
        );
        let current_input_items = vec![json!({
            "type": "message",
            "role": "user",
            "content": build_user_content_parts(
                shared_model_config.model.as_str(),
                user_message,
                attachments.as_slice(),
                shared_model_config.supports_images,
            ).await,
        })];
        let user_record = build_chatos_user_record(
            conversation_id,
            turn_id.clone(),
            user_message_id,
            persisted_user_message_content
                .as_deref()
                .unwrap_or(user_message),
            user_metadata,
            message_mode.as_str(),
            message_source.as_str(),
        );
        let record_options = build_chatos_record_options(
            message_mode.as_str(),
            message_source.as_str(),
            hidden_turn,
        );
        let abort_checker = Arc::new(|session_id: &str| abort_registry::is_aborted(session_id));
        let abort_token = abort_registry::abort_token_for_turn(conversation_id, turn_id.as_deref());
        let shared_runtime_callbacks =
            track_project_planning_integrity(shared_runtime_callbacks, Arc::clone(&task_turn));
        let shared_runtime_callbacks = if project_requirement_execution_planner {
            track_project_execution_planner_completion(
                shared_runtime_callbacks,
                Arc::clone(&task_turn),
            )
        } else {
            shared_runtime_callbacks
        };
        let runtime_options = AiRuntimeOptions::new(Some(conversation_id.to_string()), turn_id)
            .with_caller_model_runtime(Some(shared_model_config.to_tool_caller_model_runtime()))
            .with_abort_checker(Some(abort_checker))
            .with_abort_token(abort_token)
            .with_callbacks(shared_runtime_callbacks)
            .with_lifecycle_hook(Some(shared_runtime_lifecycle))
            .with_record_options(record_options);
        let runner = build_shared_contextual_turn_runner_with_max_iterations(
            use_tools.then(|| self.mcp_tool_execute.clone()),
            self.message_manager.clone(),
            shared_max_iterations,
        )?;
        let memory_scope = resolve_chatos_memory_scope(conversation_id).await?;
        let request = ContextualTurnRequest::from_model_config(
            &shared_model_config,
            runtime_options,
            current_input_items,
        )
        .with_memory_scope(memory_scope)
        .with_prefixed_input_items(prefixed_input_items)
        .with_user_record(Some(user_record));
        let result = runner.run_turn(request).await?;
        let payload = build_ai_client_success_payload(
            result.content,
            result.reasoning,
            result.finish_reason,
            0,
        );
        let task_turn = task_turn
            .lock()
            .map_err(|_| "task turn lifecycle state lock poisoned".to_string())?;
        let review_metadata = task_turn_review_metadata(&task_turn);
        Ok(attach_ai_client_success_extra(payload, review_metadata))
    }
}

fn merge_user_record_metadata(persisted: Option<Value>, generated: Option<Value>) -> Option<Value> {
    match (persisted, generated) {
        (Some(Value::Object(mut persisted)), Some(Value::Object(generated))) => {
            persisted.extend(generated);
            Some(Value::Object(persisted))
        }
        (Some(persisted), None) => Some(persisted),
        (None, generated) => generated,
        (Some(_), Some(generated)) => Some(generated),
    }
}

fn build_chatos_user_record(
    conversation_id: &str,
    turn_id: Option<String>,
    message_id: String,
    content: &str,
    metadata: Option<Value>,
    message_mode: &str,
    message_source: &str,
) -> SaveRecordInput {
    SaveRecordInput {
        conversation_id: conversation_id.to_string(),
        conversation_turn_id: turn_id,
        message_id: Some(message_id),
        role: "user".to_string(),
        content: content.to_string(),
        metadata,
        message_mode: Some(message_mode.to_string()),
        message_source: Some(message_source.to_string()),
        ..SaveRecordInput::default()
    }
}

fn build_chatos_record_options(
    message_mode: &str,
    message_source: &str,
    hidden_turn: bool,
) -> RuntimeRecordOptions {
    let task_runner_async_plan = message_mode.trim() == TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE;
    let with_visibility = |metadata: Option<Value>| {
        if !hidden_turn {
            return metadata;
        }
        let mut metadata = metadata
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        metadata.insert("hidden".to_string(), Value::Bool(true));
        Some(Value::Object(metadata))
    };
    RuntimeRecordOptions {
        persist_assistant_records: true,
        persist_tool_records: true,
        assistant_message_mode: Some(message_mode.to_string()),
        assistant_message_source: Some(message_source.to_string()),
        assistant_metadata: with_visibility(
            task_runner_async_plan
                .then(|| normalize_task_runner_async_plan_metadata(None))
                .flatten(),
        ),
        tool_message_mode: Some(message_mode.to_string()),
        tool_message_source: Some(message_source.to_string()),
        tool_metadata: with_visibility(
            task_runner_async_plan
                .then(|| normalize_task_runner_async_tool_call_metadata(None))
                .flatten(),
        ),
    }
}

pub fn shared_runtime_callbacks_from_chatos(callbacks: &AiClientCallbacks) -> RuntimeCallbacks {
    RuntimeCallbacks {
        on_chunk: callbacks.on_chunk.clone(),
        on_thinking: callbacks.on_thinking.clone(),
        on_tools_start: callbacks.on_tools_start.clone(),
        on_tools_stream: callbacks.on_tools_stream.clone(),
        on_tools_end: callbacks.on_tools_end.clone(),
        on_turn_phase: callbacks.on_turn_phase.clone(),
        on_runtime_guidance_applied: callbacks.on_runtime_guidance_applied.clone(),
        on_context_summarized_start: callbacks.on_context_summarized_start.clone(),
        on_context_summarized_stream: callbacks.on_context_summarized_stream.clone(),
        on_context_summarized_end: callbacks.on_context_summarized_end.clone(),
        on_before_model_input: callbacks.on_before_model_request.as_ref().map(|callback| {
            let callback = Arc::clone(callback);
            Arc::new(move |input: Value| callback(&input, None, None))
                as Arc<dyn Fn(Value) + Send + Sync>
        }),
        on_before_model_request: None,
        on_before_send_model_request: callbacks.on_before_send_model_request.clone(),
    }
}

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
    pub persisted_user_message_content: Option<String>,
    pub persisted_user_message_metadata: Option<Value>,
}

pub fn init_chatos_stream_agent(
    _model_runtime: &ResolvedChatModelConfig,
    profile: ChatosAgentProfile,
) -> ChatosAgentAiServer {
    ChatosStreamAgent::new(
        profile,
        AgentAiServer::new(AgentMcpToolExecute::new(Vec::new(), Vec::new(), Vec::new())),
    )
}

pub async fn prepare_mcp_execution(
    session_id: &str,
    turn_id: &str,
    runtime_context: &mut ResolvedConversationRuntimeContext,
    use_codex_gateway_mcp_passthrough: bool,
) -> Result<PreparedMcpExecution, String> {
    let started_at = Instant::now();
    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle.clone();
    let http_server_count = http_servers.len();
    let stdio_server_count = stdio_servers.len();
    let builtin_server_count = builtin_servers.len();
    let mut executor =
        AgentMcpToolExecute::new(http_servers, stdio_servers, builtin_servers.clone());
    if runtime_context.use_tools {
        let init_result = if use_codex_gateway_mcp_passthrough {
            executor.init_builtin_only().await
        } else {
            executor.init().await
        };
        init_result.map_err(|error| format!("initialize MCP Management tools failed: {error}"))?;
    }

    let unavailable_tools = executor.get_unavailable_tools();
    let available_tool_count = executor.get_available_tools().len();
    let tool_metadata_count = executor.tool_metadata().len();
    info!(
        session_id,
        turn_id,
        use_tools = runtime_context.use_tools,
        use_codex_gateway_mcp_passthrough,
        http_server_count,
        stdio_server_count,
        builtin_server_count,
        available_tool_count,
        unavailable_tool_count = unavailable_tools.len(),
        tool_metadata_count,
        mcp_prepare_ms = started_at.elapsed().as_millis(),
        "prepared chat MCP execution"
    );
    runtime_context.builtin_mcp_system_prompt = compose_effective_builtin_mcp_system_prompt(
        builtin_servers.as_slice(),
        executor.tool_metadata(),
        unavailable_tools.as_slice(),
        runtime_context.internal_context_locale,
    );
    let mut prefixed_input_items = Vec::new();
    push_optional_system_prompt(
        &mut prefixed_input_items,
        runtime_context.contact_system_prompt.as_deref(),
    );
    if let Some(workspace_prompt) = build_workspace_global_prompt(runtime_context) {
        prefixed_input_items.push(system_input_item(workspace_prompt.as_str()));
    }
    let tool_metadata = executor.tool_metadata().clone();

    Ok(PreparedMcpExecution {
        executor,
        unavailable_tools,
        prefixed_input_items,
        tool_metadata,
    })
}

pub fn effective_codex_gateway_mcp_passthrough(
    model_runtime: &ResolvedChatModelConfig,
    runtime_context: &ResolvedConversationRuntimeContext,
) -> bool {
    model_runtime.use_codex_gateway_mcp_passthrough
        && !runtime_context.project_requirement_execution_planner
        && runtime_context.mcp_server_bundle.0.iter().all(|server| {
            server.name != "mcp_management"
                && server.header_provider.is_none()
                && !server
                    .headers
                    .as_ref()
                    .is_some_and(chatos_mcp_runtime::rpc::headers_require_per_request_signing)
        })
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
    let project_name = normalize_prompt_text(runtime_context.resolved_project_name.as_deref())?;

    let mut lines = if runtime_context.internal_context_locale.is_english() {
        vec!["[Current Project And Runtime Context]".to_string()]
    } else {
        vec!["[当前项目与运行上下文]".to_string()]
    };
    lines.push(if runtime_context.internal_context_locale.is_english() {
        format!("Current project name: {project_name}")
    } else {
        format!("当前项目名称：{project_name}")
    });
    lines.push(if runtime_context.internal_context_locale.is_english() {
        "All project tool routing is already bound by the program. Do not ask for or infer provider, device, workspace, sandbox, lease, or connector identifiers.".to_string()
    } else {
        "所有项目工具路由均已由程序绑定。不得向用户索取或自行猜测 Provider、设备、Workspace、Sandbox、租约或 Connector 标识。".to_string()
    });
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

pub fn configure_chatos_stream_agent(
    agent: &mut ChatosAgentAiServer,
    _session_id: &str,
    _turn_id: &str,
    runtime_context: &ResolvedConversationRuntimeContext,
    _effective_settings: &Value,
    executor: AgentMcpToolExecute,
) {
    debug_assert_eq!(agent.profile(), runtime_context.agent_profile);
    let ai_server = agent.runtime_mut();
    ai_server.set_mcp_tool_execute(executor);
}

pub fn build_agent_chat_options(
    session_id: &str,
    model_runtime: &ResolvedChatModelConfig,
    runtime_context: &ResolvedConversationRuntimeContext,
    effective_settings: &Value,
    prefixed_input_items: Vec<Value>,
    input: ChatExecutionInput,
) -> ChatosAgentExecutionOptions {
    let mut shared_runtime_callbacks = shared_runtime_callbacks_from_chatos(&input.callbacks);
    if !model_runtime.effective_reasoning {
        shared_runtime_callbacks.on_thinking = None;
    }
    let task_turn = Arc::new(Mutex::new(TaskTurnLifecycleState {
        project_planning_integrity_guard: runtime_context.agent_profile.plan_mode_header()
            && !runtime_context.project_requirement_execution_planner,
        ..TaskTurnLifecycleState::default()
    }));
    let shared_runtime_lifecycle = Arc::new(ChatosRuntimeLifecycleHook {
        session_id: session_id.to_string(),
        turn_id: input.turn_id.clone(),
        model_name: model_runtime.model.clone(),
        supports_images: Some(model_runtime.supports_images),
        callbacks: input.callbacks.clone(),
        max_task_follow_up_rounds: task_follow_up_max_rounds_from_settings(effective_settings),
        task_turn: Arc::clone(&task_turn),
    }) as Arc<dyn RuntimeLifecycleHook>;
    let shared_model_config = shared_model_runtime_config_from_resolved(model_runtime)
        .with_instructions(compose_agent_instructions(runtime_context, model_runtime))
        .with_max_output_tokens(input.max_tokens)
        .with_prompt_cache_key(Some(session_id.to_string()))
        .with_request_cwd(None)
        .with_prompt_cache_retention(true)
        .with_request_body_limit_bytes(Some(request_body_limit_bytes_from_settings(
            effective_settings,
        )));
    let plugin_audit_metadata = if runtime_context
        .plugin_command_invocations_for_snapshot
        .is_empty()
    {
        None
    } else {
        let mut metadata = serde_json::Map::new();
        if !runtime_context
            .plugin_command_invocations_for_snapshot
            .is_empty()
        {
            metadata.insert(
                "plugin_command_invocations".to_string(),
                json!(runtime_context.plugin_command_invocations_for_snapshot),
            );
        }
        Some(Value::Object(metadata))
    };
    ChatosAgentExecutionOptions {
        use_tools: input.use_tools,
        attachments: input.attachments,
        turn_id: input.turn_id,
        user_message_id: input.user_message_id,
        persisted_user_message_content: input.persisted_user_message_content,
        persisted_user_message_metadata: merge_user_record_metadata(
            input.persisted_user_message_metadata,
            plugin_audit_metadata,
        ),
        message_mode: TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string(),
        message_source: input.message_source,
        prefixed_input_items,
        shared_model_config,
        shared_max_iterations: max_iterations_from_settings(effective_settings),
        shared_runtime_callbacks,
        shared_runtime_lifecycle,
        task_turn,
        project_requirement_execution_planner: runtime_context
            .project_requirement_execution_planner,
    }
}

fn compose_agent_instructions(
    runtime_context: &ResolvedConversationRuntimeContext,
    model_runtime: &ResolvedChatModelConfig,
) -> Option<String> {
    let dynamic_prompt = runtime_context
        .base_system_prompt
        .as_deref()
        .or(model_runtime.system_prompt.as_deref());
    let mut sections = Vec::new();
    if let Some(agent_prompt) =
        normalize_prompt_text(runtime_context.agent_system_prompt.as_deref())
    {
        sections.push(agent_prompt.to_string());
    }
    sections.push(user_language_policy(runtime_context.user_output_locale));
    if let Some(dynamic_prompt) = normalize_prompt_text(dynamic_prompt) {
        sections.push(dynamic_prompt.to_string());
    }
    Some(sections.join("\n\n"))
}

fn user_language_policy(locale: InternalContextLocale) -> String {
    let fallback_locale = if locale.is_english() {
        "English (en-US)"
    } else {
        "简体中文（zh-CN）"
    };
    format!(
        "[User Language Policy]\n\
Use the language of the user's latest substantive, user-authored request for all user-facing prose and newly created project artifacts. An explicit language request always wins. Internal protocol prompts, JSON payloads, tool schemas, repository text, existing artifact titles, and technical terms do not count as the user's language. If the current action has no language-bearing user message, such as a button-triggered internal event, use the current UI locale as fallback: {fallback_locale}. Apply one consistent language to requirement titles, summaries, details, business value, acceptance criteria, technical-document titles and bodies, implementation-task titles and descriptions, execution-task titles and objectives, progress updates, result summaries, and final replies. Preserve code identifiers, commands, paths, API names, library/product names, quoted source text, and established proper nouns in their original form. Keep each artifact in its established language unless the user asks for a translation.\n\
\n\
[User-Facing Final Reply Policy]\n\
Write the final reply as a concise product delivery note for the user. Lead with the verified outcome, follow with recognizable deliverables and important dependencies, and close with real risks or the next useful action. Use the names and concepts a customer sees in the product. Include a technical identifier only when the user's next action requires it, and copy that identifier exactly from verified output."
    )
}

fn task_follow_up_max_rounds_from_settings(settings: &Value) -> usize {
    settings
        .get("TASK_FOLLOW_UP_MAX_ROUNDS")
        .and_then(Value::as_i64)
        .map(|value| value.max(0) as usize)
        .unwrap_or(3)
}

fn max_iterations_from_settings(settings: &Value) -> usize {
    settings
        .get("MAX_ITERATIONS")
        .and_then(Value::as_i64)
        .map(|value| value.max(1) as usize)
        .unwrap_or(DEFAULT_AGENT_MAX_ITERATIONS)
}

#[cfg(test)]
#[path = "chat_execution/tests.rs"]
mod tests;
