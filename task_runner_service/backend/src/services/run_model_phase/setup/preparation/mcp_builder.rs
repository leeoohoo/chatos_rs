// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::models::{SkillInstallStatus, SkillScope};
use crate::services::{SkillService, TaskRunnerSkillLookupProvider};

use super::*;

const TASK_RUNNER_SKILL_LOOKUP_SERVER_NAME: &str = "task_runner_service";
const TASK_RUNNER_SKILL_LOOKUP_KIND: &str = "TaskRunnerSkillLookup";

pub(super) async fn build_mcp_builder_parts(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
    effective_workspace_dir: &str,
    task_process_logging_enabled: bool,
    task_service: TaskService,
    sandbox_context: Option<&crate::services::sandbox_runtime::SandboxRuntimeContext>,
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

    let mut selected_builtin_kinds = runtime_selected_builtin_kinds(task);
    if sandbox_context.is_some() {
        selected_builtin_kinds
            .retain(|kind| !crate::services::sandbox_runtime::sandbox_replaces_builtin_kind(*kind));
    }
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
    apply_project_management_builtin_context(&mut builtin_servers, task);
    let local_connector_routing = task_uses_local_connector(task);
    let harness_code_routing = task_uses_harness_code(task);
    if super::is_chatos_plan_task(task) {
        if sandbox_context.is_none() && !local_connector_routing && !harness_code_routing {
            builtin_servers.push(
                chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
                    .server_with_options(&server_options),
            );
        }
        if let Some(owner_user_id) = normalized_task_owner_user_id(task) {
            builtin_servers.push(task_runner_skill_lookup_builtin_server(
                effective_workspace_dir,
                owner_user_id,
                task.id.clone(),
            ));
        }
    }
    if task_process_logging_enabled {
        builtin_servers.push(task_process_log_builtin_server());
    }

    let project_management_execution_options =
        project_management_execution_options_for_task(service, task).await;
    let (builtin_registry, builtin_init_errors) =
        build_builtin_registry_with_project_management_options(
            &builtin_servers,
            task_service.clone(),
            service.ask_user_prompt_service.clone(),
            project_management_execution_options,
        );
    let mut builtin_registry = builtin_registry;
    if super::is_chatos_plan_task(task) {
        if sandbox_context.is_none() && !local_connector_routing && !harness_code_routing {
            builtin_registry
                .register(DisabledBuiltinProvider::code_maintainer_write_for_chatos_plan());
        }
        if let Some(owner_user_id) = normalized_task_owner_user_id(task) {
            builtin_registry.register(TaskRunnerSkillLookupProvider::new(
                TASK_RUNNER_SKILL_LOOKUP_SERVER_NAME,
                SkillService::new(&service.config, service.store.clone()),
                owner_user_id,
            ));
        }
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

fn task_runner_skill_lookup_builtin_server(
    workspace_dir: &str,
    owner_user_id: String,
    project_id: String,
) -> chatos_mcp_runtime::McpBuiltinServer {
    chatos_mcp_runtime::McpBuiltinServer {
        name: TASK_RUNNER_SKILL_LOOKUP_SERVER_NAME.to_string(),
        kind: TASK_RUNNER_SKILL_LOOKUP_KIND.to_string(),
        workspace_dir: workspace_dir.to_string(),
        user_id: Some(owner_user_id),
        project_id: Some(project_id),
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes: false,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    }
}

fn apply_project_management_builtin_context(
    builtin_servers: &mut [chatos_mcp_runtime::McpBuiltinServer],
    task: &TaskRecord,
) {
    let owner_user_id = normalized_task_owner_user_id(task);
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    for server in builtin_servers {
        if server.name != chatos_mcp_runtime::PROJECT_MANAGEMENT_SERVER_NAME {
            continue;
        }
        server.user_id = owner_user_id.clone();
        server.project_id = Some(project_id.clone());
    }
}

fn normalized_task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn project_management_execution_options_for_task(
    service: &RunService,
    task: &TaskRecord,
) -> Option<ProjectManagementExecutionOptions> {
    if !super::is_chatos_plan_task(task) {
        return None;
    }
    let owner_user_id = normalized_task_owner_user_id(task)?;
    let model_config_ids = match service.store.list_model_configs().await {
        Ok(models) => models
            .into_iter()
            .filter(|model| model.enabled)
            .filter(|model| {
                owns_task_runner_resource(model.owner_user_id.as_deref(), &owner_user_id)
            })
            .map(|model| model.id)
            .collect::<BTreeSet<_>>(),
        Err(err) => {
            warn!(
                task_id = task.id.as_str(),
                owner_user_id = owner_user_id.as_str(),
                error = err.as_str(),
                "failed to load model configs for Project Management schema enrichment"
            );
            return None;
        }
    };

    let mut tool_ids = BTreeSet::new();
    for kind in chatos_mcp_runtime::configurable_builtin_kinds() {
        tool_ids.insert(kind.kind_name().to_string());
        if let Some(config_id) = kind.config_id() {
            tool_ids.insert(config_id.to_string());
        }
    }
    match service.store.list_external_mcp_configs().await {
        Ok(configs) => {
            tool_ids.extend(
                configs
                    .into_iter()
                    .filter(|config| config.enabled)
                    .filter(|config| {
                        owns_task_runner_resource(
                            task_runner_resource_owner_or_creator(
                                config.owner_user_id.as_deref(),
                                config.creator_user_id.as_deref(),
                            ),
                            &owner_user_id,
                        )
                    })
                    .map(|config| config.id),
            );
        }
        Err(err) => {
            warn!(
                task_id = task.id.as_str(),
                owner_user_id = owner_user_id.as_str(),
                error = err.as_str(),
                "failed to load external MCP configs for Project Management schema enrichment"
            );
        }
    }

    let skill_ids = match service.store.list_skills().await {
        Ok(skills) => skills
            .into_iter()
            .filter(|skill| skill.enabled && skill.install_status == SkillInstallStatus::Installed)
            .filter(|skill| {
                skill.scope == SkillScope::AdminGlobal
                    || owns_task_runner_resource(
                        task_runner_resource_owner_or_creator(
                            skill.owner_user_id.as_deref(),
                            skill.creator_user_id.as_deref(),
                        ),
                        &owner_user_id,
                    )
            })
            .map(|skill| skill.id)
            .collect::<BTreeSet<_>>(),
        Err(err) => {
            warn!(
                task_id = task.id.as_str(),
                owner_user_id = owner_user_id.as_str(),
                error = err.as_str(),
                "failed to load skills for Project Management schema enrichment"
            );
            BTreeSet::new()
        }
    };

    Some(ProjectManagementExecutionOptions {
        model_config_ids: model_config_ids.into_iter().collect(),
        preferred_model_config_id: task.default_model_config_id.clone(),
        tool_ids: tool_ids.into_iter().collect(),
        skill_ids: skill_ids.into_iter().collect(),
    })
}

fn owns_task_runner_resource(owner_user_id: Option<&str>, expected_owner_user_id: &str) -> bool {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

fn task_runner_resource_owner_or_creator<'a>(
    owner_user_id: Option<&'a str>,
    creator_user_id: Option<&'a str>,
) -> Option<&'a str> {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            creator_user_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
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
