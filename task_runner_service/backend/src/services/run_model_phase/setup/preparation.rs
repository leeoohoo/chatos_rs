use super::*;

mod mcp_builder;
mod mcp_inputs;

use mcp_builder::build_mcp_builder_parts;
#[cfg(test)]
use mcp_inputs::project_management_skill_prefixed_input_item;
use mcp_inputs::{
    external_mcp_prefixed_input_items, load_external_mcp_servers, load_system_http_mcp_servers,
    project_management_skill_prefixed_input_items, user_skill_prefixed_input_items,
};

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
    let mut prefixed_input_items =
        project_management_skill_prefixed_input_items(service, task, task.mcp_config.locale())
            .await;
    prefixed_input_items.extend(
        user_skill_prefixed_input_items(
            service,
            task,
            task.mcp_config.locale(),
            effective_workspace_dir,
        )
        .await,
    );
    prefixed_input_items.extend(external_mcp_prefixed_input_items(
        loaded_external_mcp.summaries.as_slice(),
        task.mcp_config.locale(),
    ));
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
        prefixed_input_items,
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

fn is_chatos_plan_task(task: &TaskRecord) -> bool {
    task.task_profile
        .trim()
        .eq_ignore_ascii_case(crate::models::TASK_PROFILE_CHATOS_PLAN)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use crate::config::{AppConfig, StoreMode};
    use crate::models::{now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus};
    use crate::services::project_management_api_client::ProjectManagementSkillDocument;
    use serde_json::json;
    use serde_json::Value;
    use tokio::sync::broadcast;

    use super::*;

    #[test]
    fn project_management_skill_prefixed_item_wraps_service_skill() {
        let item = project_management_skill_prefixed_input_item(
            ProjectManagementSkillDocument {
                name: "project-management-mcp-agent-zh-cn".to_string(),
                locale: "zh-CN".to_string(),
                content: "# Project Management MCP Agent Skill\n\nbody".to_string(),
            },
            BuiltinMcpPromptLocale::ZhCn,
        )
        .expect("prefixed item");
        let text = item
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("system text");

        assert!(text.contains("Project Management 服务加载"));
        assert!(text.contains("project-management-mcp-agent-zh-cn"));
        assert!(text.contains("# Project Management MCP Agent Skill"));
    }

    #[tokio::test]
    async fn chatos_plan_builtin_servers_include_project_management_provider() {
        let config = test_config();
        let service = test_run_service(config);
        let task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");
        let run = sample_run(&task);
        let task_service = TaskService::new(service.config.clone(), service.store.clone());

        let (builtin_servers, builtin_registry) =
            build_mcp_builder_parts(&service, &task, &run, ".", false, task_service).await;
        let server = builtin_servers
            .iter()
            .find(|server| server.name == chatos_mcp_runtime::PROJECT_MANAGEMENT_SERVER_NAME)
            .expect("project management builtin server");

        assert_eq!(
            server.kind.as_str(),
            chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement.kind_name()
        );
        assert_eq!(server.user_id.as_deref(), Some("owner-1"));
        assert_eq!(server.project_id.as_deref(), Some("project-1"));

        let executor = chatos_mcp_runtime::McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry)
            .build_builtin_only()
            .expect("builtin executor");
        let tool_names = executor
            .available_tools()
            .into_iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(|name| name.as_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        assert!(tool_names
            .iter()
            .any(|name| name == "project_management_service_create_requirement"));
    }

    #[tokio::test]
    async fn default_task_does_not_include_project_management_builtin() {
        let config = test_config();
        let service = test_run_service(config);
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let run = sample_run(&task);
        let task_service = TaskService::new(service.config.clone(), service.store.clone());

        let system_servers = load_system_http_mcp_servers(&service, &task).expect("system servers");
        assert!(system_servers.is_empty());

        let (builtin_servers, builtin_registry) =
            build_mcp_builder_parts(&service, &task, &run, ".", false, task_service).await;
        assert!(builtin_servers
            .iter()
            .all(|server| server.name != chatos_mcp_runtime::PROJECT_MANAGEMENT_SERVER_NAME));
        let executor = chatos_mcp_runtime::McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry)
            .build_builtin_only()
            .expect("builtin executor");
        assert!(executor.available_tools().into_iter().all(|tool| tool
            .get("name")
            .and_then(|name| name.as_str())
            .is_none_or(|name| !name.starts_with("project_management_service_"))));
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
            internal_api_secret: None,
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

    fn test_run_service(config: AppConfig) -> RunService {
        let (run_event_sender, _) = broadcast::channel(512);
        let store =
            crate::store::AppStore::InMemory(crate::store::InMemoryStore::new(run_event_sender));
        RunService::new(
            config,
            store.clone(),
            crate::ask_user_prompt_service::AskUserPromptService::new(store),
        )
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

    fn sample_run(task: &TaskRecord) -> TaskRunRecord {
        let now = now_rfc3339();
        TaskRunRecord {
            id: "run-1".to_string(),
            task_id: task.id.clone(),
            model_config_id: "model-1".to_string(),
            memory_thread_id: task.memory_thread_id.clone(),
            status: crate::models::TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot: json!({}),
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
