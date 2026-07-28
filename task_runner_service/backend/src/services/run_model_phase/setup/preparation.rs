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
    let command_constraints =
        crate::services::plugin_runtime_relay::plugin_command_execution_constraints(run)?;
    validate_command_target_agent(task, &command_constraints)?;
    if !command_constraints.tool_allowlists.is_empty() && !task.mcp_config.enabled {
        return Err(
            "Plugin Command allowed tools require the task MCP runtime to be enabled".to_string(),
        );
    }
    crate::services::model_runtime_resolver::ensure_cloud_task_project_execution(
        &service.config,
        task,
    )
    .await?;
    let authoritative_policy = capability_policy.is_some();
    let sandbox_required = service
        .should_route_task_to_sandbox(task, authoritative_policy)
        .await?;
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
        .prepare_sandbox_if_needed(
            task,
            run,
            effective_workspace_dir.as_str(),
            authoritative_policy,
        )
        .await?;
    let system_http_servers =
        load_system_http_mcp_servers(service, task, run, sandbox_context.as_ref())?;
    let prompt = build_task_prompt(
        task,
        input.prompt_override.as_deref(),
        input.retry_instruction.as_deref(),
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
    let agent = task_runner_agent_for_task(task);
    let agent_prompt =
        crate::services::plugin_management_prompts::resolve_task_runner_agent_prompt(
            service,
            &agent,
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
    let runtime_config =
        build_runtime_config(service, task, command_constraints.max_iterations).await?;

    let mut runtime_config = service.apply_task_mcp_config(runtime_config, &task.mcp_config);
    if !command_constraints.tool_allowlists.is_empty() {
        runtime_config.builtin_prompt_mode = chatos_ai_runtime::TaskBuiltinMcpPromptMode::Effective;
    }

    let task_service = TaskService::new(service.config.clone(), service.store.clone());
    let (mut builtin_servers, mut builtin_registry) = build_mcp_builder_parts(
        service,
        task,
        run,
        effective_workspace_dir.as_str(),
        task_process_logging_enabled,
        task_service,
        sandbox_context.as_ref(),
        authoritative_policy,
    )
    .await;
    let prepared_plugin_runtime = service
        .prepare_plugin_runtime(task, run, effective_workspace_dir.as_str())
        .await?;
    let plugin_tool_lifecycle_hook = prepared_plugin_runtime.tool_lifecycle_hook(
        crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        )
        .as_str(),
    );
    builtin_servers.extend(prepared_plugin_runtime.builtin_servers.clone());
    for provider in &prepared_plugin_runtime.providers {
        builtin_registry.register_arc(provider.clone());
    }
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
    prefixed_input_items.extend(prepared_plugin_runtime.prompt_items.clone());
    let mut run_spec = build_run_spec(
        &agent,
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
    if let Some(context) = sandbox_context.as_ref() {
        run_spec.current_input_items.insert(
            0,
            sandbox_run_fact_input_item(task.mcp_config.locale(), context.run_workspace.as_str()),
        );
    }
    let memory_scope = build_memory_scope(service, task, run);
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

    let mut mcp_builder = McpExecutorBuilder::new()
        .with_http_servers(system_http_servers)
        .with_http_servers(loaded_external_mcp.http_servers)
        .with_stdio_servers(loaded_external_mcp.stdio_servers)
        .with_builtin_servers(builtin_servers)
        .with_builtin_registry(builtin_registry);
    for allowed_tools in command_constraints.tool_allowlists {
        mcp_builder = mcp_builder.with_allowed_tool_names(allowed_tools);
    }
    if let Some(hook) = plugin_tool_lifecycle_hook {
        mcp_builder = mcp_builder.with_tool_lifecycle_hook(hook);
    }

    Ok(PreparedModelExecution {
        agent,
        run_spec,
        runtime_config,
        mcp_builder,
        tool_result_model_budget_limits,
        sandbox_context,
        harness_run_context,
        effective_workspace_dir,
        plugin_sessions: prepared_plugin_runtime.sessions,
    })
}

fn validate_command_target_agent(
    task: &TaskRecord,
    constraints: &crate::services::plugin_runtime_relay::PluginCommandExecutionConstraints,
) -> Result<(), String> {
    let task_agent_key = crate::models::task_runner_agent_key_for(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    );
    if let Some(target_agent) = constraints.target_agent.as_deref() {
        if target_agent != task_agent_key.as_str() {
            return Err(format!(
                "Plugin Command target Agent {target_agent} is incompatible with the current task Agent {}",
                task_agent_key.as_str()
            ));
        }
    }
    Ok(())
}

fn sandbox_run_fact_input_item(locale: BuiltinMcpPromptLocale, run_workspace: &str) -> Value {
    let workspace = run_workspace.trim();
    let content = if locale.is_english() {
        format!(
            "[Authoritative current-run sandbox state]\nTask Runner has already acquired and health-checked the sandbox lease for this run. TerminalController and CodeMaintainer exposed in this turn are the real execution surface of that sandbox. The current sandbox workspace is `{workspace}`.\n\nA project runtime environment status of `pending` only limits application-service topology and Project Gateway targets; it does not disable this run-scoped base sandbox or its terminal/file tools. Any historical memory claiming that `pending` forbids all terminal use is obsolete for this run. Reopen and continue child tasks that were blocked only because of that obsolete conclusion. Do not ask the user to initialize the project application environment merely to use the sandbox tools already prepared for this run."
        )
    } else {
        format!(
            "[当前运行的权威沙箱状态]\nTask Runner 已为本次运行申请并完成沙箱租约健康检查。本轮暴露的 TerminalController 与 CodeMaintainer 是该沙箱的真实执行面。当前沙箱工作区为 `{workspace}`。\n\n项目运行环境的 `pending` 仅限制应用服务拓扑与 Project Gateway 目标，不会禁用本次运行专属的基础沙箱及其终端/文件工具。历史记忆中“`pending` 禁止使用所有终端”的结论对本次运行已经过期。仅因为该旧结论而阻塞的子任务应重新打开并继续执行。不得仅为使用本轮已经准备好的沙箱工具，再次要求用户初始化项目应用环境。"
        )
    };
    json!({
        "role": "system",
        "content": content,
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
        "agent_key": agent_prompt.agent_key,
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
    agent: &TaskRunnerAgent,
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
    agent.build_run_spec(
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

fn task_runner_agent_for_task(task: &TaskRecord) -> TaskRunnerAgent {
    if crate::models::uses_task_runner_planning_agent(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    ) {
        TASK_RUNNER_PLAN_AGENT
    } else {
        TASK_RUNNER_AGENT
    }
}

fn build_memory_scope(service: &RunService, task: &TaskRecord, run: &TaskRunRecord) -> MemoryScope {
    let scope = MemoryScope::thread(
        task.tenant_id.clone(),
        service.config.memory_engine_source_id.clone(),
        task.memory_thread_id.clone(),
    )
    .with_subject_id(task.subject_id.clone());
    if run
        .input_snapshot
        .get("retry_of_run_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return scope.with_policy(ComposeContextPolicy {
            include_recent_records: Some(false),
            include_thread_summary: Some(false),
            include_subject_memory: Some(false),
            recent_record_limit: Some(1),
            summary_limit: Some(1),
        });
    }
    scope
}

async fn build_runtime_config(
    service: &RunService,
    task: &TaskRecord,
    plugin_max_iterations: Option<usize>,
) -> Result<TaskRuntimeConfig, String> {
    let configured_max_iterations = service
        .effective_task_execution_max_iterations()
        .await
        .map_err(|err| format!("加载运行时配置失败: {err}"))?;
    let max_iterations =
        bounded_plugin_max_iterations(configured_max_iterations, plugin_max_iterations);

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

fn bounded_plugin_max_iterations(configured: usize, plugin_limit: Option<usize>) -> usize {
    plugin_limit
        .map(|limit| configured.min(limit))
        .unwrap_or(configured)
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
    crate::models::uses_task_runner_planning_agent(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    )
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use chatos_agent::{AgentIdentity, TASK_RUNNER_AGENT, TASK_RUNNER_PLAN_AGENT};
    use chatos_plugin_management_sdk::SystemAgentKey;

    use crate::config::{AppConfig, StoreMode};
    use crate::models::{now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus};
    use serde_json::json;
    use tokio::sync::broadcast;

    use super::*;

    #[test]
    fn task_nature_selects_distinct_task_runner_agents() {
        let mut planning = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");
        planning.mcp_config.requires_execution = false;
        let mut executing = planning.clone();
        executing.mcp_config.requires_execution = true;

        assert_eq!(
            task_runner_agent_for_task(&planning).descriptor().key,
            TASK_RUNNER_PLAN_AGENT.descriptor().key
        );
        assert_eq!(
            task_runner_agent_for_task(&planning).descriptor().key,
            SystemAgentKey::TaskRunnerPlanPhase
        );
        assert_eq!(
            task_runner_agent_for_task(&executing).descriptor().key,
            TASK_RUNNER_AGENT.descriptor().key
        );
    }

    #[test]
    fn command_target_agent_must_match_the_existing_task_agent() {
        let mut task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");
        task.mcp_config.requires_execution = false;
        let plan_constraints =
            crate::services::plugin_runtime_relay::PluginCommandExecutionConstraints {
                target_agent: Some("task_runner_plan_phase".to_string()),
                tool_allowlists: Vec::new(),
                ..Default::default()
            };
        validate_command_target_agent(&task, &plan_constraints).expect("matching target Agent");

        let run_constraints =
            crate::services::plugin_runtime_relay::PluginCommandExecutionConstraints {
                target_agent: Some("task_runner_run_phase".to_string()),
                tool_allowlists: Vec::new(),
                ..Default::default()
            };
        assert!(validate_command_target_agent(&task, &run_constraints)
            .expect_err("Command must not upgrade the task Agent")
            .contains("incompatible"));
    }

    #[test]
    fn plugin_agent_iteration_limit_can_only_narrow_the_runtime() {
        assert_eq!(bounded_plugin_max_iterations(600, Some(12)), 12);
        assert_eq!(bounded_plugin_max_iterations(8, Some(12)), 8);
        assert_eq!(bounded_plugin_max_iterations(600, None), 600);
    }

    #[tokio::test]
    async fn chatos_plan_builtin_servers_include_project_management_provider() {
        let config = test_config();
        let service = test_run_service(config);
        let mut task = sample_task(crate::models::TASK_PROFILE_CHATOS_PLAN, "project-1");
        task.mcp_config.requires_execution = false;
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

    #[test]
    fn sandbox_run_fact_overrides_stale_project_environment_memory() {
        let item = sandbox_run_fact_input_item(BuiltinMcpPromptLocale::ZhCn, "/workspace");
        let content = item["content"].as_str().expect("system content");

        assert_eq!(item["role"].as_str(), Some("system"));
        assert!(content.contains("已为本次运行申请并完成沙箱租约健康检查"));
        assert!(content.contains("`pending` 仅限制应用服务拓扑"));
        assert!(content.contains("历史记忆"));
        assert!(content.contains("重新打开并继续执行"));
        assert!(content.contains("`/workspace`"));
    }

    #[test]
    fn retry_run_uses_clean_memory_context_instead_of_failed_attempt_history() {
        let service = test_run_service(test_config());
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let mut run = sample_run(&task);
        run.input_snapshot = json!({ "retry_of_run_id": "run-old" });

        let policy = build_memory_scope(&service, &task, &run)
            .policy
            .expect("retry memory policy");

        assert_eq!(policy.include_recent_records, Some(false));
        assert_eq!(policy.include_thread_summary, Some(false));
        assert_eq!(policy.include_subject_memory, Some(false));
    }

    #[test]
    fn initial_run_keeps_default_memory_context_policy() {
        let service = test_run_service(test_config());
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let run = sample_run(&task);

        assert!(build_memory_scope(&service, &task, &run).policy.is_none());
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
            plugin_config: Default::default(),
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
            plugin_snapshots: Vec::new(),
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            dispatch_paused: false,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
