// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::*;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::empty_mcp_server_bundle;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotPluginCommandInvocationDto;
use crate::services::mcp_loader::McpHttpServer;

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

fn dependency_write_tool_call(task_id: &str, dependency_id: &str) -> Value {
    json!({
        "id": format!("call-{task_id}"),
        "type": "function",
        "function": {
            "name": "project_management_service_set_project_task_dependencies",
            "arguments": serde_json::to_string(&json!({
                "project_task_id": task_id,
                "depends_on_project_task_ids": [dependency_id],
            }))
            .expect("dependency arguments"),
        }
    })
}

fn successful_dependency_write_result() -> Value {
    json!({
        "name": "project_management_service_set_project_task_dependencies",
        "success": true,
        "is_error": false,
    })
}

fn successful_dependency_graph_result() -> Value {
    json!({
        "name": "project_management_service_get_project_dependency_graph",
        "success": true,
        "is_error": false,
    })
}

fn model_runtime(use_codex_gateway_mcp_passthrough: bool) -> ResolvedChatModelConfig {
    ResolvedChatModelConfig {
        model_config_id: Some("model-config-1".to_string()),
        model: "codex-test".to_string(),
        provider: "openai".to_string(),
        prompt_vendor: Some("gpt".to_string()),
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
        model_request_max_retries: 5,
    }
}

fn runtime_context(
    project_requirement_execution_planner: bool,
) -> ResolvedConversationRuntimeContext {
    ResolvedConversationRuntimeContext {
        agent_profile: ChatosAgentProfile::from_flags(false, project_requirement_execution_planner),
        internal_context_locale: InternalContextLocale::ZhCn,
        user_output_locale: InternalContextLocale::ZhCn,
        contact_agent_id: None,
        base_system_prompt: None,
        agent_system_prompt: Some("agent prompt".to_string()),
        contact_system_prompt: None,
        builtin_mcp_system_prompt: None,
        selected_commands_for_snapshot: Arc::new(Mutex::new(Vec::new())),
        plugin_command_invocations_for_snapshot: Vec::new(),
        resolved_project_id: Some("project-1".to_string()),
        resolved_project_name: Some("Demo Project".to_string()),
        resolved_project_root: Some("C:/project/demo".to_string()),
        default_remote_connection_id: None,
        workspace_root: Some("C:/project/demo".to_string()),
        mcp_enabled: true,
        enabled_mcp_ids_for_snapshot: Vec::new(),
        mcp_server_bundle: empty_mcp_server_bundle(),
        mcp_management_runtime_session: None,
        mcp_command_queue: None,
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
fn per_request_mcp_auth_disables_codex_gateway_passthrough() {
    let model = model_runtime(true);
    let mut context = runtime_context(false);
    context.mcp_server_bundle.0.push(McpHttpServer {
        name: "project".to_string(),
        url: "http://127.0.0.1:39210/mcp".to_string(),
        headers: Some(HashMap::from([(
            "x-project-service-internal-scope".to_string(),
            "project.mcp".to_string(),
        )])),
        timeout_ms: None,
        tool_timeout_ms: HashMap::new(),
        allowed_tool_names: None,
        preserve_tool_names: false,
        fail_on_unavailable: false,
        async_result_transport: chatos_mcp_runtime::McpAsyncResultTransport::Disabled,
        header_provider: None,
    });

    assert!(!effective_codex_gateway_mcp_passthrough(&model, &context));
}

#[test]
fn mcp_management_gateway_is_always_executed_by_chatos_runtime() {
    let model = model_runtime(true);
    let mut context = runtime_context(false);
    context.mcp_server_bundle.0.push(McpHttpServer {
        name: "mcp_management".to_string(),
        url: "http://127.0.0.1:39280/mcp".to_string(),
        headers: Some(HashMap::from([(
            "authorization".to_string(),
            "Bearer runtime-token".to_string(),
        )])),
        timeout_ms: Some(180_000),
        tool_timeout_ms: HashMap::new(),
        allowed_tool_names: None,
        preserve_tool_names: true,
        fail_on_unavailable: true,
        async_result_transport: chatos_mcp_runtime::McpAsyncResultTransport::RabbitMq,
        header_provider: None,
    });

    assert!(!effective_codex_gateway_mcp_passthrough(&model, &context));
}

#[test]
fn initializes_stream_agent_with_resolved_profile() {
    let profile = ChatosAgentProfile::from_flags(true, false);
    let agent = init_chatos_stream_agent(&model_runtime(false), profile);

    assert_eq!(agent.profile(), profile);
}

#[test]
fn project_context_prompt_contains_only_dynamic_project_facts() {
    let mut context = runtime_context(false);
    context.resolved_project_name = Some("CubeSandbox".to_string());
    context.resolved_project_root = None;
    context.workspace_root = None;

    let prompt = build_workspace_global_prompt(&context).expect("project context prompt");

    assert!(prompt.contains("当前项目名称：CubeSandbox"));
    assert!(prompt.contains("所有项目工具路由均已由程序绑定"));
    assert!(!prompt.contains("project-1"));
    assert!(!prompt.contains("cloud"));
    assert!(!prompt.contains("C:/project"));
    assert!(!prompt.contains("Task Runner 是你自己的内部异步执行通道"));
}

#[test]
fn agent_instructions_apply_user_language_to_project_artifacts() {
    let mut chinese_context = runtime_context(false);
    chinese_context.internal_context_locale = InternalContextLocale::EnUs;
    chinese_context.user_output_locale = InternalContextLocale::ZhCn;
    let chinese = compose_agent_instructions(&chinese_context, &model_runtime(false))
        .expect("Chinese instructions");

    assert!(chinese.contains("User Language Policy"));
    assert!(chinese.contains("latest substantive, user-authored request"));
    assert!(chinese.contains("简体中文（zh-CN）"));
    assert!(chinese.contains("requirement titles"));
    assert!(chinese.contains("implementation-task titles and descriptions"));
    assert!(chinese.contains("execution-task titles and objectives"));
    assert!(chinese.contains("User-Facing Final Reply Policy"));
    assert!(chinese.contains("concise product delivery note"));
    assert!(chinese.contains("names and concepts a customer sees"));

    let mut english_context = runtime_context(false);
    english_context.internal_context_locale = InternalContextLocale::ZhCn;
    english_context.user_output_locale = InternalContextLocale::EnUs;
    let english = compose_agent_instructions(&english_context, &model_runtime(false))
        .expect("English instructions");

    assert!(english.contains("English (en-US)"));
    assert!(!english.contains("简体中文（zh-CN）"));
}

#[test]
fn builds_shared_runtime_execution_contract_from_chat_context() {
    let mut context = runtime_context(false);
    context.plugin_command_invocations_for_snapshot =
        vec![TurnRuntimeSnapshotPluginCommandInvocationDto {
            plugin_id: "plugin-a".to_string(),
            command_id: "review".to_string(),
            arguments_present: true,
            arguments_sha256: Some("a".repeat(64)),
        }];
    let options = build_agent_chat_options(
        "session-1",
        &model_runtime(true),
        &context,
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
            persisted_user_message_content: Some("执行需求的 2 个关联任务。".to_string()),
            persisted_user_message_metadata: Some(json!({
                "project_requirement_execution": {
                    "requirement_title": "JDK 21 upgrade"
                }
            })),
        },
    );

    assert!(options.use_tools);
    assert_eq!(options.turn_id, "turn-1");
    assert_eq!(options.prefixed_input_items.len(), 1);
    assert_eq!(options.shared_max_iterations, 42);
    assert!(!options.project_requirement_execution_planner);
    assert_eq!(options.shared_model_config.max_output_tokens, Some(2048));
    assert!(options.shared_model_config.request_cwd.is_none());
    assert!(options.shared_model_config.include_prompt_cache_retention);
    assert_eq!(
        options.persisted_user_message_content.as_deref(),
        Some("执行需求的 2 个关联任务。")
    );
    assert_eq!(
        options
            .persisted_user_message_metadata
            .as_ref()
            .and_then(|value| value["project_requirement_execution"]["requirement_title"].as_str()),
        Some("JDK 21 upgrade")
    );
    assert_eq!(
        options
            .persisted_user_message_metadata
            .as_ref()
            .and_then(|value| { value["plugin_command_invocations"][0]["plugin_id"].as_str() }),
        Some("plugin-a")
    );
    assert_eq!(
        options
            .persisted_user_message_metadata
            .as_ref()
            .and_then(|value| {
                value["plugin_command_invocations"][0]["arguments_sha256"].as_str()
            }),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(options
        .persisted_user_message_metadata
        .as_ref()
        .and_then(|value| value["plugin_command_invocations"][0].as_object())
        .is_some_and(|value| !value.contains_key("arguments")));
}

#[test]
fn shared_runtime_record_contract_preserves_chatos_message_metadata() {
    let record_options =
        build_chatos_record_options(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE, "model-source", false);
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
    assert_eq!(user_record.content, "hello");
}

#[test]
fn hidden_planning_turn_hides_assistant_and_tool_records_until_confirmation() {
    let record_options =
        build_chatos_record_options(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE, "model-source", true);

    assert_eq!(
        record_options
            .assistant_metadata
            .as_ref()
            .and_then(|value| value.get("hidden"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        record_options
            .tool_metadata
            .as_ref()
            .and_then(|value| value.get("hidden"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn merges_persisted_execution_metadata_with_generated_turn_metadata() {
    let metadata = merge_user_record_metadata(
        Some(json!({
            "project_requirement_execution": {
                "requirement_title": "JDK 21 upgrade"
            }
        })),
        Some(json!({"conversation_turn_id": "turn-1"})),
    )
    .expect("merged metadata");

    assert_eq!(
        metadata["project_requirement_execution"]["requirement_title"].as_str(),
        Some("JDK 21 upgrade")
    );
    assert_eq!(metadata["conversation_turn_id"].as_str(), Some("turn-1"));
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

#[test]
fn planner_completion_tracker_requires_the_exact_successful_materializer_tool() {
    assert!(project_execution_planner_terminal_tool_succeeded(&json!({
        "tool_results": [{
            "name": "task_runner_service_create_project_execution_tasks",
            "success": true,
            "is_error": false
        }]
    })));
    assert!(!project_execution_planner_terminal_tool_succeeded(&json!({
        "tool_results": [{
            "name": "task_runner_service_create_project_execution_tasks",
            "success": false,
            "is_error": true
        }]
    })));
    assert!(!project_execution_planner_terminal_tool_succeeded(&json!({
        "tool_results": [{
            "name": "project_management_create_project_task",
            "success": true,
            "is_error": false
        }]
    })));
}

#[test]
fn planner_completion_tracker_preserves_existing_tools_end_callback() {
    let state = Arc::new(Mutex::new(TaskTurnLifecycleState::default()));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let callbacks = track_project_execution_planner_completion(
        RuntimeCallbacks {
            on_tools_end: Some(Arc::new({
                let observed = Arc::clone(&observed);
                move |payload| observed.lock().expect("observed").push(payload)
            })),
            ..RuntimeCallbacks::default()
        },
        Arc::clone(&state),
    );
    let payload = json!({
        "tool_results": [{
            "name": "task_runner_service_create_project_execution_tasks",
            "success": true,
            "is_error": false
        }]
    });

    callbacks.on_tools_end.expect("tools end callback")(payload.clone());

    assert!(
        state
            .lock()
            .expect("state")
            .project_execution_plan_materialized
    );
    assert_eq!(observed.lock().expect("observed").as_slice(), &[payload]);
}

#[test]
fn planning_integrity_tracker_requires_repair_then_later_graph_verification() {
    let state = Arc::new(Mutex::new(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        ..TaskTurnLifecycleState::default()
    }));
    let callbacks =
        track_project_planning_integrity(RuntimeCallbacks::default(), Arc::clone(&state));
    let callback = callbacks.on_tools_end.expect("tools end callback");

    callback(json!({
        "tool_results": [{
            "name": "project_management_service_set_project_task_dependencies",
            "success": false,
            "is_error": true,
            "content": "项目任务不存在: mistyped-id"
        }]
    }));
    assert_eq!(
        state
            .lock()
            .expect("state")
            .project_planning_write_failures
            .len(),
        1
    );

    callback(json!({
        "tool_results": [
            {
                "name": "project_management_service_set_project_task_dependencies",
                "success": true,
                "is_error": false
            },
            {
                "name": "project_management_service_get_project_dependency_graph",
                "success": true,
                "is_error": false
            }
        ]
    }));
    {
        let state = state.lock().expect("state");
        assert_eq!(state.project_planning_write_failures.len(), 1);
        assert!(state.project_planning_repair_mutation_succeeded);
    }

    callback(json!({
        "tool_results": [{
            "name": "project_management_service_get_project_dependency_graph",
            "success": true,
            "is_error": false
        }]
    }));
    let state = state.lock().expect("state");
    assert!(state.project_planning_write_failures.is_empty());
    assert!(!state.project_planning_repair_mutation_succeeded);
}

#[test]
fn planning_integrity_tracker_finalizes_a_repeated_successful_dependency_batch_after_verification()
{
    let state = Arc::new(Mutex::new(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        ..TaskTurnLifecycleState::default()
    }));
    let callbacks =
        track_project_planning_integrity(RuntimeCallbacks::default(), Arc::clone(&state));
    let on_tools_start = callbacks
        .on_tools_start
        .as_ref()
        .expect("tools start callback")
        .clone();
    let on_tools_end = callbacks
        .on_tools_end
        .as_ref()
        .expect("tools end callback")
        .clone();
    let batch = json!([dependency_write_tool_call("task-2", "task-1")]);

    for _ in 0..2 {
        on_tools_start(batch.clone());
        on_tools_end(json!({
            "tool_results": [successful_dependency_write_result()]
        }));
    }
    on_tools_start(json!([{
        "function": {
            "name": "project_management_service_get_project_dependency_graph",
            "arguments": "{}"
        }
    }]));
    on_tools_end(json!({
        "tool_results": [successful_dependency_graph_result()]
    }));

    let state = state.lock().expect("state");
    assert!(state.project_planning_force_finalization);
    assert!(state.project_planning_dependency_write_cycle.is_empty());
    assert_eq!(
        state.project_planning_last_verified_dependency_cycle.len(),
        1
    );
}

#[test]
fn planning_integrity_tracker_finalizes_when_two_verified_cycles_repeat() {
    let state = Arc::new(Mutex::new(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        ..TaskTurnLifecycleState::default()
    }));
    let callbacks =
        track_project_planning_integrity(RuntimeCallbacks::default(), Arc::clone(&state));
    let on_tools_start = callbacks
        .on_tools_start
        .as_ref()
        .expect("tools start callback")
        .clone();
    let on_tools_end = callbacks
        .on_tools_end
        .as_ref()
        .expect("tools end callback")
        .clone();
    let batch = json!([dependency_write_tool_call("task-2", "task-1")]);
    let graph_read = json!([{
        "function": {
            "name": "project_management_service_get_project_dependency_graph",
            "arguments": "{}"
        }
    }]);

    for cycle in 0..2 {
        on_tools_start(batch.clone());
        on_tools_end(json!({
            "tool_results": [successful_dependency_write_result()]
        }));
        on_tools_start(graph_read.clone());
        on_tools_end(json!({
            "tool_results": [successful_dependency_graph_result()]
        }));
        assert_eq!(
            state
                .lock()
                .expect("state")
                .project_planning_force_finalization,
            cycle == 1
        );
    }
}

#[test]
fn planning_integrity_tracker_does_not_finalize_distinct_verified_writes() {
    let state = Arc::new(Mutex::new(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        ..TaskTurnLifecycleState::default()
    }));
    let callbacks =
        track_project_planning_integrity(RuntimeCallbacks::default(), Arc::clone(&state));
    let on_tools_start = callbacks
        .on_tools_start
        .as_ref()
        .expect("tools start callback")
        .clone();
    let on_tools_end = callbacks
        .on_tools_end
        .as_ref()
        .expect("tools end callback")
        .clone();
    let graph_read = json!([{
        "function": {
            "name": "project_management_service_get_project_dependency_graph",
            "arguments": "{}"
        }
    }]);

    for batch in [
        dependency_write_tool_call("task-2", "task-1"),
        dependency_write_tool_call("task-3", "task-2"),
    ] {
        on_tools_start(json!([batch]));
        on_tools_end(json!({
            "tool_results": [successful_dependency_write_result()]
        }));
        on_tools_start(graph_read.clone());
        on_tools_end(json!({
            "tool_results": [successful_dependency_graph_result()]
        }));
    }

    assert!(
        !state
            .lock()
            .expect("state")
            .project_planning_force_finalization
    );
}

#[tokio::test]
async fn planning_integrity_guard_rejects_a_false_success_summary() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        project_planning_write_failures: vec![
            "project_management_service_set_project_task_dependencies: bad id".to_string(),
        ],
        ..TaskTurnLifecycleState::default()
    });

    let action = hook
        .after_final_response(final_response_context(ai_response("规划已经全部完成。")))
        .await
        .expect("integrity repair continuation");

    match action {
        RuntimeFinalResponseAction::Continue {
            input_items,
            reason,
        } => {
            assert_eq!(reason, "project_planning_integrity_repair");
            assert!(input_items.iter().any(|item| {
                item.get("role").and_then(Value::as_str) == Some("system")
                    && item.to_string().contains("不要总结完成")
            }));
        }
        _ => panic!("expected planning repair continuation"),
    }
}

#[tokio::test]
async fn repeated_planning_writes_enter_a_tool_free_finalization_iteration() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        project_planning_force_finalization: true,
        mode: Some(TaskTurnFollowUpMode::ContinueExecution),
        ..TaskTurnLifecycleState::default()
    });

    let directive = hook
        .before_model_request(RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration: 5,
            reason: "tool_results".to_string(),
            input: json!([]),
        })
        .await
        .expect("planning finalization directive");

    assert!(!directive.tools_enabled);
    assert!(directive.stream_output);
    assert!(directive.input_items.iter().any(|item| {
        item.get("role").and_then(Value::as_str) == Some("system")
            && item
                .to_string()
                .contains("identical project-task dependency")
            && item.to_string().contains("Do not call any more tools")
    }));
}

#[tokio::test]
async fn repeated_planning_writes_accept_the_first_final_response() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        project_planning_force_finalization: true,
        mode: Some(TaskTurnFollowUpMode::ContinueExecution),
        ..TaskTurnLifecycleState::default()
    });

    let action = hook
        .after_final_response(final_response_context(ai_response(
            "已经按照最新依赖图完成规划复核。",
        )))
        .await
        .expect("planning final response");

    assert!(matches!(action, RuntimeFinalResponseAction::Accept));
    assert!(hook.task_turn_state().expect("state").mode.is_none());
}

#[tokio::test]
async fn planning_write_failure_takes_priority_over_loop_finalization() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_planning_integrity_guard: true,
        project_planning_force_finalization: true,
        project_planning_write_failures: vec![
            "project_management_service_set_project_task_dependencies: bad id".to_string(),
        ],
        ..TaskTurnLifecycleState::default()
    });

    let directive = hook
        .before_model_request(RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration: 5,
            reason: "tool_results".to_string(),
            input: json!([]),
        })
        .await
        .expect("planning repair directive");
    assert!(directive.tools_enabled);
    assert!(!directive
        .input_items
        .iter()
        .any(|item| item.to_string().contains("Project Planning Finalization")));

    let action = hook
        .after_final_response(final_response_context(ai_response("规划已经全部完成。")))
        .await
        .expect("integrity repair continuation");
    assert!(matches!(
        action,
        RuntimeFinalResponseAction::Continue { reason, .. }
            if reason == "project_planning_integrity_repair"
    ));
}

#[tokio::test]
async fn materialized_execution_plan_enters_a_tool_free_finalization_iteration() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_execution_plan_materialized: true,
        mode: Some(TaskTurnFollowUpMode::ContinueExecution),
        ..TaskTurnLifecycleState::default()
    });

    let directive = hook
        .before_model_request(RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration: 3,
            reason: "tool_results".to_string(),
            input: json!([]),
        })
        .await
        .expect("planner finalization directive");

    assert!(!directive.tools_enabled);
    assert!(directive.stream_output);
    assert!(directive.input_items.iter().any(|item| {
        item.get("role").and_then(Value::as_str) == Some("system")
            && item
                .to_string()
                .contains("execution task graph was persisted")
            && item.to_string().contains("Do not call any more tools")
    }));
}

#[tokio::test]
async fn materialized_execution_plan_accepts_the_first_final_response() {
    let hook = lifecycle_hook_with_state(TaskTurnLifecycleState {
        project_execution_plan_materialized: true,
        mode: Some(TaskTurnFollowUpMode::ContinueExecution),
        ..TaskTurnLifecycleState::default()
    });

    let action = hook
        .after_final_response(final_response_context(ai_response(
            "执行计划已经生成，可以预览并确认执行。",
        )))
        .await
        .expect("planner final response");

    assert!(matches!(action, RuntimeFinalResponseAction::Accept));
    assert!(hook.task_turn_state().expect("state").mode.is_none());
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
