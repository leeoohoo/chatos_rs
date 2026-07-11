// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn build_mcp_builder_parts(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
    effective_workspace_dir: &str,
    task_process_logging_enabled: bool,
    task_service: TaskService,
    sandbox_context: Option<&crate::services::sandbox_runtime::SandboxRuntimeContext>,
    authoritative_policy: bool,
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

    let mut selected_builtin_kinds = if authoritative_policy {
        runtime_selected_builtin_kinds_authoritative(task)
    } else {
        runtime_selected_builtin_kinds(task)
    };
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
    if super::is_chatos_plan_task(task) {
        if sandbox_context.is_none() && !local_connector_routing && !harness_code_routing {
            builtin_registry
                .register(DisabledBuiltinProvider::code_maintainer_write_for_chatos_plan());
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
