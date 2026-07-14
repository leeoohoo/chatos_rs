// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use super::*;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::empty_mcp_server_bundle;

fn lifecycle_hook_with_state(state: TaskTurnLifecycleState) -> ChatosRuntimeLifecycleHook {
    ChatosRuntimeLifecycleHook {
        session_id: format!("missing-session-{}", uuid::Uuid::new_v4()),
        turn_id: "turn-1".to_string(),
        model_name: "model".to_string(),
        supports_images: Some(false),
        callbacks: AiClientCallbacks::default(),
        max_task_follow_up_rounds: 3,
        task_turn: Arc::new(Mutex::new(state)),
    }
}

fn ai_response(content: &str) -> AiResponse {
    AiResponse {
        content: content.to_string(),
        reasoning: Some("reasoning".to_string()),
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        provider_error: None,
        usage: None,
        response_id: Some("response-1".to_string()),
    }
}

fn final_response_context(response: AiResponse) -> RuntimeFinalResponseContext {
    RuntimeFinalResponseContext {
        conversation_id: Some("session-1".to_string()),
        conversation_turn_id: Some("turn-1".to_string()),
        iteration: 2,
        reason: "task_review".to_string(),
        response,
    }
}

fn model_runtime(use_codex_gateway_mcp_passthrough: bool) -> ResolvedChatModelConfig {
    ResolvedChatModelConfig {
        model: "codex-test".to_string(),
        provider: "openai".to_string(),
        thinking_level: None,
        temperature: 0.2,
        supports_images: false,
        supports_responses: true,
        effective_reasoning: false,
        api_key: String::new(),
        base_url: "http://codex-gateway.local".to_string(),
        system_prompt: None,
        use_active_system_context: true,
        use_codex_gateway_mcp_passthrough,
    }
}

fn runtime_context(
    project_requirement_execution_planner: bool,
) -> ResolvedConversationRuntimeContext {
    ResolvedConversationRuntimeContext {
        agent_profile: ChatosAgentProfile::from_flags(false, project_requirement_execution_planner),
        internal_context_locale: InternalContextLocale::ZhCn,
        contact_agent_id: None,
        base_system_prompt: None,
        contact_system_prompt: None,
        builtin_mcp_system_prompt: None,
        selected_commands_for_snapshot: Arc::new(Mutex::new(Vec::new())),
        resolved_project_id: Some("project-1".to_string()),
        resolved_project_name: Some("Demo Project".to_string()),
        resolved_project_source_type: Some("local".to_string()),
        resolved_project_root: Some("C:/project/demo".to_string()),
        default_remote_connection_id: None,
        workspace_root: Some("C:/project/demo".to_string()),
        mcp_enabled: true,
        enabled_mcp_ids_for_snapshot: Vec::new(),
        mcp_server_bundle: empty_mcp_server_bundle(),
        use_tools: true,
        memory_summary_prompt: None,
        runtime_error: None,
        project_requirement_execution_planner,
    }
}

#[test]
fn requirement_execution_planner_disables_codex_gateway_mcp_passthrough() {
    let model = model_runtime(true);

    assert!(effective_codex_gateway_mcp_passthrough(
        &model,
        &runtime_context(false)
    ));
    assert!(!effective_codex_gateway_mcp_passthrough(
        &model,
        &runtime_context(true)
    ));
}

#[test]
fn initializes_stream_agent_with_resolved_profile() {
    let profile = ChatosAgentProfile::from_flags(true, false);
    let agent = init_chatos_stream_agent(&model_runtime(false), profile);

    assert_eq!(agent.profile(), profile);
}

#[test]
fn project_context_prompt_names_the_project_and_requires_task_runner_follow_through() {
    let mut context = runtime_context(false);
    context.resolved_project_name = Some("CubeSandbox".to_string());
    context.resolved_project_source_type = Some("cloud".to_string());
    context.resolved_project_root = None;
    context.workspace_root = None;

    let prompt = build_workspace_global_prompt(&context).expect("project context prompt");

    assert!(prompt.contains("当前项目名称：CubeSandbox"));
    assert!(prompt.contains("当前项目 ID：project-1"));
    assert!(prompt.contains("当前项目来源类型：cloud"));
    assert!(prompt.contains("Task Runner 是你自己的内部异步执行通道"));
    assert!(prompt.contains("不得仅因为主对话不能直接读取文件就声称无法查看"));
    assert!(prompt.contains("不要要求用户再次粘贴代码或提供项目路径"));
}

#[test]
fn builds_shared_runtime_execution_contract_from_chat_context() {
    let options = build_agent_chat_options(
        "session-1",
        &model_runtime(true),
        &runtime_context(false),
        &json!({
            "MAX_ITERATIONS": 42,
            "TASK_FOLLOW_UP_MAX_ROUNDS": 4,
            "AI_REQUEST_BODY_LIMIT_BYTES": 123456
        }),
        vec![json!({"role": "system", "content": "prefix"})],
        ChatExecutionInput {
            use_tools: true,
            max_tokens: Some(2048),
            attachments: Vec::new(),
            callbacks: AiClientCallbacks::default(),
            turn_id: "turn-1".to_string(),
            user_message_id: "user-1".to_string(),
            message_source: "model-source".to_string(),
        },
    );

    assert!(options.use_tools);
    assert_eq!(options.turn_id, "turn-1");
    assert_eq!(options.prefixed_input_items.len(), 1);
    assert_eq!(options.shared_max_iterations, 42);
    assert_eq!(options.shared_model_config.max_output_tokens, Some(2048));
    assert_eq!(
        options.shared_model_config.request_cwd.as_deref(),
        Some("C:/project/demo")
    );
    assert!(options.shared_model_config.include_prompt_cache_retention);
}

#[test]
fn shared_runtime_record_contract_preserves_chatos_message_metadata() {
    let record_options =
        build_chatos_record_options(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE, "model-source");
    let user_record = build_chatos_user_record(
        "session-1",
        Some("turn-1".to_string()),
        "user-1".to_string(),
        "hello",
        Some(json!({"attachments": []})),
        TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
        "model-source",
    );

    assert!(record_options.persist_assistant_records);
    assert!(record_options.persist_tool_records);
    assert_eq!(
        record_options
            .assistant_metadata
            .as_ref()
            .and_then(|value| { value["task_runner_async"]["message_kind"].as_str() }),
        Some("plan_summary")
    );
    assert_eq!(
        record_options
            .tool_metadata
            .as_ref()
            .and_then(|value| { value["task_runner_async"]["message_kind"].as_str() }),
        Some("tool_call")
    );
    assert_eq!(user_record.conversation_id, "session-1");
    assert_eq!(user_record.conversation_turn_id.as_deref(), Some("turn-1"));
    assert_eq!(user_record.message_id.as_deref(), Some("user-1"));
    assert_eq!(
        user_record.message_mode.as_deref(),
        Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE)
    );
    assert_eq!(user_record.message_source.as_deref(), Some("model-source"));
}

#[test]
fn bridges_chatos_request_observers_to_shared_runtime_callbacks() {
    let observed_input = Arc::new(Mutex::new(None));
    let observed_payload = Arc::new(Mutex::new(None));
    let observed_summary = Arc::new(Mutex::new(None));
    let callbacks = AiClientCallbacks {
        on_before_model_request: Some(Arc::new({
            let observed_input = Arc::clone(&observed_input);
            move |input, _, _| {
                *observed_input.lock().expect("input") = Some(input.clone());
            }
        })),
        on_before_send_model_request: Some(Arc::new({
            let observed_payload = Arc::clone(&observed_payload);
            move |payload| {
                *observed_payload.lock().expect("payload") = Some(payload);
            }
        })),
        on_context_summarized_end: Some(Arc::new({
            let observed_summary = Arc::clone(&observed_summary);
            move |payload| {
                *observed_summary.lock().expect("summary") = Some(payload);
            }
        })),
        ..AiClientCallbacks::default()
    };
    let runtime_callbacks = shared_runtime_callbacks_from_chatos(&callbacks);
    let input = json!([{"role": "user", "content": "hello"}]);
    let payload = json!({"model": "test", "input": input.clone()});

    runtime_callbacks
        .on_before_model_input
        .expect("input callback")(input.clone());
    runtime_callbacks
        .on_before_send_model_request
        .expect("payload callback")(payload.clone());
    let summary = json!({"phase": "end", "compacted": true});
    runtime_callbacks
        .on_context_summarized_end
        .expect("summary callback")(summary.clone());

    assert_eq!(*observed_input.lock().expect("input"), Some(input));
    assert_eq!(*observed_payload.lock().expect("payload"), Some(payload));
    assert_eq!(*observed_summary.lock().expect("summary"), Some(summary));
}

#[tokio::test]
async fn runtime_lifecycle_hook_keeps_empty_guidance_non_intrusive() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState::default());

    let directive = hook
        .before_model_request(RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration: 1,
            reason: "initial".to_string(),
            input: json!([]),
        })
        .await
        .expect("guidance hook");

    assert!(directive.input_items.is_empty());
    assert!(directive.stream_output);
    assert!(directive.tools_enabled);
}

#[tokio::test]
async fn review_iteration_disables_streaming_and_tools() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        mode: Some(TaskTurnFollowUpMode::ReviewExecution),
        ..TaskTurnLifecycleState::default()
    });

    let directive = hook
        .before_model_request(RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration: 2,
            reason: "task_review".to_string(),
            input: json!([]),
        })
        .await
        .expect("review directive");

    assert!(!directive.stream_output);
    assert!(!directive.tools_enabled);
}

#[tokio::test]
async fn passing_review_restores_last_visible_response() {
    let visible = ai_response("visible completion");
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        follow_up_rounds: 1,
        mode: Some(TaskTurnFollowUpMode::ReviewExecution),
        last_visible_response: Some(visible.clone()),
        review_locale: Some(InternalContextLocale::EnUs),
        ..TaskTurnLifecycleState::default()
    });

    let action = hook
        .after_final_response(final_response_context(ai_response(
            "TASK_REVIEW: pass\nall checks passed",
        )))
        .await
        .expect("review action");

    match action {
        RuntimeFinalResponseAction::Replace(response) => {
            assert_eq!(response.content, visible.content);
            assert_eq!(response.reasoning, visible.reasoning);
        }
        _ => panic!("expected replacement response"),
    }
    let state = hook.task_turn_state().expect("state");
    assert!(state.review_attempted);
    assert_eq!(state.review_last_outcome, Some(TaskTurnReviewOutcome::Pass));
    assert!(state.mode.is_none());
}

#[tokio::test]
async fn final_response_metadata_reports_review_state() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        follow_up_rounds: 2,
        review_attempted: true,
        review_last_outcome: Some(TaskTurnReviewOutcome::NeedsMoreWork),
        ..TaskTurnLifecycleState::default()
    });

    let metadata = hook
        .final_response_metadata(final_response_context(ai_response("done")))
        .await
        .expect("metadata")
        .expect("review metadata");

    assert_eq!(metadata["task_turn_review"]["attempted"], true);
    assert_eq!(metadata["task_turn_review"]["outcome"], "needs_more_work");
    assert_eq!(metadata["task_turn_review"]["rounds"], 2);
}

#[tokio::test]
async fn failed_review_continues_with_hidden_review_context() {
    let visible = ai_response("visible completion");
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        follow_up_rounds: 1,
        mode: Some(TaskTurnFollowUpMode::ReviewExecution),
        last_visible_response: Some(visible.clone()),
        review_locale: Some(InternalContextLocale::EnUs),
        continuation_history: vec![
            assistant_response_input_item(&visible).expect("visible input item")
        ],
        ..TaskTurnLifecycleState::default()
    });

    let action = hook
        .after_final_response(final_response_context(ai_response(
            "TASK_REVIEW: needs_more_work\nmissing verification",
        )))
        .await
        .expect("review retry action");

    let input_items = match action {
        RuntimeFinalResponseAction::Continue {
            input_items,
            reason,
        } => {
            assert_eq!(reason, "task_review_retry");
            input_items
        }
        _ => panic!("expected continuation"),
    };
    assert!(input_items.iter().any(|item| {
        item.get("role").and_then(Value::as_str) == Some("assistant")
            && item.to_string().contains("needs_more_work")
    }));
    assert!(input_items.iter().any(|item| {
        item.get("role").and_then(Value::as_str) == Some("system")
            && item.to_string().contains("review found remaining issues")
    }));
    let state = hook.task_turn_state().expect("state");
    assert_eq!(state.follow_up_rounds, 2);
    assert_eq!(state.mode, Some(TaskTurnFollowUpMode::ContinueExecution));
    assert_eq!(
        state.review_last_outcome,
        Some(TaskTurnReviewOutcome::NeedsMoreWork)
    );
}

#[test]
fn task_follow_up_round_limit_uses_effective_settings() {
    assert_eq!(task_follow_up_max_rounds_from_settings(&json!({})), 3);
    assert_eq!(
        task_follow_up_max_rounds_from_settings(&json!({"TASK_FOLLOW_UP_MAX_ROUNDS": 5})),
        5
    );
    assert_eq!(
        task_follow_up_max_rounds_from_settings(&json!({"TASK_FOLLOW_UP_MAX_ROUNDS": -1})),
        0
    );
    assert_eq!(max_iterations_from_settings(&json!({})), 600);
    assert_eq!(
        max_iterations_from_settings(&json!({"MAX_ITERATIONS": 12})),
        12
    );
    assert_eq!(
        max_iterations_from_settings(&json!({"MAX_ITERATIONS": 0})),
        1
    );
}
