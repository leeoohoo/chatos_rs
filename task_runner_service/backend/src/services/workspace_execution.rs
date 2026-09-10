// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    McpProviderKind, RuntimeProviderFinalization, RuntimeProviderFinalizationStatus,
    RuntimeWorkspaceRouteTarget,
};
use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};

use crate::models::{
    is_reserved_internal_mcp_resource_id, now_rfc3339, EffectiveTaskToolSnapshot, TaskMcpConfig,
    TaskRecord, TaskRunBranchTarget, TaskRunRecord, TaskRunWorkspaceExecution,
    WorkspaceIntegrationStatus, WorkspacePreparationStatus,
};

use super::RunService;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct RunWorkspaceChanges {
    pub project_id: String,
    pub run_id: String,
    pub branch_ref: String,
    pub base_commit: String,
    pub result_commit: String,
    pub files: Vec<crate::models::TaskRunWorkspaceChangedFile>,
    pub patch: String,
    pub patch_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceRouteDecision {
    None,
    LocalConnector,
}

struct PreparedWorkspaceExecution {
    route: RuntimeWorkspaceRouteTarget,
    branch_target: TaskRunBranchTarget,
    execution_group_id: Option<String>,
    execution_branch_ref: Option<String>,
    execution_base_commit: Option<String>,
}

fn workspace_execution(status: WorkspacePreparationStatus) -> TaskRunWorkspaceExecution {
    TaskRunWorkspaceExecution {
        status,
        route: None,
        branch_target: None,
        execution_group_id: None,
        execution_branch_ref: None,
        execution_base_commit: None,
        integration_status: WorkspaceIntegrationStatus::NotRequired,
        integration_ready_at: None,
        integration_started_at: None,
        integrated_at: None,
        integration_attempt_count: 0,
        integration_base_commit: None,
        result_commit: None,
        integrated_commit: None,
        promoted_commit: None,
        waived_at: None,
        waiver_reason: None,
        local_changed_files: Vec::new(),
        local_patch: None,
        local_patch_truncated: false,
        conflict_files: Vec::new(),
        conflict_message: None,
        integration_last_error: None,
        prepared_at: None,
        finalized_at: None,
        lease_retained_for_diagnostics: false,
        finalization_error: None,
        error: None,
    }
}

pub(crate) fn effective_task_tool_snapshot(config: &TaskMcpConfig) -> EffectiveTaskToolSnapshot {
    let builtin_kinds = complete_builtin_kind_dependencies(
        config
            .enabled_builtin_kinds
            .iter()
            .filter_map(|kind| builtin_kind_by_any(kind)),
    );
    let mut requested_mcp_resource_ids = builtin_kinds
        .iter()
        .filter_map(|kind| chatos_mcp::system_mcp_descriptor_by_any(kind.kind_name()))
        .map(|descriptor| descriptor.resource_id.to_string())
        .chain(
            config
                .external_mcp_config_ids
                .iter()
                .filter_map(|resource_id| {
                    let resource_id = resource_id.trim();
                    (!resource_id.is_empty() && !is_reserved_internal_mcp_resource_id(resource_id))
                        .then(|| resource_id.to_string())
                }),
        )
        .collect::<Vec<_>>();
    if config.enabled {
        requested_mcp_resource_ids
            .push(chatos_plugin_management_sdk::TASK_PROCESS_LOG_MCP_RESOURCE_ID.to_string());
    }
    requested_mcp_resource_ids.sort();
    requested_mcp_resource_ids.dedup();

    EffectiveTaskToolSnapshot {
        requested_mcp_resource_ids,
        workspace_read: builtin_kinds.contains(&BuiltinMcpKind::CodeMaintainerRead),
        workspace_write: builtin_kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite),
        terminal: builtin_kinds.contains(&BuiltinMcpKind::TerminalController),
    }
}

pub(crate) fn effective_task_tool_snapshot_for_scope(
    config: &TaskMcpConfig,
    scope: &crate::models::TaskExecutionScope,
) -> EffectiveTaskToolSnapshot {
    let mut snapshot = effective_task_tool_snapshot(config);
    remove_non_project_workspace_tools(scope, &mut snapshot);
    snapshot
}

fn remove_non_project_workspace_tools(
    scope: &crate::models::TaskExecutionScope,
    snapshot: &mut EffectiveTaskToolSnapshot,
) {
    if scope.workspace_project_id().is_some() {
        return;
    }

    snapshot.requested_mcp_resource_ids.retain(|resource_id| {
        chatos_mcp::system_mcp_descriptor_by_resource_id(resource_id).is_none_or(|descriptor| {
            !matches!(
                descriptor.key,
                chatos_plugin_management_sdk::SystemMcpKey::CodeMaintainerRead
                    | chatos_plugin_management_sdk::SystemMcpKey::CodeMaintainerWrite
                    | chatos_plugin_management_sdk::SystemMcpKey::TerminalController
            )
        })
    });
    snapshot.workspace_read = false;
    snapshot.workspace_write = false;
    snapshot.terminal = false;
}

fn owned_workspace_paths(task: &TaskRecord) -> Result<Vec<String>, String> {
    let payload = task
        .input_payload
        .as_ref()
        .unwrap_or(&serde_json::Value::Null);
    owned_workspace_paths_from_payload(payload)
}

fn owned_workspace_paths_from_payload(payload: &serde_json::Value) -> Result<Vec<String>, String> {
    let owned_paths = payload
        .get("owned_paths")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(normalize_owned_workspace_root)
        .collect::<Result<Vec<_>, _>>()?;
    let mut owned_paths = owned_paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    owned_paths.sort();
    owned_paths.dedup();
    Ok(owned_paths)
}

fn normalize_owned_workspace_root(path: &str) -> Result<String, String> {
    let normalized = path.trim().trim_matches('/').to_string();
    if normalized.is_empty() {
        return Ok(String::new());
    }
    if normalized.starts_with(['/', '\\'])
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment
                    .chars()
                    .any(|value| value == '\\' || value.is_control())
        })
    {
        return Err(format!(
            "platform_task_capability_invalid: owned path is not a safe relative workspace root: {path}"
        ));
    }
    Ok(normalized)
}

pub(crate) fn task_runtime_capability_fingerprint(task: &TaskRecord) -> String {
    let mut builtin_kinds = task
        .mcp_config
        .enabled_builtin_kinds
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    builtin_kinds.sort();
    builtin_kinds.dedup();
    let mut external_mcp_config_ids = task
        .mcp_config
        .external_mcp_config_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    external_mcp_config_ids.sort();
    external_mcp_config_ids.dedup();
    let mut owned_paths = task
        .input_payload
        .as_ref()
        .and_then(|payload| payload.get("owned_paths"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(|value| value.trim().replace('\\', "/"))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    owned_paths.sort();
    owned_paths.dedup();
    let snapshot = serde_json::json!({
        "project_id": task.project_id,
        "task_profile": task.task_profile,
        "requires_execution": task.mcp_config.requires_execution,
        "workspace_changes_required": task.mcp_config.workspace_changes_required,
        "workspace_dir": task.mcp_config.workspace_dir,
        "enabled_builtin_kinds": builtin_kinds,
        "external_mcp_config_ids": external_mcp_config_ids,
        "task_role": task.input_payload.as_ref()
            .and_then(|payload| payload.get("task_role"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase),
        "owned_paths": owned_paths,
    });
    let serialized = serde_json::to_vec(&snapshot).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325u64;
    for byte in serialized {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub(crate) fn decide_workspace_route(
    tools: &EffectiveTaskToolSnapshot,
) -> Result<WorkspaceRouteDecision, String> {
    if !tools.uses_workspace() {
        return Ok(WorkspaceRouteDecision::None);
    }
    Ok(WorkspaceRouteDecision::LocalConnector)
}

pub(crate) async fn model_execution_lane_key(
    service: &RunService,
    task: &TaskRecord,
    tools: &EffectiveTaskToolSnapshot,
) -> Result<Option<String>, String> {
    if !tools.mutates_workspace() {
        return Ok(None);
    }
    let _ = (service, task);
    Ok(None)
}

pub(crate) async fn prepare_task_run_workspace(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
) -> Result<Option<RuntimeWorkspaceRouteTarget>, String> {
    let scope = task.execution_scope();
    if scope.workspace_project_id().is_none() {
        remove_non_project_workspace_tools(&scope, &mut run.effective_tools);
        if run.workspace_execution.is_some() {
            run.workspace_execution = None;
            run.updated_at = now_rfc3339();
            persist_workspace_execution(service, run).await?;
        }
        return Ok(None);
    }

    if let Some(execution) = run.workspace_execution.as_ref() {
        match execution.status {
            WorkspacePreparationStatus::Ready => return Ok(execution.route.clone()),
            WorkspacePreparationStatus::Failed => {
                return Err(execution
                    .error
                    .clone()
                    .unwrap_or_else(|| "Task Run workspace preparation failed".to_string()))
            }
            WorkspacePreparationStatus::Pending => {}
        }
    }
    if !run.effective_tools.uses_workspace() {
        return Ok(None);
    }

    if run.workspace_execution.is_none() {
        run.workspace_execution = Some(workspace_execution(WorkspacePreparationStatus::Pending));
        run.updated_at = now_rfc3339();
        persist_workspace_execution(service, run).await?;
    }

    let prepared = prepare_workspace_inner(service, task, run).await;
    match prepared {
        Ok(prepared) => {
            let integration_status = if prepared.execution_group_id.is_some()
                && matches!(
                    prepared.branch_target,
                    TaskRunBranchTarget::Run { .. } | TaskRunBranchTarget::Local
                ) {
                WorkspaceIntegrationStatus::Pending
            } else {
                WorkspaceIntegrationStatus::NotRequired
            };
            run.workspace_execution = Some(TaskRunWorkspaceExecution {
                route: Some(prepared.route.clone()),
                branch_target: Some(prepared.branch_target),
                execution_group_id: prepared.execution_group_id,
                execution_branch_ref: prepared.execution_branch_ref,
                execution_base_commit: prepared.execution_base_commit,
                integration_status,
                prepared_at: Some(now_rfc3339()),
                ..workspace_execution(WorkspacePreparationStatus::Ready)
            });
            run.updated_at = now_rfc3339();
            persist_workspace_execution(service, run).await?;
            service
                .store
                .append_run_event(crate::models::TaskRunEventRecord::new(
                    run.id.clone(),
                    "workspace_prepared",
                    Some("任务工作区已准备完成".to_string()),
                    serde_json::to_value(&prepared.route).ok(),
                ))
                .await?;
            Ok(Some(prepared.route))
        }
        Err(error) => {
            run.workspace_execution = Some(TaskRunWorkspaceExecution {
                error: Some(error.clone()),
                ..workspace_execution(WorkspacePreparationStatus::Failed)
            });
            run.updated_at = now_rfc3339();
            let _ = persist_workspace_execution(service, run).await;
            let _ = service
                .store
                .append_run_event(crate::models::TaskRunEventRecord::new(
                    run.id.clone(),
                    "workspace_prepare_failed",
                    Some(format!("任务工作区准备失败：{error}")),
                    Some(serde_json::json!({"error": error})),
                ))
                .await;
            Err(error)
        }
    }
}

async fn prepare_workspace_inner(
    _service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
) -> Result<PreparedWorkspaceExecution, String> {
    let scope = task.execution_scope();
    let _project_id = scope
        .workspace_project_id()
        .ok_or_else(|| "user conversation tasks do not have a project workspace".to_string())?;
    task.project_context
        .as_ref()
        .ok_or_else(|| "project task is missing its frozen client project context".to_string())?;
    let decision = decide_workspace_route(&run.effective_tools)?;
    match decision {
        WorkspaceRouteDecision::None => {
            Err("workspace preparation was requested without workspace tools".to_string())
        }
        WorkspaceRouteDecision::LocalConnector => Ok(PreparedWorkspaceExecution {
            route: RuntimeWorkspaceRouteTarget::LocalConnector {
                default_tool_root: None,
                owned_paths: owned_workspace_paths(task)?,
            },
            branch_target: TaskRunBranchTarget::Local,
            execution_group_id: run
                .effective_tools
                .mutates_workspace()
                .then(|| execution_group_id_for_task(task)),
            execution_branch_ref: None,
            execution_base_commit: None,
        }),
    }
}

pub(super) fn execution_group_id_for_task(task: &TaskRecord) -> String {
    task.input_payload
        .as_ref()
        .and_then(|payload| payload.get("execution_group_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            task.source_user_message_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(task.id.as_str())
        .to_string()
}

async fn persist_workspace_execution(
    service: &RunService,
    run: &mut TaskRunRecord,
) -> Result<(), String> {
    let saved = service.store.save_run(run.clone()).await?;
    *run = saved;
    Ok(())
}

pub(crate) async fn load_task_run_workspace_changes(
    _service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
) -> Result<RunWorkspaceChanges, String> {
    let execution = run
        .workspace_execution
        .as_ref()
        .ok_or_else(|| "当前运行没有代码变更上下文".to_string())?;
    if matches!(
        execution.route.as_ref(),
        Some(RuntimeWorkspaceRouteTarget::LocalConnector { .. })
    ) {
        let project_id = task.project_id.clone().ok_or_else(|| {
            "workspace change inspection requires a concrete project scope".to_string()
        })?;
        return Ok(RunWorkspaceChanges {
            project_id,
            run_id: run.id.clone(),
            branch_ref: format!(
                "local-run:{}",
                execution
                    .execution_group_id
                    .as_deref()
                    .unwrap_or(run.id.as_str())
            ),
            base_commit: execution
                .execution_base_commit
                .clone()
                .ok_or_else(|| "本地运行尚未返回代码快照提交".to_string())?,
            result_commit: execution
                .result_commit
                .clone()
                .ok_or_else(|| "本地运行尚未返回结果提交".to_string())?,
            files: execution.local_changed_files.clone(),
            patch: execution.local_patch.clone().unwrap_or_default(),
            patch_truncated: execution.local_patch_truncated,
        });
    }
    Err(
        "workspace changes are available only from the Local Connector execution result"
            .to_string(),
    )
}

pub(crate) async fn finalize_task_run_workspace(
    _service: &RunService,
    _task: &TaskRecord,
    run: &mut TaskRunRecord,
) -> Result<(), String> {
    let Some(execution) = run.workspace_execution.as_ref() else {
        return Ok(());
    };
    if execution.status != WorkspacePreparationStatus::Ready {
        return Ok(());
    }
    if !matches!(
        execution.route.as_ref(),
        Some(RuntimeWorkspaceRouteTarget::LocalConnector { .. })
    ) {
        return Err("Task Runner accepts only Local Connector project workspaces".to_string());
    }
    Ok(())
}
pub(crate) async fn apply_runtime_provider_finalization(
    service: &RunService,
    run: &mut TaskRunRecord,
    provider_finalization: Option<&RuntimeProviderFinalization>,
) -> Result<(), String> {
    let Some(execution) = run.workspace_execution.as_ref() else {
        return Ok(());
    };
    if !matches!(
        execution.route.as_ref(),
        Some(RuntimeWorkspaceRouteTarget::LocalConnector { .. })
    ) {
        return Ok(());
    }
    if run.model_phase_status != crate::models::ModelPhaseStatus::Succeeded {
        return Ok(());
    }
    if execution.integration_status == WorkspaceIntegrationStatus::Waived {
        return Ok(());
    }
    if execution.finalized_at.is_some()
        && matches!(
            execution.integration_status,
            WorkspaceIntegrationStatus::Integrated | WorkspaceIntegrationStatus::Conflict
        )
    {
        return Ok(());
    }
    let finalization = provider_finalization.ok_or_else(|| {
        format!(
            "{}: Local Connector did not return a Git finalization result",
            crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX
        )
    })?;
    if finalization.provider_kind != McpProviderKind::LocalConnector {
        return Err("MCP Management returned finalization for the wrong provider".to_string());
    }
    if finalization.execution_group_id.as_deref() != execution.execution_group_id.as_deref() {
        return Err(
            "Local Connector returned finalization for a different execution group".to_string(),
        );
    }
    let now = now_rfc3339();
    let execution = run
        .workspace_execution
        .as_mut()
        .expect("workspace execution checked above");
    execution.finalized_at = Some(now.clone());
    execution.finalization_error = None;
    execution.execution_branch_ref = finalization.execution_branch_ref.clone();
    execution.execution_base_commit = finalization.base_commit.clone();
    execution.result_commit = finalization.result_commit.clone();
    execution.local_changed_files = finalization
        .files
        .iter()
        .map(|file| crate::models::TaskRunWorkspaceChangedFile {
            status: file.status.clone(),
            path: file.path.clone(),
            old_path: file.old_path.clone(),
        })
        .collect();
    execution.local_patch = finalization.patch.clone();
    execution.local_patch_truncated = finalization.patch_truncated;
    execution.integration_attempt_count = execution.integration_attempt_count.saturating_add(1);
    match finalization.status {
        RuntimeProviderFinalizationStatus::Succeeded
        | RuntimeProviderFinalizationStatus::NoChanges => {
            execution.integration_status = WorkspaceIntegrationStatus::Integrated;
            execution.integrated_at = Some(now.clone());
            execution.integrated_commit = finalization.integrated_commit.clone();
            execution.conflict_files.clear();
            execution.conflict_message = None;
            execution.integration_last_error = None;
        }
        RuntimeProviderFinalizationStatus::Conflict => {
            execution.integration_status = WorkspaceIntegrationStatus::Conflict;
            execution.conflict_files = finalization.conflict_files.clone();
            execution.conflict_message = finalization.message.clone();
            execution.integration_last_error = None;
        }
    }
    run.updated_at = now;
    persist_workspace_execution(service, run).await?;
    service
        .store
        .append_run_event(crate::models::TaskRunEventRecord::new(
            run.id.clone(),
            match finalization.status {
                RuntimeProviderFinalizationStatus::Succeeded
                | RuntimeProviderFinalizationStatus::NoChanges => "integration_completed",
                RuntimeProviderFinalizationStatus::Conflict => "integration_conflict",
            },
            Some(match finalization.status {
                RuntimeProviderFinalizationStatus::Succeeded => {
                    "本地 Run 代码已集成到执行批次 worktree".to_string()
                }
                RuntimeProviderFinalizationStatus::NoChanges => {
                    "本地 Run 没有代码变更，已完成集成门禁".to_string()
                }
                RuntimeProviderFinalizationStatus::Conflict => {
                    "本地 Run 代码与执行批次 worktree 冲突".to_string()
                }
            }),
            serde_json::to_value(finalization).ok(),
        ))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(read: bool, write: bool, terminal: bool) -> EffectiveTaskToolSnapshot {
        EffectiveTaskToolSnapshot {
            requested_mcp_resource_ids: Vec::new(),
            workspace_read: read,
            workspace_write: write,
            terminal,
        }
    }

    #[test]
    fn workspace_tools_always_choose_local_connector() {
        assert_eq!(
            decide_workspace_route(&tools(true, true, true)).unwrap(),
            WorkspaceRouteDecision::LocalConnector
        );
        assert_eq!(
            decide_workspace_route(&tools(true, true, false)).unwrap(),
            WorkspaceRouteDecision::LocalConnector
        );
        assert_eq!(
            decide_workspace_route(&tools(true, false, false)).unwrap(),
            WorkspaceRouteDecision::LocalConnector
        );
    }

    #[test]
    fn tasks_without_workspace_tools_need_no_workspace_route() {
        assert_eq!(
            decide_workspace_route(&tools(false, false, false)).unwrap(),
            WorkspaceRouteDecision::None
        );
    }

    #[test]
    fn user_conversation_tasks_drop_workspace_tools_but_keep_remote_capabilities() {
        let config = TaskMcpConfig {
            enabled: true,
            enabled_builtin_kinds: vec![
                "CodeMaintainerWrite".to_string(),
                "TerminalController".to_string(),
            ],
            external_mcp_config_ids: vec!["browser-mcp".to_string()],
            ..TaskMcpConfig::default()
        };

        let scope = crate::models::resolve_task_execution_scope(None, "tenant-1", "user-1");
        let snapshot = effective_task_tool_snapshot_for_scope(&config, &scope);

        assert!(!snapshot.workspace_read);
        assert!(!snapshot.workspace_write);
        assert!(!snapshot.terminal);
        assert_eq!(
            snapshot.requested_mcp_resource_ids,
            vec![
                "browser-mcp".to_string(),
                "system_mcp_task_process_log".to_string(),
            ]
        );
    }

    #[test]
    fn concrete_projects_keep_workspace_tools() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
            ..TaskMcpConfig::default()
        };

        let scope =
            crate::models::resolve_task_execution_scope(Some("project-1"), "tenant-1", "user-1");
        let snapshot = effective_task_tool_snapshot_for_scope(&config, &scope);

        assert!(snapshot.workspace_read);
        assert!(snapshot
            .requested_mcp_resource_ids
            .contains(&"builtin_code_maintainer_read".to_string()));
    }

    #[test]
    fn task_owned_paths_are_preserved_as_write_scope() {
        let payload = serde_json::json!({
            "owned_paths": ["README.md", "backend", "README.md"]
        });

        assert_eq!(
            owned_workspace_paths_from_payload(&payload).unwrap(),
            vec!["README.md".to_string(), "backend".to_string()]
        );
    }

    #[test]
    fn unsafe_task_owned_path_is_rejected() {
        let payload = serde_json::json!({
            "owned_paths": ["../backend"]
        });

        let error = owned_workspace_paths_from_payload(&payload).unwrap_err();
        assert!(error.contains("owned path"));
    }
}
