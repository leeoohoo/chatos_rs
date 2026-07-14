// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chatos_agent::{ChatosAgentProfile, ChatosStreamAgent, ChatosStreamRuntime};
use chatos_ai_runtime::{
    AiResponse, AiRuntimeOptions, ContextualTurnRequest, ModelRuntimeConfig,
    RuntimeBeforeModelRequest, RuntimeCallbacks, RuntimeFinalResponseAction,
    RuntimeFinalResponseContext, RuntimeIterationContext, RuntimeLifecycleHook,
    RuntimeRecordOptions, SaveRecordInput,
};
use serde_json::{json, Value};
use tracing::info;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::ai_settings::request_body_limit_bytes_from_settings;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::modules::conversation_runtime::task_board::{
    build_task_turn_follow_up_directive, build_task_turn_follow_up_message,
    build_task_turn_review_retry_guidance, parse_task_turn_review_outcome,
    strip_task_turn_review_marker, TaskTurnFollowUpMode, TaskTurnReviewOutcome,
};
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

pub type ChatosAgentAiServer = ChatosStreamAgent<AgentAiServer>;

pub struct ChatosAgentExecutionOptions {
    use_tools: bool,
    attachments: Vec<Attachment>,
    turn_id: String,
    user_message_id: String,
    message_mode: String,
    message_source: String,
    prefixed_input_items: Vec<Value>,
    shared_model_config: ModelRuntimeConfig,
    shared_max_iterations: usize,
    shared_runtime_callbacks: RuntimeCallbacks,
    shared_runtime_lifecycle: Arc<dyn RuntimeLifecycleHook>,
    task_turn: Arc<Mutex<TaskTurnLifecycleState>>,
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
            message_mode,
            message_source,
            prefixed_input_items,
            shared_model_config,
            shared_max_iterations,
            shared_runtime_callbacks,
            shared_runtime_lifecycle,
            task_turn,
        } = options;
        let turn_id = normalize_turn_id(Some(turn_id.as_str()));
        let user_metadata = build_user_message_metadata(&attachments, turn_id.as_deref());
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
            user_message,
            user_metadata,
            message_mode.as_str(),
            message_source.as_str(),
        );
        let record_options =
            build_chatos_record_options(message_mode.as_str(), message_source.as_str());
        let abort_checker = Arc::new(|session_id: &str| abort_registry::is_aborted(session_id));
        let runtime_options = AiRuntimeOptions::new(Some(conversation_id.to_string()), turn_id)
            .with_caller_model_runtime(Some(shared_model_config.to_tool_caller_model_runtime()))
            .with_abort_checker(Some(abort_checker))
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

fn build_chatos_record_options(message_mode: &str, message_source: &str) -> RuntimeRecordOptions {
    let task_runner_async_plan = message_mode.trim() == TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE;
    RuntimeRecordOptions {
        persist_assistant_records: true,
        persist_tool_records: true,
        assistant_message_mode: Some(message_mode.to_string()),
        assistant_message_source: Some(message_source.to_string()),
        assistant_metadata: task_runner_async_plan
            .then(|| normalize_task_runner_async_plan_metadata(None))
            .flatten(),
        tool_message_mode: Some(message_mode.to_string()),
        tool_message_source: Some(message_source.to_string()),
        tool_metadata: task_runner_async_plan
            .then(|| normalize_task_runner_async_tool_call_metadata(None))
            .flatten(),
    }
}

struct ChatosRuntimeLifecycleHook {
    session_id: String,
    turn_id: String,
    model_name: String,
    supports_images: Option<bool>,
    callbacks: AiClientCallbacks,
    max_task_follow_up_rounds: usize,
    task_turn: Arc<Mutex<TaskTurnLifecycleState>>,
}

#[derive(Default)]
struct TaskTurnLifecycleState {
    follow_up_rounds: usize,
    mode: Option<TaskTurnFollowUpMode>,
    last_visible_response: Option<AiResponse>,
    review_locale: Option<InternalContextLocale>,
    review_attempted: bool,
    review_last_outcome: Option<TaskTurnReviewOutcome>,
    continuation_history: Vec<Value>,
}

impl ChatosRuntimeLifecycleHook {
    fn task_turn_state(&self) -> Result<std::sync::MutexGuard<'_, TaskTurnLifecycleState>, String> {
        self.task_turn
            .lock()
            .map_err(|_| "task turn lifecycle state lock poisoned".to_string())
    }

    fn emit_task_turn_phase(
        &self,
        phase: &'static str,
        mode: TaskTurnFollowUpMode,
        iteration: usize,
    ) {
        if let Some(callback) = &self.callbacks.on_turn_phase {
            callback(json!({
                "phase": phase,
                "reason": "task_follow_up",
                "task_follow_up_mode": match mode {
                    TaskTurnFollowUpMode::ContinueExecution => "continue",
                    TaskTurnFollowUpMode::ReviewExecution => "review",
                },
                "iteration": iteration,
            }));
        }
    }

    fn emit_task_turn_thinking(&self, mode: TaskTurnFollowUpMode) {
        if let Some(callback) = &self.callbacks.on_thinking {
            callback(match mode {
                TaskTurnFollowUpMode::ContinueExecution => {
                    "检测到尚未完成的任务，继续在同一轮执行。".to_string()
                }
                TaskTurnFollowUpMode::ReviewExecution => {
                    "任务看起来已完成，正在同一轮进行复查。".to_string()
                }
            });
        }
    }

    fn continue_with_response(
        state: &mut TaskTurnLifecycleState,
        response: &AiResponse,
        guidance: &str,
    ) -> Vec<Value> {
        if let Some(item) = assistant_response_input_item(response) {
            state.continuation_history.push(item);
        }
        state
            .continuation_history
            .extend(follow_up_message_items(guidance));
        state.continuation_history.clone()
    }

    fn handle_review_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        let outcome = parse_task_turn_review_outcome(context.response.content.as_str());
        let mut state = self.task_turn_state()?;
        state.review_attempted = true;
        state.review_last_outcome = Some(outcome);

        if outcome == TaskTurnReviewOutcome::Pass {
            let replacement = state
                .last_visible_response
                .clone()
                .unwrap_or_else(|| AiResponse {
                    content: strip_task_turn_review_marker(context.response.content.as_str()),
                    ..context.response.clone()
                });
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Replace(Box::new(replacement)));
        }

        if state.follow_up_rounds >= self.max_task_follow_up_rounds {
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }

        let locale = state.review_locale.unwrap_or(InternalContextLocale::ZhCn);
        state.follow_up_rounds += 1;
        state.mode = Some(TaskTurnFollowUpMode::ContinueExecution);
        let guidance = build_task_turn_review_retry_guidance(locale);
        let input_items =
            Self::continue_with_response(&mut state, &context.response, guidance.as_str());
        drop(state);

        self.emit_task_turn_phase(
            "execution",
            TaskTurnFollowUpMode::ContinueExecution,
            context.iteration,
        );
        self.emit_task_turn_thinking(TaskTurnFollowUpMode::ContinueExecution);
        Ok(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: "task_review_retry".to_string(),
        })
    }
}

#[async_trait]
impl RuntimeLifecycleHook for ChatosRuntimeLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        let input_items =
            crate::services::runtime_guidance_input::load_runtime_guidance_input_items(
                Some(self.session_id.as_str()),
                Some(self.turn_id.as_str()),
                false,
                self.model_name.as_str(),
                self.supports_images,
                &self.callbacks,
            )
            .await;
        let review_mode = matches!(
            self.task_turn_state()?.mode,
            Some(TaskTurnFollowUpMode::ReviewExecution)
        );
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_input_items(input_items)
            .with_stream_output(!review_mode)
            .with_tools_enabled(!review_mode))
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        if matches!(
            self.task_turn_state()?.mode,
            Some(TaskTurnFollowUpMode::ReviewExecution)
        ) {
            return self.handle_review_response(context);
        }

        if self.max_task_follow_up_rounds == 0 {
            return Ok(RuntimeFinalResponseAction::Accept);
        }

        let Some(directive) =
            build_task_turn_follow_up_directive(self.session_id.as_str(), self.turn_id.as_str())
                .await
        else {
            self.task_turn_state()?.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        };

        let mut state = self.task_turn_state()?;
        if state.follow_up_rounds >= self.max_task_follow_up_rounds {
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }
        state.last_visible_response = Some(context.response.clone());
        state.follow_up_rounds += 1;
        state.mode = Some(directive.mode);
        state.review_locale = Some(directive.locale);
        let input_items = Self::continue_with_response(
            &mut state,
            &context.response,
            directive.guidance.as_str(),
        );
        drop(state);

        let phase = match directive.mode {
            TaskTurnFollowUpMode::ContinueExecution => "execution",
            TaskTurnFollowUpMode::ReviewExecution => "review",
        };
        self.emit_task_turn_phase(phase, directive.mode, context.iteration);
        self.emit_task_turn_thinking(directive.mode);
        Ok(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: match directive.mode {
                TaskTurnFollowUpMode::ContinueExecution => "task_follow_up".to_string(),
                TaskTurnFollowUpMode::ReviewExecution => "task_review".to_string(),
            },
        })
    }

    async fn final_response_metadata(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<Option<Value>, String> {
        let state = self.task_turn_state()?;
        Ok(Some(task_turn_review_metadata(&state)))
    }
}

fn task_turn_review_metadata(state: &TaskTurnLifecycleState) -> Value {
    let outcome = match state.review_last_outcome {
        Some(TaskTurnReviewOutcome::Pass) => "pass",
        Some(TaskTurnReviewOutcome::NeedsMoreWork) => "needs_more_work",
        Some(TaskTurnReviewOutcome::Unknown) => "unknown",
        None => "not_attempted",
    };
    json!({
        "task_turn_review": {
            "attempted": state.review_attempted,
            "outcome": outcome,
            "rounds": state.follow_up_rounds,
        }
    })
}

fn assistant_response_input_item(response: &AiResponse) -> Option<Value> {
    let content = if response.content.trim().is_empty() {
        response.reasoning.as_deref().unwrap_or("").trim()
    } else {
        response.content.trim()
    };
    if content.is_empty() {
        return None;
    }
    Some(json!({
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": content }],
    }))
}

fn follow_up_message_items(guidance: &str) -> Vec<Value> {
    match build_task_turn_follow_up_message(guidance) {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        item => vec![item],
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
) -> PreparedMcpExecution {
    let started_at = Instant::now();
    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle.clone();
    let http_server_count = http_servers.len();
    let stdio_server_count = stdio_servers.len();
    let builtin_server_count = builtin_servers.len();
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

    PreparedMcpExecution {
        executor,
        unavailable_tools,
        prefixed_input_items,
        tool_metadata,
    }
}

pub fn effective_codex_gateway_mcp_passthrough(
    model_runtime: &ResolvedChatModelConfig,
    runtime_context: &ResolvedConversationRuntimeContext,
) -> bool {
    model_runtime.use_codex_gateway_mcp_passthrough
        && !runtime_context.project_requirement_execution_planner
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
    let project_id = normalize_prompt_text(runtime_context.resolved_project_id.as_deref())
        .filter(|value| *value != PUBLIC_PROJECT_ID);
    let project_name = normalize_prompt_text(runtime_context.resolved_project_name.as_deref());
    let project_source =
        normalize_prompt_text(runtime_context.resolved_project_source_type.as_deref());
    if workspace_root.is_none()
        && project_root.is_none()
        && project_id.is_none()
        && project_name.is_none()
    {
        return None;
    }

    let mut lines = if runtime_context.internal_context_locale.is_english() {
        vec!["[Current Project And Runtime Context]".to_string()]
    } else {
        vec!["[当前项目与运行上下文]".to_string()]
    };
    if let Some(project_name) = project_name {
        lines.push(if runtime_context.internal_context_locale.is_english() {
            format!("Current project name: {project_name}")
        } else {
            format!("当前项目名称：{project_name}")
        });
    }
    if let Some(project_id) = project_id {
        lines.push(if runtime_context.internal_context_locale.is_english() {
            format!("Current project id: {project_id}")
        } else {
            format!("当前项目 ID：{project_id}")
        });
    }
    if let Some(project_source) = project_source {
        lines.push(if runtime_context.internal_context_locale.is_english() {
            format!("Current project source type: {project_source}")
        } else {
            format!("当前项目来源类型：{project_source}")
        });
    }
    if let Some(workspace_root) = workspace_root {
        lines.push(if runtime_context.internal_context_locale.is_english() {
            format!("Current workspace root: {workspace_root}")
        } else {
            format!("当前工作目录：{workspace_root}")
        });
    }
    if let Some(project_root) = project_root {
        if Some(project_root) != normalize_prompt_text(runtime_context.workspace_root.as_deref()) {
            lines.push(if runtime_context.internal_context_locale.is_english() {
                format!("Current project root: {project_root}")
            } else {
                format!("当前项目目录：{project_root}")
            });
        }
    }
    if runtime_context.internal_context_locale.is_english() {
        lines.extend([
            "This conversation is explicitly bound to the current project. Treat references such as “this project”, “the code”, “look at it”, or “help me change it” as referring to this project unless the user says otherwise.".to_string(),
            "Task Runner is your internal asynchronous execution path, not a separate product the user must operate. The current user, project id, conversation, and available runtime context are automatically attached to tasks you create.".to_string(),
            "When a request requires inspecting, searching, modifying, running, testing, building, debugging, or validating the current project—or using real files, logs, terminal, browser, Local Connector, sandbox, MCP, or Skills—use Task Runner to create the necessary task instead of claiming that you cannot access the project or asking the user to paste code or provide the project path.".to_string(),
            "Use the Task Runner capability catalogs and list_available_skills when specialized tools or Skills may be needed. After arranging the work, briefly tell the user what follow-through you initiated; factual results will return through the normal callback flow.".to_string(),
        ]);
    } else {
        lines.extend([
            "当前对话已经明确绑定到上述项目。除非用户另有说明，用户提到“这个项目”“代码”“帮我看看”“帮我修改”等内容时，默认就是指当前项目。".to_string(),
            "Task Runner 是你自己的内部异步执行通道，不是需要用户操作的另一个产品。你创建任务时，当前用户、项目 ID、会话以及可用运行上下文都会由系统自动绑定。".to_string(),
            "当需求需要查看、搜索、理解、修改、运行、测试、构建、排障或验证当前项目，或者需要真实文件、日志、终端、浏览器、Local Connector、沙箱、MCP 或 Skills 时，必须通过 Task Runner 创建任务继续推进。不得仅因为主对话不能直接读取文件就声称无法查看，也不要要求用户再次粘贴代码或提供项目路径。".to_string(),
            "需要专门能力时，先使用 Task Runner 的能力目录和 list_available_skills 选择合适工具或 Skills。安排完成后，只需自然地告诉用户你已经开始推进哪些后续步骤；真实结果会通过正常回调链路返回。".to_string(),
        ]);
    }
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
    let use_codex_gateway_mcp_passthrough =
        effective_codex_gateway_mcp_passthrough(model_runtime, runtime_context);
    let mut shared_runtime_callbacks = shared_runtime_callbacks_from_chatos(&input.callbacks);
    if !model_runtime.effective_reasoning {
        shared_runtime_callbacks.on_thinking = None;
    }
    let task_turn = Arc::new(Mutex::new(TaskTurnLifecycleState::default()));
    let shared_runtime_lifecycle = Arc::new(ChatosRuntimeLifecycleHook {
        session_id: session_id.to_string(),
        turn_id: input.turn_id.clone(),
        model_name: model_runtime.model.clone(),
        supports_images: Some(model_runtime.supports_images),
        callbacks: input.callbacks.clone(),
        max_task_follow_up_rounds: task_follow_up_max_rounds_from_settings(effective_settings),
        task_turn: Arc::clone(&task_turn),
    }) as Arc<dyn RuntimeLifecycleHook>;
    let request_cwd = if use_codex_gateway_mcp_passthrough {
        runtime_context.resolved_project_root.clone()
    } else {
        None
    };
    let shared_model_config = shared_model_runtime_config_from_resolved(model_runtime)
        .with_instructions(
            runtime_context
                .base_system_prompt
                .clone()
                .or_else(|| model_runtime.system_prompt.clone()),
        )
        .with_max_output_tokens(input.max_tokens)
        .with_prompt_cache_key(Some(session_id.to_string()))
        .with_request_cwd(request_cwd.clone())
        .with_prompt_cache_retention(true)
        .with_request_body_limit_bytes(Some(request_body_limit_bytes_from_settings(
            effective_settings,
        )));
    ChatosAgentExecutionOptions {
        use_tools: input.use_tools,
        attachments: input.attachments,
        turn_id: input.turn_id,
        user_message_id: input.user_message_id,
        message_mode: TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string(),
        message_source: input.message_source,
        prefixed_input_items,
        shared_model_config,
        shared_max_iterations: max_iterations_from_settings(effective_settings),
        shared_runtime_callbacks,
        shared_runtime_lifecycle,
        task_turn,
    }
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
        .unwrap_or(600)
}

#[cfg(test)]
#[path = "chat_execution/tests.rs"]
mod tests;
