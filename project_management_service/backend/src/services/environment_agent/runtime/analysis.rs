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
    bind_analysis_request(
        &mut environment.detected_stack,
        analysis_requirement,
        selected_dependencies,
    );
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
        &memory,
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
            let mut response = response_for_project(state, environment).await?;
            if enforce_project_runtime_boundary(
                project,
                &mut response.environment,
                &mut response.images,
            ) {
                response.environment = state
                    .store
                    .upsert_project_runtime_environment(&response.environment)
                    .await?;
                response.images = state
                    .store
                    .replace_project_runtime_environment_images(
                        project.id.as_str(),
                        response.images.as_slice(),
                    )
                    .await?;
            }
            let Some(image_record_id) = pending_workspace_image_id(&response) else {
                return Ok(response);
            };
            super::super::generate_project_runtime_environment_image(
                state,
                project,
                user_access_token,
                image_record_id.as_str(),
            )
            .await
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

fn pending_workspace_image_id(response: &ProjectRuntimeEnvironmentResponse) -> Option<String> {
    if !response.environment.sandbox_enabled
        || matches!(
            response.environment.status,
            ProjectRuntimeEnvironmentStatus::Disabled
                | ProjectRuntimeEnvironmentStatus::Analyzing
                | ProjectRuntimeEnvironmentStatus::NotRunnable
                | ProjectRuntimeEnvironmentStatus::Failed
        )
    {
        return None;
    }
    response
        .images
        .iter()
        .find(|image| {
            image.service_role == RuntimeServiceRole::Workspace
                && image.mcp_policy.attachment == RuntimeMcpAttachment::WorkspaceGatewayTarget
                && !matches!(
                    image.status.trim().to_ascii_lowercase().as_str(),
                    "ready" | "available" | "local" | "succeeded" | "completed" | "running"
                )
        })
        .map(|image| image.id.clone())
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

#[cfg(test)]
mod automatic_image_preparation_tests {
    use super::*;

    #[test]
    fn pending_analysis_selects_the_program_workspace_image() {
        let response = ProjectRuntimeEnvironmentResponse {
            environment: ProjectRuntimeEnvironmentRecord {
                project_id: "project-1".to_string(),
                status: ProjectRuntimeEnvironmentStatus::PendingImageBuild,
                sandbox_enabled: true,
                sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
                file_provider: RuntimeEnvironmentProvider::Harness,
                analysis_summary: None,
                not_runnable_reason: None,
                execution_service_id: Some("workspace".to_string()),
                detected_stack: Value::Object(Default::default()),
                required_services: Value::Array(Vec::new()),
                env_vars: Value::Object(Default::default()),
                environment_variables: Vec::new(),
                generated_config_files: Vec::new(),
                last_agent_run_id: None,
                last_error: None,
                created_at: "2026-08-02T00:00:00Z".to_string(),
                updated_at: "2026-08-02T00:00:00Z".to_string(),
            },
            images: vec![ProjectRuntimeEnvironmentImageRecord {
                id: "workspace-image".to_string(),
                project_id: "project-1".to_string(),
                environment_key: "workspace".to_string(),
                environment_type: "workspace".to_string(),
                service_id: "workspace".to_string(),
                display_name: "Project Workspace".to_string(),
                service_role: RuntimeServiceRole::Workspace,
                source_root: ".".to_string(),
                dockerfile: None,
                image_id: None,
                image_ref: None,
                image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
                status: "planned".to_string(),
                error: None,
                features: Value::Array(Vec::new()),
                custom_build_script: None,
                startup_command: None,
                test_command: None,
                auto_start: true,
                depends_on: Vec::new(),
                ports: Value::Array(Vec::new()),
                env_vars: Value::Object(Default::default()),
                mcp_policy: ProgramManagedMcpPolicy::workspace_target(),
                component_kind: String::new(),
                created_at: "2026-08-02T00:00:00Z".to_string(),
                updated_at: "2026-08-02T00:00:00Z".to_string(),
            }],
        };

        assert_eq!(
            pending_workspace_image_id(&response).as_deref(),
            Some("workspace-image")
        );
    }

    #[test]
    fn not_runnable_project_never_prepares_a_workspace_image() {
        let mut response = ProjectRuntimeEnvironmentResponse {
            environment: ProjectRuntimeEnvironmentRecord {
                project_id: "project-1".to_string(),
                status: ProjectRuntimeEnvironmentStatus::NotRunnable,
                sandbox_enabled: true,
                sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
                file_provider: RuntimeEnvironmentProvider::Harness,
                analysis_summary: None,
                not_runnable_reason: Some("empty project".to_string()),
                execution_service_id: Some("workspace".to_string()),
                detected_stack: Value::Object(Default::default()),
                required_services: Value::Array(Vec::new()),
                env_vars: Value::Object(Default::default()),
                environment_variables: Vec::new(),
                generated_config_files: Vec::new(),
                last_agent_run_id: None,
                last_error: None,
                created_at: "2026-08-02T00:00:00Z".to_string(),
                updated_at: "2026-08-02T00:00:00Z".to_string(),
            },
            images: Vec::new(),
        };
        response.images.push(ProjectRuntimeEnvironmentImageRecord {
            id: "workspace-image".to_string(),
            project_id: "project-1".to_string(),
            environment_key: "workspace".to_string(),
            environment_type: "workspace".to_string(),
            display_name: "Project Workspace".to_string(),
            service_id: "workspace".to_string(),
            service_role: RuntimeServiceRole::Workspace,
            source_root: ".".to_string(),
            component_kind: String::new(),
            startup_command: None,
            test_command: None,
            depends_on: Vec::new(),
            auto_start: true,
            mcp_policy: ProgramManagedMcpPolicy::workspace_target(),
            image_id: None,
            image_ref: None,
            image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            features: Value::Array(Vec::new()),
            ports: Value::Array(Vec::new()),
            env_vars: Value::Object(Default::default()),
            dockerfile: None,
            custom_build_script: None,
            status: "planned".to_string(),
            error: None,
            created_at: "2026-08-02T00:00:00Z".to_string(),
            updated_at: "2026-08-02T00:00:00Z".to_string(),
        });

        assert_eq!(pending_workspace_image_id(&response), None);
    }
}

async fn run_project_environment_agent(
    state: &AppState,
    project: &ProjectRecord,
    environment_plan: RuntimeEnvironmentPlan,
    prompt_vendor: Option<&str>,
    model_config: &ModelRuntimeConfig,
    memory: &ProjectAgentMemory,
    run_context: &ProjectEnvironmentAgentRunContext<'_>,
) -> Result<(), String> {
    let agent_prompt = resolve_project_environment_agent_prompt(
        state,
        prompt_vendor,
        model_config.provider.as_str(),
    )
    .await?;
    let gateway = resolve_project_environment_mcp(
        project,
        run_context.owner_user_id,
        run_context.run_id,
        run_context.model_config_id,
    )
    .await?;
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

async fn execute_project_environment_agent(
    project: &ProjectRecord,
    model_config: &ModelRuntimeConfig,
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

fn bind_analysis_request(
    detected_stack: &mut Value,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) {
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
    if let Some(analysis_requirement) = analysis_requirement
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        object.insert(
            "analysis_requirement".to_string(),
            Value::String(analysis_requirement.to_string()),
        );
    } else {
        object.remove("analysis_requirement");
    }
}

fn build_project_environment_agent_prompt(
    project: &ProjectRecord,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) -> Result<String, String> {
    let context = project_environment_agent_context(
        project.id.as_str(),
        project.name.as_str(),
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
    })
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
    use super::{bind_analysis_request, project_environment_agent_context};

    #[test]
    fn agent_context_does_not_expose_program_routing_or_environment_state() {
        let context = project_environment_agent_context(
            "project-1",
            "Example",
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
    fn analysis_request_is_bound_to_the_persisted_analysis_run() {
        let mut detected_stack = serde_json::json!({"project_type": "rust"});
        bind_analysis_request(
            &mut detected_stack,
            Some("Build a React game and run browser tests"),
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
        assert_eq!(
            detected_stack["analysis_requirement"],
            "Build a React game and run browser tests"
        );
    }
}
