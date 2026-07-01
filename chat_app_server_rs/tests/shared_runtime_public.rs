// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use chat_app_server_rs::shared_runtime::{
    build_ai_runtime, build_ai_runtime_from_shared_mcp_executor,
    build_initialized_shared_builtin_mcp_executor, build_mcp_tool_execute,
    build_mcp_tool_execute_from_builtin_kinds, build_model_request,
    build_model_request_from_config, build_model_runtime_config_from_resolved,
    compose_effective_shared_builtin_mcp_system_prompt, compose_shared_builtin_mcp_system_prompt,
    default_runtime_builtin_kinds, inspect_effective_shared_builtin_mcp_system_prompt,
    inspect_shared_builtin_mcp_system_prompt, AiRuntimeBuilder, AiRuntimeOptions, AiTurnReport,
    AiTurnStatus, BuiltinMcpPromptLocale, BuiltinMcpServerOptions, ModelRuntimeConfig,
    ResolvedChatModelConfig, RuntimeRecordOptions, RuntimeTurnSpec, SharedBuiltinMcpKind,
    SharedMcpExecutorBuilder, TaskBuiltinMcpPromptMode, TaskMcpInitMode, TaskMemoryRuntimeConfig,
    TaskRunExecution, TaskRunSpec, TaskRuntimeBuilder, TaskRuntimeConfig,
};

#[test]
fn shared_runtime_public_facade_builds_core_values() {
    let executor = build_mcp_tool_execute(Vec::new(), Vec::new(), Vec::new());
    let runtime = build_ai_runtime(Some(executor));
    let _runtime = runtime.with_max_iterations(3);

    let shared_executor = chatos_mcp_runtime::McpExecutor::new(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        chatos_mcp_runtime::BuiltinToolRegistry::new(),
    );
    let _runtime = build_ai_runtime_from_shared_mcp_executor(shared_executor);

    let request = build_model_request(
        "http://127.0.0.1:8080/v1".to_string(),
        "test-key".to_string(),
        "test-model".to_string(),
        "gpt".to_string(),
        json!("hello"),
        Vec::new(),
        false,
        None,
        None,
        None,
        None,
    );
    assert_eq!(request.model, "test-model");

    let options =
        AiRuntimeOptions::default().with_record_options(RuntimeRecordOptions::persist_all());
    assert!(options.record_options.persist_assistant_records);
    assert!(options.record_options.persist_tool_records);

    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "test-key",
        "test-model",
        "gpt",
    );
    let request = build_model_request_from_config(&config, json!("hi"), Vec::new());
    assert_eq!(request.model, "test-model");

    let spec = RuntimeTurnSpec::for_user_text(config, "task_1", "hello");
    let request = spec.into_contextual_turn_request();
    assert_eq!(
        request.runtime_options.conversation_id.as_deref(),
        Some("task_1")
    );

    let task_config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "test-key",
        "task-model",
        "gpt",
    );
    let prompt_executor = SharedMcpExecutorBuilder::new()
        .with_builtin_kinds(
            vec![SharedBuiltinMcpKind::TaskManager],
            &BuiltinMcpServerOptions::new("/tmp/chatos-shared-runtime-test"),
        )
        .build();
    let task_spec = TaskRunSpec::new("task_1", "run_1", task_config, "run task")
        .with_model_config_id("cfg_1")
        .with_configured_builtin_mcp_prompt_from_executor(
            &prompt_executor,
            BuiltinMcpPromptLocale::ZhCn,
        );
    assert_eq!(
        task_spec
            .builtin_mcp_prompt
            .as_ref()
            .map(|snapshot| &snapshot.mode),
        Some(&TaskBuiltinMcpPromptMode::Configured)
    );
    let task_request = task_spec.into_contextual_turn_request();
    assert_eq!(task_request.model_request.model, "task-model");
    assert_eq!(
        task_request.runtime_options.conversation_turn_id.as_deref(),
        Some("run_1")
    );

    let task_runtime = TaskRuntimeBuilder::new()
        .with_mcp_executor(prompt_executor)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
        .build();
    let prepared = task_runtime.prepare_spec(TaskRunSpec::new(
        "task_2",
        "run_2",
        ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "test-key",
            "task-model",
            "gpt",
        ),
        "run another task",
    ));
    assert!(prepared.builtin_mcp_prompt.is_some());
}

#[test]
fn shared_runtime_public_facade_builds_chatos_builtin_executor() {
    let options = BuiltinMcpServerOptions::new("/tmp/chatos-shared-runtime-test")
        .with_user_id("user-1")
        .with_project_id("project-1");
    let executor =
        build_mcp_tool_execute_from_builtin_kinds(vec![SharedBuiltinMcpKind::TaskManager], options)
            .expect("build task manager executor");
    assert!(executor.get_available_tools().is_empty());

    let runtime_kinds = default_runtime_builtin_kinds();
    assert!(runtime_kinds.contains(&SharedBuiltinMcpKind::TaskManager));

    let servers = chatos_mcp_runtime::builtin_servers_from_kinds(
        vec![SharedBuiltinMcpKind::TaskManager],
        &BuiltinMcpServerOptions::new("/tmp/chatos-shared-runtime-test"),
    );
    let prompt =
        compose_shared_builtin_mcp_system_prompt(servers.as_slice(), BuiltinMcpPromptLocale::ZhCn)
            .expect("builtin prompt");
    assert!(prompt.contains("`task_manager_add_task`"));

    let debug =
        inspect_shared_builtin_mcp_system_prompt(servers.as_slice(), BuiltinMcpPromptLocale::ZhCn);
    assert!(debug
        .selected_section_ids
        .contains(&"builtin_task_manager".to_string()));
}

#[test]
fn shared_runtime_public_facade_builds_initialized_shared_mcp_executor() {
    let options = BuiltinMcpServerOptions::new("/tmp/chatos-shared-runtime-test")
        .with_user_id("user-1")
        .with_project_id("project-1");
    let executor = build_initialized_shared_builtin_mcp_executor(
        vec![SharedBuiltinMcpKind::TaskManager],
        options,
    )
    .expect("build initialized shared executor");
    assert!(!executor.available_tools().is_empty());
    let effective_prompt =
        compose_effective_shared_builtin_mcp_system_prompt(&executor, BuiltinMcpPromptLocale::ZhCn)
            .expect("effective builtin prompt");
    assert!(effective_prompt.contains("`task_manager_add_task`"));

    let effective_debug =
        inspect_effective_shared_builtin_mcp_system_prompt(&executor, BuiltinMcpPromptLocale::ZhCn);
    assert!(effective_debug
        .active_builtin_server_names
        .contains(&"task_manager".to_string()));

    let _runtime = build_ai_runtime_from_shared_mcp_executor(executor);
}

#[test]
fn shared_runtime_public_facade_exports_runtime_builder() {
    let executor = chatos_mcp_runtime::McpExecutor::new(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        chatos_mcp_runtime::BuiltinToolRegistry::new(),
    );
    let runner = AiRuntimeBuilder::new()
        .with_mcp_executor(executor)
        .with_max_iterations(2)
        .build_contextual_turn_runner();

    let _runtime = runner.runtime();

    let _executor = SharedMcpExecutorBuilder::new().build();
    let _task_runtime = TaskRuntimeBuilder::new()
        .with_builtin_only_mcp_executor_builder(SharedMcpExecutorBuilder::new())
        .expect("builtin-only mcp builder")
        .build();
    let task_runtime_config = TaskRuntimeConfig::new()
        .with_mcp_init_mode(TaskMcpInitMode::Disabled)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
        .with_memory_engine(Some(TaskMemoryRuntimeConfig::new(
            "http://127.0.0.1:1",
            "task_runner",
        )));
    assert_eq!(task_runtime_config.mcp_init_mode, TaskMcpInitMode::Disabled);
    assert!(task_runtime_config.memory_engine.is_some());
    let execution = TaskRunExecution::for_user_text(
        task_runtime_config,
        "task_1",
        "run_1",
        ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "test-key",
            "task-model",
            "gpt",
        ),
        "run task",
    );
    assert_eq!(execution.run_spec.task_id, "task_1");
}

#[test]
fn shared_runtime_public_facade_converts_resolved_model_config() {
    let resolved = ResolvedChatModelConfig {
        model: "gpt-test".to_string(),
        provider: "gpt".to_string(),
        thinking_level: Some("medium".to_string()),
        temperature: 0.3,
        supports_images: true,
        supports_responses: true,
        effective_reasoning: true,
        api_key: "secret".to_string(),
        base_url: "http://127.0.0.1:8080/v1".to_string(),
        system_prompt: Some("system".to_string()),
        use_active_system_context: true,
        use_codex_gateway_mcp_passthrough: false,
    };

    let config = build_model_runtime_config_from_resolved(&resolved);
    assert_eq!(config.model, "gpt-test");
    assert_eq!(config.provider, "gpt");
    assert_eq!(config.api_key, "secret");
    assert_eq!(config.base_url, "http://127.0.0.1:8080/v1");
    assert_eq!(config.thinking_level.as_deref(), Some("medium"));
    assert_eq!(config.temperature, Some(0.3));
    assert_eq!(config.instructions.as_deref(), Some("system"));
    assert!(config.supports_responses);

    let report = AiTurnReport::aborted();
    assert_eq!(report.status, AiTurnStatus::Aborted);
}
