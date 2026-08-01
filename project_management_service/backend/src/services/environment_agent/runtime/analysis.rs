// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

pub(in crate::services::environment_agent) async fn analyze_project_runtime_environment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment =
        ensure_runtime_environment_for_project(&state.store, project, None).await?;
    let run_id = run_id.to_string();

    if !environment.sandbox_enabled {
        environment.status = ProjectRuntimeEnvironmentStatus::Disabled;
        environment.sandbox_provider = RuntimeEnvironmentProvider::None;
        environment.file_provider = RuntimeEnvironmentProvider::None;
        environment.analysis_summary =
            Some("该项目已关闭沙箱环境初始化，不会自动分析或创建运行环境镜像。".to_string());
        environment.not_runnable_reason = None;
        environment.last_agent_run_id = Some(run_id);
        environment.last_error = None;
        environment.updated_at = now_rfc3339();
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), &[])
            .await?;
        return response_for_project(state, environment).await;
    }

    let owner_user_id = project
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_user_id) = owner_user_id else {
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.analysis_summary =
            Some("无法运行项目管理 Agent：项目缺少 owner_user_id。".to_string());
        environment.last_error = Some("project owner_user_id is required".to_string());
        environment.updated_at = now_rfc3339();
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        return response_for_project(state, environment).await;
    };

    let environment_plan =
        match resolve_runtime_environment_plan(project, &state.config, user_access_token).await {
            RuntimeEnvironmentDecision::Stop(stop) => {
                apply_stop_decision(&mut environment, run_id, stop);
                let environment = state
                    .store
                    .upsert_project_runtime_environment(&environment)
                    .await?;
                if matches!(
                    environment.status,
                    ProjectRuntimeEnvironmentStatus::NotRunnable
                        | ProjectRuntimeEnvironmentStatus::Disabled
                        | ProjectRuntimeEnvironmentStatus::Failed
                ) {
                    state
                        .store
                        .replace_project_runtime_environment_images(project.id.as_str(), &[])
                        .await?;
                }
                return response_for_project(state, environment).await;
            }
            RuntimeEnvironmentDecision::Ready(plan) => plan,
        };

    environment.status = ProjectRuntimeEnvironmentStatus::Analyzing;
    environment.file_provider = environment_plan.file_provider;
    environment.sandbox_provider = environment_plan.sandbox_provider;
    environment.analysis_summary = Some("正在分析项目技术栈和运行环境需求。".to_string());
    environment.not_runnable_reason = None;
    environment.last_agent_run_id = Some(run_id.clone());
    environment.last_error = None;
    bind_selected_dependencies(&mut environment.detected_stack, selected_dependencies);
    environment.updated_at = now_rfc3339();
    environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;

    let model_runtime = match resolve_default_environment_initialization_model_runtime(
        &state.config,
        owner_user_id,
    )
    .await
    {
        Ok(Some(runtime)) => runtime,
        Ok(None) => {
            environment.status = ProjectRuntimeEnvironmentStatus::PendingConfiguration;
            environment.analysis_summary = Some(
                "项目可进入运行环境分析，但还没有配置“环境初始化模型”。请先在用户菜单中配置默认模型。"
                    .to_string(),
            );
            environment.last_error = None;
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("读取环境初始化模型配置失败。".to_string());
            environment.last_error = Some(err);
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
    };

    let local_inspection = inspect_local_project(project);
    let memory = match build_project_agent_memory(
        &state.config,
        owner_user_id,
        project.id.as_str(),
        user_access_token,
    )
    .await
    {
        Ok(memory) => memory,
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary =
                Some("项目管理 Agent Memory Engine 初始化失败。".to_string());
            environment.last_error = Some(err);
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
    };
    let agent_result = run_project_environment_agent(
        state,
        project,
        environment_plan,
        model_runtime.prompt_vendor.as_deref(),
        &model_runtime.model_config,
        local_inspection.as_ref(),
        &memory,
        user_access_token,
        &ProjectEnvironmentAgentRunContext {
            run_id: run_id.as_str(),
            owner_user_id,
            model_config_id: model_runtime.model_config_id.as_str(),
            analysis_requirement,
            selected_dependencies,
        },
    )
    .await;

    match agent_result {
        Ok(()) => {
            let Some(environment) = state
                .store
                .get_project_runtime_environment(project.id.as_str())
                .await?
            else {
                return Err(
                    "project environment agent did not persist runtime environment".to_string(),
                );
            };
            if environment.status == ProjectRuntimeEnvironmentStatus::Analyzing {
                let mut failed = environment;
                failed.status = ProjectRuntimeEnvironmentStatus::Failed;
                failed.analysis_summary =
                    Some("项目管理 Agent 已执行，但没有写入运行环境初始化结果。".to_string());
                failed.last_error = Some(
                    "agent did not call update_current_project_runtime_environment".to_string(),
                );
                failed.updated_at = now_rfc3339();
                let failed = state
                    .store
                    .upsert_project_runtime_environment(&failed)
                    .await?;
                return response_for_project(state, failed).await;
            }
            response_for_project(state, environment).await
        }
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("项目管理 Agent 初始化运行环境失败。".to_string());
            environment.last_error = Some(err.clone());
            environment.updated_at = now_rfc3339();
            tracing::warn!(
                project_id = project.id.as_str(),
                model_config_id = model_runtime.model_config_id.as_str(),
                model = model_runtime.model_config.model.as_str(),
                error = err.as_str(),
                "project environment agent failed"
            );
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            response_for_project(state, environment).await
        }
    }
}

struct ProjectEnvironmentAgentRunContext<'a> {
    run_id: &'a str,
    owner_user_id: &'a str,
    model_config_id: &'a str,
    analysis_requirement: Option<&'a str>,
    selected_dependencies: &'a [String],
}

async fn response_for_project(
    state: &AppState,
    environment: ProjectRuntimeEnvironmentRecord,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let images = state
        .store
        .list_project_runtime_environment_images(environment.project_id.as_str())
        .await?;
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

async fn run_project_environment_agent(
    state: &AppState,
    project: &ProjectRecord,
    environment_plan: RuntimeEnvironmentPlan,
    prompt_vendor: Option<&str>,
    model_config: &ModelRuntimeConfig,
    local_inspection: Option<&LocalProjectInspection>,
    memory: &ProjectAgentMemory,
    user_access_token: Option<&str>,
    run_context: &ProjectEnvironmentAgentRunContext<'_>,
) -> Result<(), String> {
    let agent_prompt = resolve_project_environment_agent_prompt(
        state,
        prompt_vendor,
        model_config.provider.as_str(),
    )
    .await?;
    let mcp_resolution = resolve_project_environment_mcp(
        project,
        run_context.owner_user_id,
        run_context.run_id,
        run_context.model_config_id,
    )
    .await?;
    match mcp_resolution {
        ProjectEnvironmentMcpResolution::Legacy => {
            let capability_policy = resolve_legacy_project_agent_capabilities(
                state,
                run_context.owner_user_id,
                user_access_token,
            )
            .await?;
            let executor = build_legacy_project_environment_mcp_executor(
                state,
                project,
                &environment_plan,
                user_access_token,
                run_context.run_id,
                &capability_policy,
                run_context.selected_dependencies,
            )
            .await?;
            ensure_agent_required_tools_available(&executor, &environment_plan)?;
            let provider_skills_prompt =
                compose_legacy_provider_skills_prompt(&capability_policy, &environment_plan);
            execute_project_environment_agent(
                project,
                model_config,
                local_inspection,
                memory,
                run_context.run_id,
                run_context.analysis_requirement,
                run_context.selected_dependencies,
                agent_prompt,
                executor,
                provider_skills_prompt,
            )
            .await
        }
        ProjectEnvironmentMcpResolution::Gateway(gateway) => {
            let provider_skills_prompt = gateway.provider_skills_prompt();
            let result = async {
                let executor = McpExecutor::builder()
                    .with_http_server(gateway.server().clone())
                    .build_initialized()
                    .await?;
                ensure_agent_required_tools_available(&executor, &environment_plan)?;
                execute_project_environment_agent(
                    project,
                    model_config,
                    local_inspection,
                    memory,
                    run_context.run_id,
                    run_context.analysis_requirement,
                    run_context.selected_dependencies,
                    agent_prompt,
                    executor,
                    provider_skills_prompt,
                )
                .await
            }
            .await;
            gateway.close(project.id.as_str(), run_context.run_id).await;
            result
        }
    }
}

async fn execute_project_environment_agent(
    project: &ProjectRecord,
    model_config: &ModelRuntimeConfig,
    local_inspection: Option<&LocalProjectInspection>,
    memory: &ProjectAgentMemory,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
    agent_prompt: chatos_plugin_management_sdk::ResolvedAgentPrompt,
    executor: McpExecutor,
    provider_skills_prompt: Option<String>,
) -> Result<(), String> {
    let mut prompt = build_project_environment_agent_prompt(
        project,
        local_inspection,
        run_id,
        analysis_requirement,
        selected_dependencies,
    )?;
    if let Some(provider_skills_prompt) = provider_skills_prompt {
        prompt.push_str("\n\n");
        prompt.push_str(provider_skills_prompt.trim());
    }
    let metadata = json!({
        "agent": "project_management_environment_agent",
        "run_id": run_id,
        "project_id": project.id,
        "agent_prompt_vendor": agent_prompt.vendor.as_str(),
        "agent_prompt_revision": agent_prompt.revision,
        "agent_prompt_checksum": agent_prompt.checksum,
    });
    let agent_memory = AgentTurnMemory::new(
        memory.composer.clone(),
        memory.writer.clone(),
        memory.scope.clone(),
        memory.conversation_id.clone(),
    );
    let request = AgentTurnRequest::new(
        model_config.clone(),
        memory.conversation_id.clone(),
        run_id,
        prompt,
    )
    .with_system_prompt(agent_prompt.content)
    .with_mcp_executor(executor)
    .with_memory(Some(agent_memory))
    .with_max_iterations(chatos_agent::load_agent_max_iterations("project-service").await)
    .with_metadata(metadata);
    let result = AgentExecutor::new()
        .run(&PROJECT_ENVIRONMENT_AGENT, request)
        .await
        .map_err(|error| error.message().to_string())?;
    tracing::info!(
        project_id = project.id.as_str(),
        run_id,
        finish_reason = result.finish_reason.as_deref().unwrap_or(""),
        "project environment agent completed"
    );
    Ok(())
}

fn bind_selected_dependencies(detected_stack: &mut Value, selected_dependencies: &[String]) {
    if !detected_stack.is_object() {
        *detected_stack = json!({});
    }
    let Some(object) = detected_stack.as_object_mut() else {
        return;
    };
    let mut selected_dependencies = selected_dependencies
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    selected_dependencies.sort();
    selected_dependencies.dedup();
    object.insert(
        "selected_dependencies".to_string(),
        json!(selected_dependencies),
    );
}

async fn resolve_legacy_project_agent_capabilities(
    state: &AppState,
    owner_user_id: &str,
    user_access_token: Option<&str>,
) -> Result<ResolvedAgentCapabilities, String> {
    let request =
        ResolveAgentCapabilitiesRequest::new(SystemAgentKey::ProjectManagementAgent, owner_user_id)
            .with_runtime_context(None, None, Some("cloud".to_string()), None);
    let capabilities = if let Some(access_token) = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state
            .plugin_management_client
            .resolve_for_user(&request, access_token)
            .await
            .map_err(|err| err.to_string())?
    } else {
        state
            .plugin_management_client
            .resolve_for_service(&request)
            .await
            .map_err(|err| err.to_string())?
    };
    capabilities
        .ensure_required_runtime_supported([], [])
        .map_err(|err| err.to_string())?;
    let code_read_resource_id = BuiltinMcpKind::CodeMaintainerRead
        .config_id()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "system_builtin_code_maintainer_read".to_string());
    for resource_id in [
        code_read_resource_id.as_str(),
        PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
        SANDBOX_IMAGES_MCP_RESOURCE_ID,
    ] {
        capabilities
            .require_available_mcp(resource_id)
            .map_err(|err| err.to_string())?;
    }
    Ok(capabilities)
}

fn build_project_environment_agent_prompt(
    project: &ProjectRecord,
    local_inspection: Option<&LocalProjectInspection>,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) -> Result<String, String> {
    let context = project_environment_agent_context(
        project.id.as_str(),
        project.name.as_str(),
        local_inspection,
        run_id,
        analysis_requirement,
        selected_dependencies,
    );
    serde_json::to_string_pretty(&context)
        .map_err(|err| format!("serialize project environment run context failed: {err}"))
}

fn project_environment_agent_context(
    project_id: &str,
    project_name: &str,
    local_inspection: Option<&LocalProjectInspection>,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) -> Value {
    json!({
        "mode": "cloud_tool_execution",
        "run_id": run_id,
        "project": {
            "id": project_id,
            "name": project_name,
        },
        "analysis_request": {
            "user_requirement": analysis_requirement
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            "selected_dependencies": selected_dependencies,
        },
        "pre_scan": {
            "detected_stack": local_inspection
                .map(|inspection| inspection.detected_stack.clone())
                .unwrap_or_else(empty_object),
            "required_services": local_inspection
                .map(|inspection| inspection.required_services.clone())
                .unwrap_or_else(empty_array),
            "manifest_context": local_inspection
                .map(|inspection| inspection.manifest_context.clone())
                .unwrap_or_default(),
        },
    })
}

fn effective_project_environment_mcp_resource_ids(plan: &RuntimeEnvironmentPlan) -> Vec<String> {
    let mut resource_ids = vec![
        PROJECT_ENVIRONMENT_MCP_RESOURCE_ID.to_string(),
        SANDBOX_IMAGES_MCP_RESOURCE_ID.to_string(),
    ];
    if matches!(
        plan.file_provider,
        RuntimeEnvironmentProvider::Harness | RuntimeEnvironmentProvider::LocalConnector
    ) {
        resource_ids.push(
            BuiltinMcpKind::CodeMaintainerRead
                .config_id()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "system_builtin_code_maintainer_read".to_string()),
        );
    }
    resource_ids
}

fn compose_legacy_provider_skills_prompt(
    capability_policy: &ResolvedAgentCapabilities,
    plan: &RuntimeEnvironmentPlan,
) -> Option<String> {
    let effective_mcp_resource_ids = effective_project_environment_mcp_resource_ids(plan);
    capability_policy.compose_provider_skills_prompt(
        effective_mcp_resource_ids.iter().map(String::as_str),
        Some("zh-CN"),
    )
}

fn apply_stop_decision(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    run_id: String,
    stop: StopDecision,
) {
    environment.status = stop.status;
    environment.sandbox_provider = RuntimeEnvironmentProvider::None;
    environment.file_provider = RuntimeEnvironmentProvider::None;
    environment.analysis_summary = Some(stop.summary);
    environment.not_runnable_reason = stop.not_runnable_reason;
    environment.execution_service_id = None;
    environment.last_agent_run_id = Some(run_id);
    environment.last_error = stop.last_error;
    environment.updated_at = now_rfc3339();
}

#[cfg(test)]
mod tests {
    use super::{bind_selected_dependencies, project_environment_agent_context};

    #[test]
    fn agent_context_does_not_expose_program_routing_or_environment_state() {
        let context = project_environment_agent_context(
            "project-1",
            "Example",
            None,
            "run-1",
            Some("Use Node.js 22 and expose port 3000"),
            &["PostgreSQL".to_string(), "Redis".to_string()],
        );
        assert!(context.get("routing").is_none());
        assert!(context.get("current_environment").is_none());
        assert_eq!(
            context["analysis_request"]["user_requirement"],
            "Use Node.js 22 and expose port 3000"
        );
        assert_eq!(
            context["analysis_request"]["selected_dependencies"],
            serde_json::json!(["PostgreSQL", "Redis"])
        );
        let serialized = serde_json::to_string(&context).expect("serialize context");
        for forbidden in [
            "file_provider",
            "sandbox_provider",
            "Harness",
            "Sandbox Manager",
            "analysis_summary",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn selected_dependencies_are_bound_to_the_persisted_analysis_run() {
        let mut detected_stack = serde_json::json!({"project_type": "rust"});
        bind_selected_dependencies(
            &mut detected_stack,
            &[
                " Redis ".to_string(),
                "PostgreSQL".to_string(),
                "Redis".to_string(),
            ],
        );
        assert_eq!(
            detected_stack["selected_dependencies"],
            serde_json::json!(["PostgreSQL", "Redis"])
        );
    }
}
