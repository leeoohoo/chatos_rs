use std::sync::Arc;

use serde_json::json;

use super::{
    TaskBuiltinMcpPromptMode, TaskMcpInitMode, TaskMemoryRuntimeConfig, TaskRunExecution,
    TaskRunSpec, TaskRuntime, TaskRuntimeConfig,
};
use crate::{
    AiRuntime, AiTurnStatus, ContextualTurnRunner, MemoryRecordScope, MemoryScope,
    ModelRuntimeConfig, RuntimeRecordOptions,
};

#[test]
fn task_run_spec_serializes_model_config_id_and_builds_turn_request() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    )
    .with_responses_support(true);
    let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
        .with_model_config_id("model_cfg_1")
        .with_memory_scope(Some(MemoryScope::thread(
            "tenant_1",
            "task_runner",
            "task_1",
        )))
        .with_prefixed_input_items(vec![json!({"role":"system","content":"prefix"})])
        .with_tools(vec![json!({"type":"function","name":"tool_1"})]);

    let encoded = serde_json::to_string(&spec).expect("serialize task spec");
    let decoded: TaskRunSpec =
        serde_json::from_str(encoded.as_str()).expect("deserialize task spec");
    let request = decoded.into_contextual_turn_request();

    assert_eq!(request.model_request.model, "gpt-test");
    assert!(request.model_request.supports_responses);
    assert_eq!(request.model_request.tools.len(), 1);
    assert_eq!(
        request.runtime_options.conversation_id.as_deref(),
        Some("task_1")
    );
    assert_eq!(
        request.runtime_options.conversation_turn_id.as_deref(),
        Some("run_1")
    );
    assert_eq!(
        request.runtime_options.caller_model.as_deref(),
        Some("gpt-test")
    );
    assert!(
        request
            .runtime_options
            .record_options
            .persist_assistant_records
    );
    assert!(request.runtime_options.record_options.persist_tool_records);
    assert_eq!(
        request.prefixed_input_items[0]["content"].as_str(),
        Some("prefix")
    );
    assert_eq!(
        request.current_input_items[0]["content"].as_str(),
        Some("execute it")
    );
    assert_eq!(
        request
            .user_record
            .as_ref()
            .and_then(|record| record.metadata.as_ref())
            .and_then(|metadata| metadata.get("model_config_id"))
            .and_then(|value| value.as_str()),
        Some("model_cfg_1")
    );
}

#[test]
fn task_run_spec_injects_configured_builtin_mcp_prompt_from_executor() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
    let executor = chatos_mcp_runtime::McpExecutor::builder()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .build();

    let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
        .with_configured_builtin_mcp_prompt_from_executor(
            &executor,
            chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
        );
    let encoded = serde_json::to_string(&spec).expect("serialize task spec");
    let decoded: TaskRunSpec =
        serde_json::from_str(encoded.as_str()).expect("deserialize task spec");

    assert_eq!(
        decoded
            .builtin_mcp_prompt
            .as_ref()
            .map(|snapshot| &snapshot.mode),
        Some(&TaskBuiltinMcpPromptMode::Configured)
    );
    assert!(decoded
        .prefixed_input_items
        .first()
        .and_then(|item| item.get("content"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|content| content.contains("`task_manager_add_task`")));
}

#[test]
fn task_run_spec_replaces_builtin_mcp_prompt_snapshot() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
    let mut executor = chatos_mcp_runtime::McpExecutor::builder()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .build();
    executor.init_builtin_only().expect("builtin init");

    let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
        .with_prefixed_input_items(vec![json!({"role":"system","content":"custom"})])
        .with_configured_builtin_mcp_prompt_from_executor(
            &executor,
            chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
        )
        .with_effective_builtin_mcp_prompt_from_executor(
            &executor,
            chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
        );

    assert_eq!(
        spec.builtin_mcp_prompt
            .as_ref()
            .map(|snapshot| &snapshot.mode),
        Some(&TaskBuiltinMcpPromptMode::Effective)
    );
    assert_eq!(spec.prefixed_input_items.len(), 1);
    assert_eq!(
        spec.prefixed_input_items[0]
            .get("content")
            .and_then(serde_json::Value::as_str),
        Some("custom")
    );
}

#[test]
fn task_runtime_builder_prepares_configured_builtin_prompt() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
    let executor = chatos_mcp_runtime::McpExecutor::builder()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .build();
    let runtime = TaskRuntime::builder()
        .with_mcp_executor(executor)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
        .build();

    let spec = runtime.prepare_spec(TaskRunSpec::new("task_1", "run_1", config, "execute it"));

    assert_eq!(runtime.mcp_executor().map(|_| true), Some(true));
    assert_eq!(
        runtime.builtin_prompt_mode(),
        TaskBuiltinMcpPromptMode::Configured
    );
    assert!(spec
        .prefixed_input_items
        .first()
        .and_then(|item| item.get("content"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|content| content.contains("`task_manager_add_task`")));
}

#[test]
fn task_runtime_builder_defaults_to_effective_builtin_prompt() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
    let executor = chatos_mcp_runtime::McpExecutor::builder()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .build_builtin_only()
        .expect("builtin init");
    let runtime = TaskRuntime::builder().with_mcp_executor(executor).build();

    let spec = runtime.prepare_spec(TaskRunSpec::new("task_1", "run_1", config, "execute it"));

    assert_eq!(
        runtime.builtin_prompt_mode(),
        TaskBuiltinMcpPromptMode::Effective
    );
    assert!(spec.prefixed_input_items.is_empty());
    assert_eq!(
        spec.builtin_mcp_prompt
            .as_ref()
            .map(|snapshot| &snapshot.mode),
        Some(&TaskBuiltinMcpPromptMode::Effective)
    );
}

#[test]
fn task_runtime_builder_accepts_builtin_only_mcp_builder() {
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
    let mcp_builder = chatos_mcp_runtime::McpExecutor::builder()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options);

    let runtime = TaskRuntime::builder()
        .with_builtin_only_mcp_executor_builder(mcp_builder)
        .expect("builtin-only mcp executor")
        .build();

    assert!(runtime.mcp_executor().is_some());
    assert!(runtime
        .mcp_executor()
        .expect("executor")
        .unavailable_tools()
        .iter()
        .any(
            |item| item.get("reason").and_then(serde_json::Value::as_str)
                == Some("missing builtin provider")
        ));
}

#[tokio::test]
async fn task_runtime_builder_accepts_initialized_mcp_builder() {
    let mcp_builder = chatos_mcp_runtime::McpExecutor::builder();

    let runtime = TaskRuntime::builder()
        .with_initialized_mcp_executor_builder(mcp_builder)
        .await
        .expect("initialized mcp executor")
        .build();

    assert!(runtime.mcp_executor().is_some());
    assert!(runtime
        .mcp_executor()
        .expect("executor")
        .available_tools()
        .is_empty());
}

#[test]
fn task_runtime_config_serializes_runtime_shape() {
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new("/tmp/task-runtime");
    let config = TaskRuntimeConfig::new()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .with_mcp_init_mode(TaskMcpInitMode::BuiltinOnly)
        .with_builtin_prompt_locale(chatos_mcp_runtime::BuiltinMcpPromptLocale::EnUs)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
        .with_max_iterations(Some(7));

    let encoded = serde_json::to_string(&config).expect("serialize runtime config");
    let decoded: TaskRuntimeConfig =
        serde_json::from_str(encoded.as_str()).expect("deserialize runtime config");

    assert_eq!(decoded.builtin_servers.len(), 1);
    assert_eq!(decoded.mcp_init_mode, TaskMcpInitMode::BuiltinOnly);
    assert_eq!(
        decoded.builtin_prompt_locale,
        chatos_mcp_runtime::BuiltinMcpPromptLocale::EnUs
    );
    assert_eq!(
        decoded.builtin_prompt_mode,
        TaskBuiltinMcpPromptMode::Configured
    );
    assert_eq!(decoded.max_iterations, Some(7));
    assert_eq!(
        decoded.builtin_servers[0].name,
        chatos_mcp_runtime::TASK_MANAGER_SERVER_NAME
    );
}

#[test]
fn task_memory_runtime_config_serializes_direct_memory_settings() {
    let memory = TaskMemoryRuntimeConfig::new("http://127.0.0.1:1", "task_runner")
        .with_timeout_ms(500)
        .with_compose_context(false)
        .with_record_scope(Some(MemoryRecordScope::message_thread(
            "tenant_1", "thread_1",
        )));
    let config = TaskRuntimeConfig::new().with_memory_engine(Some(memory));

    let encoded = serde_json::to_string(&config).expect("serialize memory runtime config");
    let decoded: TaskRuntimeConfig =
        serde_json::from_str(encoded.as_str()).expect("deserialize memory runtime config");
    let memory = decoded.memory_engine.expect("memory config");

    assert_eq!(memory.base_url, "http://127.0.0.1:1");
    assert_eq!(memory.source_id, "task_runner");
    assert_eq!(memory.timeout_ms, 500);
    assert!(!memory.compose_context);
    assert!(memory.retry_on_context_overflow);
    assert_eq!(memory.active_summary_poll_interval_ms, 10_000);
    assert_eq!(memory.active_summary_poll_timeout_ms, 120_000);
    assert_eq!(
        memory
            .record_scope
            .as_ref()
            .map(|scope| scope.record_type.as_str()),
        Some("message")
    );
}

#[tokio::test]
async fn task_runtime_config_builds_builtin_only_runtime() {
    let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new("/tmp/task-runtime");
    let config = TaskRuntimeConfig::new()
        .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
        .with_mcp_init_mode(TaskMcpInitMode::BuiltinOnly)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Effective);

    let runtime = config.build_runtime().await.expect("runtime");

    assert!(runtime.mcp_executor().is_some());
    assert!(runtime
        .mcp_executor()
        .expect("executor")
        .unavailable_tools()
        .iter()
        .any(
            |item| item.get("reason").and_then(serde_json::Value::as_str)
                == Some("missing builtin provider")
        ));
}

#[tokio::test]
async fn task_runtime_config_can_disable_mcp() {
    let config = TaskRuntimeConfig::new()
        .with_mcp_init_mode(TaskMcpInitMode::Disabled)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured);

    let runtime = config.build_runtime().await.expect("runtime");

    assert!(runtime.mcp_executor().is_none());
    assert_eq!(
        runtime.builtin_prompt_mode(),
        TaskBuiltinMcpPromptMode::Configured
    );
}

#[tokio::test]
async fn task_runtime_config_builds_with_memory_engine_config() {
    let memory = TaskMemoryRuntimeConfig::new("http://127.0.0.1:1", "task_runner")
        .with_timeout_ms(100)
        .with_record_scope(Some(MemoryRecordScope::message_thread(
            "tenant_1", "thread_1",
        )));
    let config = TaskRuntimeConfig::new()
        .with_mcp_init_mode(TaskMcpInitMode::Disabled)
        .with_memory_engine(Some(memory));

    let runtime = config.build_runtime().await.expect("runtime");

    assert!(runtime.mcp_executor().is_none());
}

#[test]
fn task_run_execution_serializes_runtime_config_and_spec() {
    let runtime_config = TaskRuntimeConfig::new()
        .with_mcp_init_mode(TaskMcpInitMode::Disabled)
        .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured);
    let model_config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:8080/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let execution =
        TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go")
            .with_model_config_id("model_cfg_1");

    let encoded = serde_json::to_string(&execution).expect("serialize execution");
    let decoded: TaskRunExecution =
        serde_json::from_str(encoded.as_str()).expect("deserialize execution");

    assert_eq!(
        decoded.runtime_config.mcp_init_mode,
        TaskMcpInitMode::Disabled
    );
    assert_eq!(decoded.run_spec.task_id, "task_1");
    assert_eq!(decoded.run_spec.run_id, "run_1");
    assert_eq!(
        decoded.run_spec.model_config_id.as_deref(),
        Some("model_cfg_1")
    );
}

#[tokio::test]
async fn task_run_execution_runs_with_runtime_options() {
    let runtime_config = TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Disabled);
    let model_config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:1/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let execution =
        TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go");
    let options = execution
        .run_spec
        .runtime_options()
        .with_abort_checker(Some(Arc::new(|conversation_id| {
            conversation_id == "task_1"
        })));

    let report = execution.run_report_with_options(options).await;

    assert_eq!(report.status, AiTurnStatus::Aborted);
    assert_eq!(report.task_id, "task_1");
    assert_eq!(report.run_id, "run_1");
}

#[test]
fn task_run_execution_wraps_runtime_init_failure_report() {
    let runtime_config = TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Full);
    let model_config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:1/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let execution =
        TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go");

    let report = execution.runtime_init_failed_report("boom");

    assert_eq!(report.status, AiTurnStatus::Failed);
    assert!(report
        .error
        .as_deref()
        .is_some_and(|error| error.contains("runtime init failed")));
}

#[tokio::test]
async fn task_runner_report_captures_aborted_runtime() {
    let config = ModelRuntimeConfig::openai_compatible(
        "http://127.0.0.1:1/v1",
        "secret",
        "gpt-test",
        "openai",
    );
    let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
        .with_model_config_id("model_cfg_1")
        .with_record_options(RuntimeRecordOptions::default());
    let options = spec
        .runtime_options()
        .with_abort_checker(Some(Arc::new(|conversation_id| {
            conversation_id == "task_1"
        })));
    let runner = ContextualTurnRunner::new(AiRuntime::new(None), None);

    let report = runner.run_task_report_with_options(spec, options).await;

    assert_eq!(report.task_id, "task_1");
    assert_eq!(report.run_id, "run_1");
    assert_eq!(report.model_config_id.as_deref(), Some("model_cfg_1"));
    assert_eq!(report.status, AiTurnStatus::Aborted);
    assert!(report.is_aborted());
    assert_eq!(report.user_message(), "任务已取消。");
}
