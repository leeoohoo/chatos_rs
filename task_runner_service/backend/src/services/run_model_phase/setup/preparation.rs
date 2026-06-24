use super::*;
use crate::config::AppConfig;
use crate::models::{normalize_project_id, PUBLIC_PROJECT_ID};

pub(super) async fn prepare_model_execution(
    service: &RunService,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
    run: &mut TaskRunRecord,
    input: &StartTaskRunRequest,
    effective_workspace_dir: &str,
    prerequisite_context: &[PrerequisiteTaskContext],
) -> Result<PreparedModelExecution, String> {
    let loaded_external_mcp = load_external_mcp_servers(service, task).await?;
    let system_http_servers = load_system_http_mcp_servers(service, task)?;
    let prompt = build_task_prompt(
        task,
        input.prompt_override.as_deref(),
        prerequisite_context,
        task.mcp_config.locale(),
    );
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
        external_mcp_prefixed_input_items(
            loaded_external_mcp.summaries.as_slice(),
            task.mcp_config.locale(),
        ),
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
    if !loaded_external_mcp.summaries.is_empty() {
        info!(
            task_id = task.id.as_str(),
            run_id = run.id.as_str(),
            external_mcp_servers = %loaded_external_mcp
                .summaries
                .iter()
                .map(|summary| format!("{}:{}:{}", summary.id, summary.name, summary.transport))
                .collect::<Vec<_>>()
                .join(","),
            "task runner loaded external MCP servers"
        );
    }

    let mcp_builder = McpExecutorBuilder::new()
        .with_http_servers(system_http_servers)
        .with_http_servers(loaded_external_mcp.http_servers)
        .with_stdio_servers(loaded_external_mcp.stdio_servers)
        .with_builtin_servers(builtin_servers)
        .with_builtin_registry(builtin_registry);

    Ok(PreparedModelExecution {
        run_spec,
        runtime_config,
        mcp_builder,
        tool_result_model_budget_limits,
    })
}

#[derive(Debug, Clone)]
struct LoadedExternalMcpServers {
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    summaries: Vec<ExternalMcpRuntimeSummary>,
}

#[derive(Debug, Clone)]
struct ExternalMcpRuntimeSummary {
    id: String,
    name: String,
    transport: String,
}

async fn load_external_mcp_servers(
    service: &RunService,
    task: &TaskRecord,
) -> Result<LoadedExternalMcpServers, String> {
    if !task.mcp_config.enabled || task.mcp_config.external_mcp_config_ids.is_empty() {
        return Ok(LoadedExternalMcpServers {
            http_servers: Vec::new(),
            stdio_servers: Vec::new(),
            summaries: Vec::new(),
        });
    }

    let mut http_servers = Vec::new();
    let mut stdio_servers = Vec::new();
    let mut summaries = Vec::new();
    for config_id in &task.mcp_config.external_mcp_config_ids {
        let config = service
            .store
            .get_external_mcp_config(config_id)
            .await?
            .ok_or_else(|| format!("外部 MCP 配置不存在: {config_id}"))?;
        if !config.enabled {
            return Err(format!("外部 MCP 配置未启用: {config_id}"));
        }
        if let Some(server) = config.to_http_server() {
            http_servers.push(server);
        } else if let Some(server) = config.to_stdio_server() {
            stdio_servers.push(server);
        } else {
            return Err(format!("外部 MCP 配置无效: {config_id}"));
        }
        summaries.push(ExternalMcpRuntimeSummary {
            id: config.id,
            name: config.name,
            transport: config.transport,
        });
    }
    Ok(LoadedExternalMcpServers {
        http_servers,
        stdio_servers,
        summaries,
    })
}

fn load_system_http_mcp_servers(
    service: &RunService,
    task: &TaskRecord,
) -> Result<Vec<McpHttpServer>, String> {
    let mut servers = Vec::new();
    if let Some(server) = build_chatos_plan_project_management_server(&service.config, task)? {
        servers.push(server);
    }
    Ok(servers)
}

fn build_chatos_plan_project_management_server(
    config: &AppConfig,
    task: &TaskRecord,
) -> Result<Option<McpHttpServer>, String> {
    if !is_chatos_plan_task(task) {
        return Ok(None);
    }
    let base_url = config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project service base url is not configured".to_string())?;
    let sync_secret = config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project service sync secret is not configured".to_string())?;
    let project_id = normalize_project_id(Some(task.project_id.clone()));
    if project_id == PUBLIC_PROJECT_ID {
        return Err("Chatos Plan mode requires concrete project_id".to_string());
    }
    let owner_user_id = task
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "plan task missing owner_user_id".to_string())?;
    let mut headers = std::collections::HashMap::new();
    headers.insert(
        "X-Project-Service-Sync-Secret".to_string(),
        sync_secret.to_string(),
    );
    headers.insert(
        "X-Task-Runner-Owner-User-Id".to_string(),
        owner_user_id.to_string(),
    );
    if let Some(username) = task
        .owner_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert(
            "X-Task-Runner-Owner-Username".to_string(),
            username.to_string(),
        );
    }
    if let Some(display_name) = task
        .owner_display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert(
            "X-Task-Runner-Owner-Display-Name".to_string(),
            display_name.to_string(),
        );
    }
    headers.insert("X-Chatos-Project-Id".to_string(), project_id);
    headers.insert(
        "X-Task-Runner-Task-Profile".to_string(),
        crate::models::TASK_PROFILE_CHATOS_PLAN.to_string(),
    );
    Ok(Some(
        McpHttpServer::new(
            PROJECT_MANAGEMENT_MCP_SERVER_NAME,
            format!("{}/mcp", base_url.trim_end_matches('/')),
        )
        .with_headers(headers),
    ))
}

fn external_mcp_prefixed_input_items(
    summaries: &[ExternalMcpRuntimeSummary],
    locale: BuiltinMcpPromptLocale,
) -> Vec<Value> {
    if summaries.is_empty() {
        return Vec::new();
    }

    let list = summaries
        .iter()
        .map(|summary| {
            format!(
                "- {} (id: {}, transport: {})",
                summary.name, summary.id, summary.transport
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = if locale.is_english() {
        format!(
            "[External MCP]\nTask Runner has loaded these user-configured external MCP servers for this task:\n{list}\n\nIf the task objective asks you to use these external systems, directly call the corresponding tools currently exposed to you. External MCP tool names usually use the config name as their prefix. Do not inspect local Gemini/Codex/Claude MCP config files to decide whether these MCP servers exist; they are injected by Task Runner for this run. Use builtin tools only when the task also needs local code, terminal, browser, or other builtin capabilities."
        )
    } else {
        format!(
            "[外部 MCP]\nTask Runner 已为当前任务加载这些用户配置的外部 MCP：\n{list}\n\n如果任务目标要求使用这些外部系统，请直接调用当前暴露给你的对应工具。外部 MCP 工具名通常会以配置名称作为前缀。不要检查本机 Gemini/Codex/Claude 的 MCP 配置文件来判断这些 MCP 是否存在；它们已经由 Task Runner 在本次运行中注入。只有当任务同时需要本地代码、终端、浏览器或其他 builtin 能力时，才使用 builtin 工具。"
        )
    };

    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    })]
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
    external_mcp_prefixed_input_items: Vec<Value>,
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
    let mut prefixed_input_items = external_mcp_prefixed_input_items;
    if task_process_logging_enabled {
        prefixed_input_items.extend(task_process_log_prefixed_input_items(
            task.mcp_config.locale(),
        ));
    }
    if !prefixed_input_items.is_empty() {
        run_spec = run_spec.with_prefixed_input_items(prefixed_input_items);
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
            .with_access_token(crate::auth::get_current_access_token())
            .with_operator_token(service.config.memory_engine_operator_token.clone())
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

    let selected_builtin_kinds = runtime_selected_builtin_kinds(task);
    let selected_builtin_kind_names = selected_builtin_kinds
        .iter()
        .map(|kind| kind.kind_name().to_string())
        .collect::<Vec<_>>();
    info!(
        task_id = task.id.as_str(),
        run_id = run.id.as_str(),
        builtin_mcp_count = selected_builtin_kind_names.len(),
        builtin_mcp_kinds = %selected_builtin_kind_names.join(","),
        external_mcp_config_count = task.mcp_config.external_mcp_config_ids.len(),
        external_mcp_config_ids = %task.mcp_config.external_mcp_config_ids.join(","),
        "task runner resolved MCP selection"
    );
    let mut builtin_servers =
        builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
    if is_chatos_plan_task(task) {
        builtin_servers.push(
            chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
                .server_with_options(&server_options),
        );
    }
    if task_process_logging_enabled {
        builtin_servers.push(task_process_log_builtin_server());
    }

    let (builtin_registry, builtin_init_errors) = build_builtin_registry(
        &builtin_servers,
        task_service.clone(),
        service.ask_user_prompt_service.clone(),
    );
    let mut builtin_registry = builtin_registry;
    if is_chatos_plan_task(task) {
        builtin_registry.register(DisabledBuiltinProvider::code_maintainer_write_for_chatos_plan());
    }
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

fn is_chatos_plan_task(task: &TaskRecord) -> bool {
    task.task_profile
        .trim()
        .eq_ignore_ascii_case(crate::models::TASK_PROFILE_CHATOS_PLAN)
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use crate::config::StoreMode;
    use crate::models::{now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus};

    use super::*;

    #[test]
    fn chatos_plan_project_management_server_includes_required_headers() {
        let config = test_config();
        let task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");

        let server = build_chatos_plan_project_management_server(&config, &task)
            .expect("build server")
            .expect("plan server");

        assert_eq!(server.name, PROJECT_MANAGEMENT_MCP_SERVER_NAME);
        assert_eq!(server.url, "http://127.0.0.1:39210/mcp");
        let headers = server.headers.expect("headers");
        assert_eq!(
            headers
                .get("X-Project-Service-Sync-Secret")
                .map(String::as_str),
            Some("sync-secret")
        );
        assert_eq!(
            headers
                .get("X-Task-Runner-Owner-User-Id")
                .map(String::as_str),
            Some("owner-1")
        );
        assert_eq!(
            headers.get("X-Chatos-Project-Id").map(String::as_str),
            Some("project-1")
        );
        assert_eq!(
            headers
                .get("X-Task-Runner-Task-Profile")
                .map(String::as_str),
            Some(crate::models::TASK_PROFILE_CHATOS_PLAN)
        );
    }

    #[test]
    fn chatos_plan_project_management_server_requires_concrete_project() {
        let config = test_config();
        let task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, PUBLIC_PROJECT_ID);

        let err = build_chatos_plan_project_management_server(&config, &task)
            .expect_err("public project should fail");

        assert_eq!(err, "Chatos Plan mode requires concrete project_id");
    }

    #[test]
    fn default_task_does_not_inject_project_management_server() {
        let config = test_config();
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");

        assert!(build_chatos_plan_project_management_server(&config, &task)
            .expect("default task build")
            .is_none());
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            store_mode: StoreMode::Memory,
            database_url: "memory://plan-runtime-preparation-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(30_000),
            execution_timeout: Duration::from_millis(30_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 1_000,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            project_service_base_url: Some("http://127.0.0.1:39210".to_string()),
            project_service_sync_secret: Some("sync-secret".to_string()),
            project_service_request_timeout: Duration::from_millis(5_000),
        }
    }

    fn sample_task(task_profile: &str, project_id: &str) -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: "task-1".to_string(),
            title: "task".to_string(),
            description: None,
            objective: "objective".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "memory-1".to_string(),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: project_id.to_string(),
            task_profile: task_profile.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: Default::default(),
            mcp_config: TaskMcpConfig::default(),
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }
}
