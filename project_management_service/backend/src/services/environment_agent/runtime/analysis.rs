// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::source_snapshot::{
    bind_source_snapshot, capture_harness_source_snapshot, set_analysis_progress,
};
use super::super::*;
use chatos_agent::{AgentIdentity, SystemAgentDefinition};
use chatos_ai_runtime::{
    AiRuntime, AiRuntimeOptions, ContextualTurnRequest, ContextualTurnRunner,
    McpRuntimeToolExecutor, MemoryContextOverflowRecovery, RuntimeRecordOptions, SaveRecordInput,
};
use chatos_cloud_agent_protocol::{CloudAgentRunRecord, CloudAgentRunStatus};
use chatos_cloud_agent_runtime::{
    cloud_agent_trigger_execution_identity, cloud_agent_trigger_input_items,
    create_cloud_agent_run, CloudAgentModelTrigger, CloudAgentRunStore, CloudAgentServiceAdapter,
    CloudAgentSingleStepExecution, CloudAgentSingleStepExecutor, CloudAgentSingleStepOutput,
    NewCloudAgentRun,
};
use chatos_mcp_runtime::McpExecutor;
use chrono::Utc;
use std::sync::Arc;

struct ProjectEnvironmentSingleStepExecutor {
    state: AppState,
}

#[async_trait::async_trait]
impl CloudAgentSingleStepExecutor for AppState {
    async fn execute_single_step(
        &self,
        cloud_run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        ProjectEnvironmentSingleStepExecutor {
            state: self.clone(),
        }
        .execute_single_step(cloud_run, trigger)
        .await
    }
}

#[async_trait::async_trait]
impl CloudAgentServiceAdapter for AppState {
    fn owner_service(&self) -> &'static str {
        "project-service"
    }

    fn cloud_agent_store(&self) -> chatos_cloud_agent_runtime::CloudAgentStateStore {
        self.cloud_agent_store.clone()
    }

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String> {
        finalize_cloud_agent_terminal(self, agent_run_id).await
    }
}

#[async_trait::async_trait]
impl CloudAgentSingleStepExecutor for ProjectEnvironmentSingleStepExecutor {
    async fn execute_single_step(
        &self,
        cloud_run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        let state = &self.state;
        let run_input =
            serde_json::from_value::<ProjectEnvironmentAgentRunInput>(cloud_run.input.clone())
                .map_err(|error| {
                    format!("decode Project Environment Agent input failed: {error}")
                })?;
        if run_input.project_id != cloud_run.owner_entity_id
            || run_input.owner_user_id != cloud_run.owner_user_id
            || run_input.model_config_id != cloud_run.model_config_ref
            || run_input.agent_key != cloud_run.agent_key
        {
            return Err("Project Environment Agent persisted input identity changed".to_string());
        }
        let project = state
            .store
            .get_project(run_input.project_id.as_str())
            .await?
            .ok_or_else(|| format!("Project not found: {}", run_input.project_id))?;
        let session_id = cloud_run
            .mcp_runtime_session_ref
            .as_deref()
            .ok_or_else(|| "Project Environment Agent has no MCP runtime session".to_string())?;
        let gateway = resolve_existing_project_environment_mcp(
            &project,
            run_input.owner_user_id.as_str(),
            cloud_run.ordering.agent_run_id.as_str(),
            run_input.model_config_id.as_str(),
            session_id,
        )
        .await?;
        let executor = McpExecutor::builder()
            .with_http_server(gateway.server().clone())
            .build_initialized()
            .await?;
        ensure_agent_required_tools_available(
            &executor,
            &RuntimeEnvironmentPlan {
                file_provider: run_input.file_provider,
                sandbox_provider: run_input.sandbox_provider,
            },
        )?;
        let memory = build_project_agent_memory(
            &state.config,
            run_input.owner_user_id.as_str(),
            run_input.project_id.as_str(),
            None,
        )
        .await?;
        let agent = chatos_agent::ProjectEnvironmentAgent::for_project_locality(matches!(
            project.source_type,
            crate::models::ProjectSourceType::Local
                | crate::models::ProjectSourceType::LocalConnector
        ));
        if agent.descriptor().key.as_str() != run_input.agent_key {
            return Err("Project Environment Agent locality changed during the run".to_string());
        }
        let metadata = json!({
            "agent": "project_management_environment_agent",
            "run_id": cloud_run.ordering.agent_run_id,
            "project_id": project.id,
            "agent_prompt_vendor": run_input.agent_prompt.vendor.as_str(),
            "agent_prompt_revision": run_input.agent_prompt.revision,
            "agent_prompt_checksum": run_input.agent_prompt.checksum,
        });
        let mut model_config = agent.configure_model_with_prompt(
            run_input.model_config.clone(),
            run_input.agent_prompt.content.as_str(),
        );
        model_config.previous_response_id = cloud_run.previous_response_id.clone();
        let current_input_items = cloud_agent_trigger_input_items(
            cloud_run,
            trigger,
            vec![chatos_ai_runtime::user_text_item(run_input.prompt.clone())],
        )?;
        let retry_input_items = current_input_items.clone();
        let user_record = matches!(trigger, CloudAgentModelTrigger::RunStarted { .. }).then(|| {
            SaveRecordInput::user_message(memory.conversation_id.clone(), run_input.prompt.clone())
                .with_conversation_turn_id(cloud_run.ordering.agent_run_id.clone())
                .with_message_mode(agent.message_mode())
                .with_message_source(agent.message_source())
                .with_metadata(metadata.clone())
        });
        let record_options = RuntimeRecordOptions::persist_all()
            .with_assistant_message_mode(agent.message_mode())
            .with_assistant_message_source(agent.message_source())
            .with_assistant_metadata(metadata.clone())
            .with_tool_message_mode(agent.message_mode())
            .with_tool_message_source(agent.message_source())
            .with_tool_metadata(metadata);
        let runtime_options = AiRuntimeOptions::new(
            Some(memory.conversation_id.clone()),
            Some(cloud_run.ordering.agent_run_id.clone()),
        )
        .with_caller_model(Some(model_config.model.clone()))
        .with_caller_model_runtime(Some(model_config.to_tool_caller_model_runtime()))
        .with_record_options(record_options);
        let model_request = model_config.to_model_request(Value::Null, executor.available_tools());
        let request =
            ContextualTurnRequest::new(model_request, runtime_options, current_input_items)
                .with_memory_scope(Some(memory.scope.clone()))
                .with_user_record(user_record);
        let runtime = AiRuntime::new(Some(Arc::new(McpRuntimeToolExecutor::new(executor))))
            .with_max_iterations(usize::try_from(cloud_run.max_iterations).unwrap_or(usize::MAX))
            .with_record_writer(Some(Arc::new(memory.writer)));
        let runner = ContextualTurnRunner::new(runtime, Some(memory.composer))
            .with_context_overflow_recovery(Some(
                MemoryContextOverflowRecovery::new()
                    .with_trigger_reason(agent.context_overflow_trigger()),
            ));
        let (reason, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        let outcome = if cloud_run
            .deadline_at
            .is_some_and(|deadline| deadline <= Utc::now())
        {
            chatos_ai_runtime::AiSingleStepOutcome::Failed {
                error: "project environment analysis deadline reached".to_string(),
            }
        } else {
            runner
                .execute_once(
                    request,
                    usize::try_from(cloud_run.iteration.saturating_add(1)).unwrap_or(usize::MAX),
                    reason,
                    model_attempt,
                )
                .await?
        };
        Ok(CloudAgentSingleStepExecution::Apply(
            CloudAgentSingleStepOutput::new(outcome)
                .with_mcp_runtime(session_id, run_input.mcp_command_queue)
                .with_retry_input_items(retry_input_items),
        ))
    }
}

pub(crate) async fn finalize_cloud_agent_terminal(
    state: &AppState,
    agent_run_id: &str,
) -> Result<(), String> {
    let cloud_run = state
        .cloud_agent_store
        .load_run(agent_run_id)
        .await?
        .ok_or_else(|| format!("Cloud Agent run not found: {agent_run_id}"))?;
    if !cloud_run.status.is_terminal() {
        return Err("Project Environment lifecycle arrived before terminal state".to_string());
    }
    let run_input =
        serde_json::from_value::<ProjectEnvironmentAgentRunInput>(cloud_run.input.clone())
            .map_err(|error| format!("decode Project Environment Agent input failed: {error}"))?;
    let Some(mut environment) = state
        .store
        .get_project_runtime_environment(run_input.project_id.as_str())
        .await?
    else {
        return Err("project environment disappeared before Agent finalization".to_string());
    };
    if environment.last_agent_run_id.as_deref() == Some(agent_run_id) {
        if cloud_run.status == CloudAgentRunStatus::Succeeded
            && environment.status == ProjectRuntimeEnvironmentStatus::Analyzing
        {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary =
                Some("项目管理 Agent 已执行，但没有写入运行环境初始化结果。".to_string());
            environment.last_error =
                Some("agent did not call update_current_project_runtime_environment".to_string());
        } else if matches!(
            cloud_run.status,
            CloudAgentRunStatus::Failed
                | CloudAgentRunStatus::Blocked
                | CloudAgentRunStatus::Cancelled
        ) && environment.status == ProjectRuntimeEnvironmentStatus::Analyzing
        {
            let error = cloud_run
                .terminal_outcome
                .as_ref()
                .and_then(|outcome| outcome.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("Project Environment Agent execution failed")
                .to_string();
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("项目管理 Agent 初始化运行环境失败。".to_string());
            environment.last_error = Some(error);
        }
        environment.updated_at = now_rfc3339();
        set_analysis_progress(
            &mut environment.detected_stack,
            agent_run_id,
            if environment.status == ProjectRuntimeEnvironmentStatus::Failed {
                "agent_analysis_failed"
            } else {
                "agent_analysis_completed"
            },
            run_input.analysis_started_at.as_str(),
            environment.updated_at.as_str(),
            Some(environment.updated_at.as_str()),
            environment.last_error.as_deref(),
        );
        state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
    }
    if let Some(session_ref) = cloud_run.mcp_runtime_session_ref.as_deref() {
        let config =
            chatos_mcp_management_sdk::McpManagementClientConfig::from_env("project-service")
                .await
                .map_err(|error| error.to_string())?;
        let client = chatos_mcp_management_sdk::McpManagementClient::new(config)
            .map_err(|error| error.to_string())?;
        if let Err(error) = client.close_runtime_session(session_ref).await {
            tracing::warn!(
                agent_run_id,
                session_id = session_ref,
                error = %error,
                "close Project Environment MCP runtime session failed"
            );
        }
    }
    if environment.last_agent_run_id.as_deref() == Some(agent_run_id)
        && cloud_run.status == CloudAgentRunStatus::Succeeded
    {
        let project = state
            .store
            .get_project(run_input.project_id.as_str())
            .await?
            .ok_or_else(|| format!("Project not found: {}", run_input.project_id))?;
        let mut response = response_for_project(state, environment).await?;
        if enforce_project_runtime_boundary(
            &project,
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
        if response.environment.sandbox_provider == RuntimeEnvironmentProvider::CloudSandboxManager
        {
            if let Some(image_record_id) = pending_workspace_image_id(&response) {
                if let Err(error) = super::super::generate_project_runtime_environment_image(
                    state,
                    &project,
                    None,
                    image_record_id.as_str(),
                )
                .await
                {
                    tracing::warn!(
                        agent_run_id,
                        project_id = project.id.as_str(),
                        image_record_id = image_record_id.as_str(),
                        error = error.as_str(),
                        "automatic Project Environment image preparation failed"
                    );
                }
            }
        }
    }
    Ok(())
}

pub(in crate::services::environment_agent) async fn analyze_project_runtime_environment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    run_id: &str,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
    prefer_china_mirrors: bool,
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
        prefer_china_mirrors,
    );
    let analysis_started_at = now_rfc3339();
    environment.updated_at = analysis_started_at.clone();
    set_analysis_progress(
        &mut environment.detected_stack,
        run_id.as_str(),
        "resolving_source_snapshot",
        analysis_started_at.as_str(),
        environment.updated_at.as_str(),
        None,
        None,
    );
    environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;

    if environment_plan.file_provider == RuntimeEnvironmentProvider::Harness {
        let snapshot = match capture_harness_source_snapshot(state, project, owner_user_id).await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                environment.status = ProjectRuntimeEnvironmentStatus::Failed;
                environment.analysis_summary =
                    Some("读取 Harness 默认分支源码快照失败。".to_string());
                environment.last_error = Some(err.clone());
                environment.updated_at = now_rfc3339();
                set_analysis_progress(
                    &mut environment.detected_stack,
                    run_id.as_str(),
                    "source_snapshot_failed",
                    analysis_started_at.as_str(),
                    environment.updated_at.as_str(),
                    Some(environment.updated_at.as_str()),
                    Some(err.as_str()),
                );
                let environment = state
                    .store
                    .upsert_project_runtime_environment(&environment)
                    .await?;
                return response_for_project(state, environment).await;
            }
        };
        bind_source_snapshot(&mut environment.detected_stack, &snapshot);
    }
    environment.updated_at = now_rfc3339();
    set_analysis_progress(
        &mut environment.detected_stack,
        run_id.as_str(),
        "running_agent_analysis",
        analysis_started_at.as_str(),
        environment.updated_at.as_str(),
        None,
        None,
    );
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
            set_analysis_progress(
                &mut environment.detected_stack,
                run_id.as_str(),
                "pending_model_configuration",
                analysis_started_at.as_str(),
                environment.updated_at.as_str(),
                Some(environment.updated_at.as_str()),
                None,
            );
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
            set_analysis_progress(
                &mut environment.detected_stack,
                run_id.as_str(),
                "model_configuration_failed",
                analysis_started_at.as_str(),
                environment.updated_at.as_str(),
                Some(environment.updated_at.as_str()),
                environment.last_error.as_deref(),
            );
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
    };

    if let Err(err) = build_project_agent_memory(
        &state.config,
        owner_user_id,
        project.id.as_str(),
        user_access_token,
    )
    .await
    {
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.analysis_summary =
            Some("项目管理 Agent Memory Engine 初始化失败。".to_string());
        environment.last_error = Some(err);
        environment.updated_at = now_rfc3339();
        set_analysis_progress(
            &mut environment.detected_stack,
            run_id.as_str(),
            "memory_initialization_failed",
            analysis_started_at.as_str(),
            environment.updated_at.as_str(),
            Some(environment.updated_at.as_str()),
            environment.last_error.as_deref(),
        );
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        return response_for_project(state, environment).await;
    }
    let source_snapshot = environment.detected_stack.get("source_snapshot").cloned();
    let agent_prompt = resolve_project_environment_agent_prompt(
        state,
        project,
        model_runtime.prompt_vendor.as_deref(),
        model_runtime.model_config.provider.as_str(),
    )
    .await?;
    let gateway = resolve_project_environment_mcp(
        project,
        owner_user_id,
        run_id.as_str(),
        model_runtime.model_config_id.as_str(),
    )
    .await?;
    let persist_result = async {
        let executor = McpExecutor::builder()
            .with_http_server(gateway.server().clone())
            .build_initialized()
            .await?;
        ensure_agent_required_tools_available(&executor, &environment_plan)?;
        let mut prompt = build_project_environment_agent_prompt(
            project,
            run_id.as_str(),
            analysis_requirement,
            selected_dependencies,
            prefer_china_mirrors,
            source_snapshot.as_ref(),
        )?;
        let provider_skills_prompt = gateway.provider_skills_prompt();
        if let Some(provider_skills_prompt) = provider_skills_prompt.as_deref() {
            prompt.push_str("\n\n");
            prompt.push_str(provider_skills_prompt.trim());
        }
        let agent = chatos_agent::ProjectEnvironmentAgent::for_project_locality(matches!(
            project.source_type,
            crate::models::ProjectSourceType::Local
                | crate::models::ProjectSourceType::LocalConnector
        ));
        let agent_key = agent.descriptor().key.as_str().to_string();
        let max_iterations = chatos_agent::load_agent_max_iterations("project-service").await;
        let lane_key = format!("project_environment:{}", project.id);
        let cloud_now = Utc::now();
        let now = now_rfc3339();
        let run_input = ProjectEnvironmentAgentRunInput {
            agent_run_id: run_id.clone(),
            project_id: project.id.clone(),
            owner_user_id: owner_user_id.to_string(),
            model_config_id: model_runtime.model_config_id.clone(),
            model_config: model_runtime.model_config.clone(),
            agent_key: agent_key.clone(),
            agent_prompt: agent_prompt.clone(),
            prompt,
            mcp_command_queue: gateway.command_queue().to_string(),
            file_provider: environment_plan.file_provider,
            sandbox_provider: environment_plan.sandbox_provider,
            analysis_started_at,
            created_at: now.clone(),
            updated_at: now,
        };
        create_cloud_agent_run(
            &state.cloud_agent_store,
            NewCloudAgentRun {
                ordering_lane_key: lane_key,
                agent_run_id: run_id.clone(),
                owner_service: "project-service".to_string(),
                owner_entity_type: "project_runtime_environment".to_string(),
                owner_entity_id: project.id.clone(),
                owner_user_id: owner_user_id.to_string(),
                agent_key,
                input: serde_json::to_value(run_input).map_err(|error| {
                    format!("encode Project Environment Agent input failed: {error}")
                })?,
                model_config_ref: model_runtime.model_config_id.clone(),
                model_runtime_snapshot_ref: format!("project_environment_agent:{run_id}:input"),
                agent_prompt_revision: agent_prompt.revision.to_string(),
                agent_prompt_checksum: agent_prompt.checksum.clone(),
                capability_policy_revision: "mcp_runtime_session".to_string(),
                mcp_runtime_session_ref: Some(gateway.session_id().to_string()),
                current_input_items_ref: format!("project_environment_agent:{run_id}:initial"),
                max_iterations: u32::try_from(max_iterations).unwrap_or(u32::MAX),
                deadline_at: chrono::Duration::from_std(state.config.environment_analysis_timeout)
                    .ok()
                    .map(|duration| cloud_now + duration),
                runtime_routing_key: crate::cloud_agent_queue::PROJECT_CLOUD_AGENT_ROUTING_KEY
                    .to_string(),
                start_causation_id: project.id.clone(),
                start_payload: json!({"project_id": project.id}),
            },
        )
        .await
        .map(|_| ())
    }
    .await;
    if let Err(error) = persist_result {
        return Err(match gateway.close().await {
            Ok(()) => error,
            Err(close_error) => format!("{error}; {close_error}"),
        });
    }
    response_for_project(state, environment).await
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

#[cfg(test)]
mod cloud_agent_input_tests {
    use super::*;
    use chatos_ai_runtime::ModelRuntimeConfig;
    use chatos_plugin_management_sdk::{AgentPromptVendor, ResolvedAgentPrompt};

    #[test]
    fn durable_owner_input_round_trips_without_losing_runtime_identity() {
        let input = ProjectEnvironmentAgentRunInput {
            agent_run_id: "agent-run-1".to_string(),
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            model_config_id: "model-config-1".to_string(),
            model_config: ModelRuntimeConfig::openai_compatible(
                "https://model.example/v1",
                "secret-key",
                "gpt-test",
                "openai",
            )
            .with_thinking_level(Some("high".to_string())),
            agent_key: "project_management_agent".to_string(),
            agent_prompt: ResolvedAgentPrompt {
                agent_key: "project_management_agent".to_string(),
                vendor: AgentPromptVendor::Gpt,
                content: "system prompt".to_string(),
                revision: 7,
                checksum: "checksum-7".to_string(),
                published_at: "2026-08-12T00:00:00Z".to_string(),
            },
            prompt: "initial input".to_string(),
            mcp_command_queue: "mcp.commands".to_string(),
            file_provider: RuntimeEnvironmentProvider::Harness,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            analysis_started_at: "2026-08-12T00:00:00Z".to_string(),
            created_at: "2026-08-12T00:00:00Z".to_string(),
            updated_at: "2026-08-12T00:00:00Z".to_string(),
        };

        let decoded: ProjectEnvironmentAgentRunInput =
            serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();

        assert_eq!(decoded.agent_run_id, input.agent_run_id);
        assert_eq!(decoded.project_id, input.project_id);
        assert_eq!(decoded.agent_key, input.agent_key);
        assert_eq!(decoded.mcp_command_queue, input.mcp_command_queue);
        assert_eq!(decoded.model_config.api_key, "secret-key");
        assert_eq!(decoded.model_config.thinking_level.as_deref(), Some("high"));
        assert_eq!(decoded.agent_prompt.revision, 7);
        assert_eq!(decoded.file_provider, RuntimeEnvironmentProvider::Harness);
        assert_eq!(
            decoded.sandbox_provider,
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
    }
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

fn bind_analysis_request(
    detected_stack: &mut Value,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
    prefer_china_mirrors: bool,
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
    object.insert(
        "prefer_china_mirrors".to_string(),
        Value::Bool(prefer_china_mirrors),
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
    prefer_china_mirrors: bool,
    source_snapshot: Option<&Value>,
) -> Result<String, String> {
    let context = project_environment_agent_context(
        project.id.as_str(),
        project.name.as_str(),
        run_id,
        analysis_requirement,
        selected_dependencies,
        prefer_china_mirrors,
        source_snapshot,
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
    prefer_china_mirrors: bool,
    source_snapshot: Option<&Value>,
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
            "prefer_china_mirrors": prefer_china_mirrors,
        },
        "source_snapshot": source_snapshot,
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
            true,
            None,
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
        assert_eq!(context["analysis_request"]["prefer_china_mirrors"], true);
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
            true,
        );
        assert_eq!(
            detected_stack["selected_dependencies"],
            serde_json::json!(["PostgreSQL", "Redis"])
        );
        assert_eq!(
            detected_stack["analysis_requirement"],
            "Build a React game and run browser tests"
        );
        assert_eq!(detected_stack["prefer_china_mirrors"], true);
    }
}
