// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

mod mcp_builder;
mod mcp_inputs;

use mcp_builder::build_mcp_builder_parts;
use mcp_inputs::{
    external_mcp_prefixed_input_items, load_external_mcp_servers, load_system_http_mcp_servers,
    mcp_provider_skills_prefixed_input_items,
};

pub(super) async fn prepare_model_execution(
    service: &RunService,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
    run: &mut TaskRunRecord,
    input: &StartTaskRunRequest,
    effective_workspace_dir: &str,
    prerequisite_context: &[PrerequisiteTaskContext],
    capability_policy: Option<&TaskRunnerCapabilityPolicy>,
) -> Result<PreparedModelExecution, String> {
    crate::services::model_runtime_resolver::ensure_cloud_task_project_execution(
        &service.config,
        task,
    )
    .await?;
    let sandbox_required = service.should_route_task_to_sandbox(task).await?;
    let harness_run_context = if sandbox_required {
        service
            .prepare_harness_run_for_sandbox(task, run, effective_workspace_dir)
            .await
    } else {
        None
    };
    let effective_workspace_dir = harness_run_context
        .as_ref()
        .map(|context| context.effective_workspace_dir.as_str())
        .unwrap_or(effective_workspace_dir)
        .to_string();
    let loaded_external_mcp = load_external_mcp_servers(
        service,
        task,
        effective_workspace_dir.as_str(),
        capability_policy,
    )
    .await?;
    let sandbox_context = service
        .prepare_sandbox_if_needed(task, run, effective_workspace_dir.as_str())
        .await?;
    let system_http_servers =
        load_system_http_mcp_servers(service, task, run, sandbox_context.as_ref())?;
    let prompt = build_task_prompt(
        task,
        input.prompt_override.as_deref(),
        prerequisite_context,
        task.mcp_config.locale(),
    );
    let resolved_model_config =
        crate::services::model_runtime_resolver::resolve_model_runtime_for_task(
            &service.config,
            task,
            model_config,
        )
        .await?;
    let agent_prompt =
        crate::services::plugin_management_prompts::resolve_task_runner_agent_prompt(
            service,
            resolved_model_config.prompt_vendor.as_deref(),
            resolved_model_config.provider.as_str(),
        )
        .await?;
    let metadata = build_execution_metadata(
        task,
        run,
        model_config,
        &agent_prompt,
        sandbox_context.as_ref(),
    );
    let task_process_logging_enabled = task_process_logging_enabled(&task.mcp_config);
    let tool_result_model_budget_limits = service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(|err| format!("加载运行时配置失败: {err}"))?;
    let runtime_config = build_runtime_config(service, task).await?;

    let runtime_config = service.apply_task_mcp_config(runtime_config, &task.mcp_config);

    let task_service = TaskService::new(service.config.clone(), service.store.clone());
    let (builtin_servers, builtin_registry) = build_mcp_builder_parts(
        service,
        task,
        run,
        effective_workspace_dir.as_str(),
        task_process_logging_enabled,
        task_service,
        sandbox_context.as_ref(),
        capability_policy.is_some(),
    )
    .await;
    let mut prefixed_input_items = external_mcp_prefixed_input_items(
        loaded_external_mcp.summaries.as_slice(),
        task.mcp_config.locale(),
    );
    let provider_skills_prompt = capability_policy.and_then(|policy| {
        let locale = if task.mcp_config.locale().is_english() {
            "en-US"
        } else {
            "zh-CN"
        };
        policy.compose_provider_skills_prompt(
            loaded_external_mcp
                .summaries
                .iter()
                .map(|summary| summary.id.as_str()),
            locale,
        )
    });
    prefixed_input_items.extend(mcp_provider_skills_prefixed_input_items(
        provider_skills_prompt,
    ));
    let mut run_spec = build_run_spec(
        task,
        run,
        &resolved_model_config,
        model_config,
        effective_workspace_dir.as_str(),
        prompt,
        agent_prompt.content,
        metadata,
        task_process_logging_enabled,
        prefixed_input_items,
    );
    let memory_scope = build_memory_scope(service, task);
    run_spec = run_spec.with_memory_scope(Some(memory_scope));
    persist_context_snapshot(service, run, run_spec.memory_scope.as_ref()).await;
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
        sandbox_context,
        harness_run_context,
        effective_workspace_dir,
    })
}

fn build_execution_metadata(
    task: &TaskRecord,
    run: &TaskRunRecord,
    model_config: &ModelConfigRecord,
    agent_prompt: &chatos_plugin_management_sdk::ResolvedAgentPrompt,
    sandbox_context: Option<&crate::services::sandbox_runtime::SandboxRuntimeContext>,
) -> serde_json::Value {
    let mut metadata = json!({
        "task_id": task.id,
        "run_id": run.id,
        "model_config_id": model_config.id,
        "service": "task_runner_service",
        "agent_prompt_vendor": agent_prompt.vendor.as_str(),
        "agent_prompt_revision": agent_prompt.revision,
        "agent_prompt_checksum": agent_prompt.checksum,
    });
    if let Some(context) = sandbox_context {
        if let Some(object) = metadata.as_object_mut() {
            object.insert("sandbox_enabled".to_string(), json!(true));
            object.insert("sandbox".to_string(), context.to_metadata());
        }
    }
    metadata
}

fn build_run_spec(
    task: &TaskRecord,
    run: &TaskRunRecord,
    runtime_model_config: &ModelConfigRecord,
    metadata_model_config: &ModelConfigRecord,
    _effective_workspace_dir: &str,
    prompt: String,
    agent_system_prompt: String,
    metadata: serde_json::Value,
    task_process_logging_enabled: bool,
    external_mcp_prefixed_input_items: Vec<Value>,
) -> TaskRunSpec {
    let mut effective_model_config = runtime_model_config.clone();
    effective_model_config.request_cwd = None;
    let mut model_runtime_config = effective_model_config.to_runtime_config(None);
    model_runtime_config.instructions = Some(
        match model_runtime_config
            .instructions
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(existing) => format!("{}\n\n{existing}", agent_system_prompt.trim()),
            None => agent_system_prompt,
        },
    );

    let mut prefixed_input_items = external_mcp_prefixed_input_items;
    if task_process_logging_enabled {
        prefixed_input_items.extend(task_process_log_prefixed_input_items(
            task.mcp_config.locale(),
        ));
    }
    TASK_RUNNER_AGENT.build_run_spec(
        TaskRunnerRunSpecInput::new(
            task.id.clone(),
            run.id.clone(),
            model_runtime_config,
            metadata_model_config.id.clone(),
            prompt,
            metadata,
        )
        .with_prefixed_input_items(prefixed_input_items),
    )
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
            .with_internal_service_auth(
                "task-runner",
                service.config.memory_engine_operator_token.clone(),
            )
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
    use serde_json::json;
    use tokio::sync::broadcast;

    use super::*;
    #[tokio::test]
    async fn chatos_plan_builtin_servers_include_project_management_provider() {
        let config = test_config();
        let service = test_run_service(config);
        let task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");
        let run = sample_run(&task);
        let task_service = TaskService::new(service.config.clone(), service.store.clone());

        let (builtin_servers, builtin_registry) =
            build_mcp_builder_parts(&service, &task, &run, ".", false, task_service, None, false)
                .await;
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

        let system_servers =
            load_system_http_mcp_servers(&service, &task, &run, None).expect("system servers");
        assert!(system_servers.is_empty());

        let (builtin_servers, builtin_registry) =
            build_mcp_builder_parts(&service, &task, &run, ".", false, task_service, None, false)
                .await;
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
            role: crate::config::TaskRunnerRole::All,
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
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 1_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            local_connector_internal_api_secret: None,
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
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
