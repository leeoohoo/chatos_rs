use super::*;

pub(super) async fn prepare_model_execution(
    service: &RunService,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
    run: &mut TaskRunRecord,
    input: &StartTaskRunRequest,
    effective_workspace_dir: &str,
    prerequisite_context: &[PrerequisiteTaskContext],
) -> Result<PreparedModelExecution, String> {
    let prompt = build_task_prompt(task, input.prompt_override.as_deref(), prerequisite_context);
    let metadata = build_execution_metadata(task, run, model_config);
    let task_process_logging_enabled = task_process_logging_enabled(&task.mcp_config);
    let mut run_spec = build_run_spec(
        task,
        run,
        model_config,
        effective_workspace_dir,
        prompt,
        metadata.clone(),
        task_process_logging_enabled,
    );

    let memory_scope = build_memory_scope(service, task);
    run_spec = run_spec.with_memory_scope(Some(memory_scope));

    let tool_result_model_budget_limits = service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(|err| format!("加载运行时配置失败: {err}"))?;
    let runtime_config = build_runtime_config(service, task).await?;

    let runtime_config = service.apply_task_mcp_config(runtime_config, &task.mcp_config);
    persist_context_snapshot(service, run, run_spec.memory_scope.as_ref()).await;

    let task_service = TaskService::new(service.config.clone(), service.store.clone());
    let (builtin_servers, builtin_registry) = build_mcp_builder_parts(
        service,
        task,
        run,
        effective_workspace_dir,
        task_process_logging_enabled,
        task_service.clone(),
    )
    .await;

    let mcp_builder = McpExecutorBuilder::new()
        .with_builtin_servers(builtin_servers)
        .with_builtin_registry(builtin_registry);

    Ok(PreparedModelExecution {
        run_spec,
        runtime_config,
        mcp_builder,
        tool_result_model_budget_limits,
    })
}

fn build_execution_metadata(
    task: &TaskRecord,
    run: &TaskRunRecord,
    model_config: &ModelConfigRecord,
) -> serde_json::Value {
    json!({
        "task_id": task.id,
        "run_id": run.id,
        "model_config_id": model_config.id,
        "service": "task_runner_service",
    })
}

fn build_run_spec(
    task: &TaskRecord,
    run: &TaskRunRecord,
    model_config: &ModelConfigRecord,
    effective_workspace_dir: &str,
    prompt: String,
    metadata: serde_json::Value,
    task_process_logging_enabled: bool,
) -> TaskRunSpec {
    let mut effective_model_config = model_config.clone();
    effective_model_config.request_cwd = Some(effective_workspace_dir.to_string());
    let model_runtime_config =
        effective_model_config.to_runtime_config(Some(effective_workspace_dir.to_string()));

    let mut run_spec = TaskRunSpec::new(
        task.id.clone(),
        run.id.clone(),
        model_runtime_config,
        prompt.clone(),
    )
    .with_model_config_id(model_config.id.clone())
    .with_metadata(Some(metadata.clone()))
    .with_record_options(
        RuntimeRecordOptions::persist_all()
            .with_assistant_message_mode("task_run")
            .with_assistant_message_source("task_runner")
            .with_tool_message_mode("task_tool")
            .with_tool_message_source("task_runner")
            .with_assistant_metadata(metadata.clone())
            .with_tool_metadata(metadata.clone()),
    )
    .with_user_record(Some(
        SaveRecordInput::user_message(run.id.clone(), prompt)
            .with_conversation_turn_id(run.id.clone())
            .with_message_mode("task_run")
            .with_message_source("task_runner")
            .with_metadata(metadata),
    ));
    if task_process_logging_enabled {
        run_spec = run_spec.with_prefixed_input_items(task_process_log_prefixed_input_items(
            task.mcp_config.locale(),
        ));
    }
    run_spec
}

fn build_memory_scope(service: &RunService, task: &TaskRecord) -> MemoryScope {
    MemoryScope::thread(
        task.tenant_id.clone(),
        service.config.memory_engine_source_id.clone(),
        task.memory_thread_id.clone(),
    )
    .with_subject_id(task.subject_id.clone())
}

async fn build_runtime_config(
    service: &RunService,
    task: &TaskRecord,
) -> Result<TaskRuntimeConfig, String> {
    let max_iterations = service
        .effective_task_execution_max_iterations()
        .await
        .map_err(|err| format!("加载运行时配置失败: {err}"))?;

    let mut runtime_config = TaskRuntimeConfig::new().with_max_iterations(Some(max_iterations));
    if let Some(memory_engine_base_url) = service.config.memory_engine_base_url.clone() {
        runtime_config = runtime_config.with_memory_engine(Some(
            TaskMemoryRuntimeConfig::new(
                memory_engine_base_url,
                service.config.memory_engine_source_id.clone(),
            )
            .with_timeout_ms(service.config.memory_timeout.as_millis() as u64)
            .with_record_scope(Some(MemoryRecordScope::message_thread(
                task.tenant_id.clone(),
                task.memory_thread_id.clone(),
            ))),
        ));
    }

    Ok(runtime_config)
}

async fn persist_context_snapshot(
    service: &RunService,
    run: &mut TaskRunRecord,
    memory_scope: Option<&MemoryScope>,
) {
    if let Some(snapshot) = service.compose_context_snapshot(memory_scope).await {
        run.context_snapshot = Some(snapshot);
        run.updated_at = now_rfc3339();
        if let Err(err) = service.store.save_run(run.clone()).await {
            warn!(
                "failed to persist context snapshot for run {}: {}",
                run.id, err
            );
        }
    }
}

async fn build_mcp_builder_parts(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
    effective_workspace_dir: &str,
    task_process_logging_enabled: bool,
    task_service: TaskService,
) -> (
    Vec<chatos_mcp_runtime::McpBuiltinServer>,
    chatos_mcp_runtime::BuiltinToolRegistry,
) {
    let mut server_options = BuiltinMcpServerOptions::new(effective_workspace_dir.to_string())
        .with_user_id(task.subject_id.clone())
        .with_project_id(task.id.clone())
        .with_auto_create_task(true);
    if let Some(remote_server_id) = task.mcp_config.default_remote_server_id.clone() {
        server_options = server_options.with_remote_connection_id(remote_server_id);
    }

    let selected_builtin_kinds = selected_builtin_kinds(&task.mcp_config);
    let mut builtin_servers =
        builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
    if task_process_logging_enabled {
        builtin_servers.push(task_process_log_builtin_server());
    }

    let (builtin_registry, builtin_init_errors) = build_builtin_registry(
        &builtin_servers,
        task_service.clone(),
        service.ui_prompt_service.clone(),
    );
    let mut builtin_registry = builtin_registry;
    if task_process_logging_enabled {
        builtin_registry.register(TaskProcessLogBuiltinProvider::new(
            TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
            task_service,
            task.id.clone(),
            run.id.clone(),
        ));
    }

    persist_builtin_init_errors(service, run, builtin_init_errors).await;
    (builtin_servers, builtin_registry)
}

async fn persist_builtin_init_errors(
    service: &RunService,
    run: &TaskRunRecord,
    builtin_init_errors: Vec<String>,
) {
    for err in builtin_init_errors {
        if let Err(event_err) = service
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "builtin_provider_warning",
                Some(err.clone()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append builtin warning event for run {}: {}",
                run.id, event_err
            );
        }
        warn!("task runner builtin provider warning: {err}");
    }
}
