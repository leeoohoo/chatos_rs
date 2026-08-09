// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

mod mcp_inputs;
mod mcp_management_gateway;

use mcp_inputs::mcp_provider_skills_prefixed_input_items;
use mcp_management_gateway::resolve_mcp_management_gateway;

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
    service.ensure_run_thread(task, run).await?;
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
            .await?
    } else {
        None
    };
    let effective_workspace_dir = harness_run_context
        .as_ref()
        .map(|context| context.effective_workspace_dir.as_str())
        .unwrap_or(effective_workspace_dir)
        .to_string();
    let sandbox_context = service
        .prepare_sandbox_if_needed(
            task,
            run,
            effective_workspace_dir.as_str(),
            authoritative_policy,
        )
        .await?;
    if let Some(context) = sandbox_context.as_ref() {
        run.bind_current_attempt_environment(&context.sandbox_id, &context.lease_id);
        run.updated_at = now_rfc3339();
        *run = service
            .store
            .save_run(run.clone())
            .await
            .map_err(|err| format!("保存 Run Attempt 沙箱归属失败: {err}"))?;
    }
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
    let task_process_logging_enabled = match capability_policy {
        Some(policy) => {
            task_process_logging_enabled(&task.mcp_config) && policy.task_process_log_mcp_enabled()
        }
        None => task_process_logging_enabled(&task.mcp_config),
    };
    let tool_result_model_budget_limits = service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(|err| format!("加载运行时配置失败: {err}"))?;
    let prompt_cache_policy = service
        .effective_prompt_cache_policy()
        .await
        .map_err(|err| format!("加载模型缓存配置失败: {err}"))?;
    let runtime_config =
        build_runtime_config(service, task, run, command_constraints.max_iterations).await?;

    let mut runtime_config = service.apply_task_mcp_config(runtime_config, &task.mcp_config);
    if !command_constraints.tool_allowlists.is_empty() {
        runtime_config.builtin_prompt_mode = chatos_ai_runtime::TaskBuiltinMcpPromptMode::Effective;
    }

    let prepared_plugin_runtime = service
        .prepare_plugin_runtime(task, run, effective_workspace_dir.as_str())
        .await?;
    let mcp_management_gateway =
        resolve_mcp_management_gateway(task, run, sandbox_context.as_ref()).await?;
    let gateway_provider_skills_prompt = mcp_management_gateway.provider_skills_prompt.clone();
    let plugin_tool_lifecycle_hook = prepared_plugin_runtime.tool_lifecycle_hook(
        crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        )
        .as_str(),
    );
    let mut prefixed_input_items =
        mcp_provider_skills_prefixed_input_items(gateway_provider_skills_prompt);
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
        prompt_cache_policy,
    );
    if sandbox_context.is_some() {
        run_spec
            .current_input_items
            .insert(0, sandbox_run_fact_input_item(task.mcp_config.locale()));
    }
    let memory_scope = build_memory_scope(service, task, run);
    run_spec = run_spec.with_memory_scope(Some(memory_scope));
    persist_context_snapshot(service, run, run_spec.memory_scope.as_ref()).await;
    let (mcp_management_server, mcp_management_runtime_session) =
        mcp_management_gateway.into_parts();
    let mut mcp_builder = McpExecutorBuilder::new().with_http_server(mcp_management_server);
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
        mcp_management_runtime_session,
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

fn sandbox_run_fact_input_item(locale: BuiltinMcpPromptLocale) -> Value {
    let content = if locale.is_english() {
        "[Current project workspace]\nThe project file and terminal tools exposed in this run all operate on the same current project workspace. Use project-root-relative paths and use the tools directly."
            .to_string()
    } else {
        "[当前项目工作区]\n本轮提供的项目文件与项目终端工具都作用于同一个当前项目工作区。路径使用项目根目录相对路径，直接使用工具即可。"
            .to_string()
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
    prompt_cache_policy: crate::services::run_service::TaskRunnerPromptCachePolicy,
) -> TaskRunSpec {
    let mut effective_model_config = runtime_model_config.clone();
    effective_model_config.request_cwd = None;
    let mut model_runtime_config = effective_model_config.to_runtime_config(None);
    apply_prompt_cache_policy(
        &mut model_runtime_config,
        run.id.as_str(),
        prompt_cache_policy,
    );
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

fn apply_prompt_cache_policy(
    model_runtime_config: &mut chatos_ai_runtime::ModelRuntimeConfig,
    run_id: &str,
    policy: crate::services::run_service::TaskRunnerPromptCachePolicy,
) {
    if policy.enabled {
        model_runtime_config.prompt_cache_key = Some(format!("task-runner:{run_id}"));
        model_runtime_config.include_prompt_cache_retention = policy.retention_enabled;
    } else {
        model_runtime_config.prompt_cache_key = None;
        model_runtime_config.include_prompt_cache_retention = false;
    }
}

#[cfg(test)]
mod prompt_cache_tests {
    use super::apply_prompt_cache_policy;
    use crate::services::run_service::TaskRunnerPromptCachePolicy;

    fn runtime_config() -> chatos_ai_runtime::ModelRuntimeConfig {
        chatos_ai_runtime::ModelRuntimeConfig::openai_compatible(
            "https://api.openai.com/v1",
            "secret",
            "gpt-test",
            "openai",
        )
    }

    #[test]
    fn same_run_uses_stable_cache_key_and_different_runs_are_isolated() {
        let policy = TaskRunnerPromptCachePolicy {
            enabled: true,
            retention_enabled: true,
        };
        let mut first = runtime_config();
        let mut repeated = runtime_config();
        let mut different = runtime_config();

        apply_prompt_cache_policy(&mut first, "run-1", policy);
        apply_prompt_cache_policy(&mut repeated, "run-1", policy);
        apply_prompt_cache_policy(&mut different, "run-2", policy);

        assert_eq!(first.prompt_cache_key, repeated.prompt_cache_key);
        assert_ne!(first.prompt_cache_key, different.prompt_cache_key);
        assert_eq!(first.prompt_cache_key.as_deref(), Some("task-runner:run-1"));
        assert!(first.include_prompt_cache_retention);
    }

    #[test]
    fn managed_policy_can_disable_all_cache_options_without_model_fallback() {
        let mut config = runtime_config()
            .with_prompt_cache_key(Some("model-default".to_string()))
            .with_prompt_cache_retention(true);

        apply_prompt_cache_policy(
            &mut config,
            "run-1",
            TaskRunnerPromptCachePolicy {
                enabled: false,
                retention_enabled: true,
            },
        );

        assert_eq!(config.prompt_cache_key, None);
        assert!(!config.include_prompt_cache_retention);
    }
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
    MemoryScope::thread(
        task.tenant_id.clone(),
        service.config.memory_engine_source_id.clone(),
        run.memory_thread_id.clone(),
    )
    .with_policy(ComposeContextPolicy {
        include_recent_records: Some(true),
        include_thread_summary: Some(true),
        include_subject_memory: Some(false),
        recent_record_limit: None,
        summary_limit: None,
    })
}

async fn build_runtime_config(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
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
            .with_record_scope(Some(build_memory_record_scope(task, run))),
        ));
    }

    Ok(runtime_config)
}

fn build_memory_record_scope(task: &TaskRecord, run: &TaskRunRecord) -> MemoryRecordScope {
    MemoryRecordScope::message_thread(task.tenant_id.clone(), run.memory_thread_id.clone())
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
                target_agent: Some(SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string()),
                tool_allowlists: Vec::new(),
                ..Default::default()
            };
        validate_command_target_agent(&task, &plan_constraints).expect("matching target Agent");

        let run_constraints =
            crate::services::plugin_runtime_relay::PluginCommandExecutionConstraints {
                target_agent: Some(SystemAgentKey::TaskRunnerRunPhase.as_str().to_string()),
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

    #[test]
    fn sandbox_run_fact_only_describes_the_program_bound_workspace() {
        let item = sandbox_run_fact_input_item(BuiltinMcpPromptLocale::ZhCn);
        let content = item["content"].as_str().expect("system content");

        assert_eq!(item["role"].as_str(), Some("system"));
        assert!(content.contains("都作用于同一个当前项目工作区"));
        assert!(content.contains("项目根目录相对路径"));
        assert!(!content.contains("pending"));
        assert!(!content.contains("初始化"));
        assert!(!content.contains("Provider"));
        assert!(!content.contains("租约"));
        assert!(!content.contains("/workspace"));
        assert!(!content.contains("pairing"));
    }

    #[test]
    fn run_memory_scope_keeps_current_run_history_without_subject_recall() {
        let service = test_run_service(test_config());
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let mut run = sample_run(&task);
        run.input_snapshot = json!({ "retry_of_run_id": "run-old" });

        let scope = build_memory_scope(&service, &task, &run);
        let policy = scope.policy.expect("run memory policy");

        assert_eq!(scope.thread_id, run.memory_thread_id);
        assert_eq!(policy.include_recent_records, Some(true));
        assert_eq!(policy.include_thread_summary, Some(true));
        assert_eq!(policy.include_subject_memory, Some(false));
    }

    #[test]
    fn compose_and_record_scopes_use_the_same_run_thread() {
        let service = test_run_service(test_config());
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let run = sample_run(&task);
        let compose_scope = build_memory_scope(&service, &task, &run);
        let record_scope = build_memory_record_scope(&task, &run);

        assert_eq!(compose_scope.tenant_id, record_scope.tenant_id);
        assert_eq!(Some(compose_scope.thread_id), record_scope.thread_id);
    }

    #[test]
    fn different_runs_do_not_share_memory_threads() {
        let service = test_run_service(test_config());
        let task = sample_task(crate::models::TASK_PROFILE_DEFAULT, "project-1");
        let mut first = sample_run(&task);
        first.id = "run-1".to_string();
        first.memory_thread_id = crate::models::task_run_memory_thread_id(
            task.memory_thread_id.as_str(),
            first.id.as_str(),
        );
        let mut second = sample_run(&task);
        second.id = "run-2".to_string();
        second.memory_thread_id = crate::models::task_run_memory_thread_id(
            task.memory_thread_id.as_str(),
            second.id.as_str(),
        );

        assert_ne!(
            build_memory_scope(&service, &task, &first).thread_id,
            build_memory_scope(&service, &task, &second).thread_id
        );
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://plan-runtime-preparation-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            memory_engine_http_client: reqwest::Client::new(),
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(30_000),
            execution_timeout: Duration::from_millis(30_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            worker_id: "test-worker".to_string(),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 1_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            local_connector_internal_api_secret: None,
            local_connector_service_base_url: Some("http://127.0.0.1:39230".to_string()),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_millis(5_000),
            plugin_relay_request_timeout: Duration::from_millis(60_000),
            plugin_hook_relay_timeout: Duration::from_millis(330_000),
            plugin_connector_discovery_timeout: Duration::from_millis(10_000),
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            project_service_base_url: Some("http://127.0.0.1:39210".to_string()),
            project_service_internal_base_url: Some("http://127.0.0.1:39210".to_string()),
            project_service_internal_http_client: reqwest::Client::new(),
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
            execution_lane_key: None,
            model_config_id: "model-1".to_string(),
            memory_thread_id: crate::models::task_run_memory_thread_id(
                task.memory_thread_id.as_str(),
                "run-1",
            ),
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
            cancel_event_pending: false,
            dispatch_paused: false,
            dispatch_event_pending: false,
            post_process_event_pending: false,
            post_process_event_enqueued: false,
            post_process_completed: false,
            post_process_dead_lettered: false,
            post_process_attempt_count: 0,
            post_process_last_error: None,
            memory_summary_processed: false,
            chatos_followup_processed: false,
            terminal_cleanup_event_pending: false,
            terminal_cleanup_event_enqueued: false,
            terminal_cleanup_completed: false,
            terminal_cleanup_attempt_count: 0,
            terminal_cleanup_last_error: None,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            attempts: Vec::new(),
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
