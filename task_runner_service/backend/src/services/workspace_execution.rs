// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{HarnessBranchTarget, RuntimeWorkspaceRouteTarget};
use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};

use crate::models::{
    now_rfc3339, EffectiveTaskToolSnapshot, TaskMcpConfig, TaskRecord, TaskRunBranchTarget,
    TaskRunRecord, TaskRunWorkspaceExecution, WorkspacePreparationStatus,
};

use super::project_management_api_client::{
    FinalizeRunWorkspaceRequest, PrepareRunWorkspaceRequest, PrepareRunWorkspaceResponse,
    PreparedRunBranch,
};
use super::RunService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceRouteDecision {
    None,
    LocalConnector,
    HarnessDefaultBranch,
    HarnessRunBranch,
    CloudSandboxRunBranch,
}

struct PreparedWorkspaceExecution {
    route: RuntimeWorkspaceRouteTarget,
    branch_target: TaskRunBranchTarget,
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
                    (!resource_id.is_empty()).then(|| resource_id.to_string())
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

pub(crate) fn decide_workspace_route(
    source_type: Option<&str>,
    tools: &EffectiveTaskToolSnapshot,
) -> Result<WorkspaceRouteDecision, String> {
    if !tools.uses_workspace() {
        return Ok(WorkspaceRouteDecision::None);
    }
    match source_type
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("local_connector") | Some("local") => Ok(WorkspaceRouteDecision::LocalConnector),
        Some("cloud") => {
            if tools.terminal {
                Ok(WorkspaceRouteDecision::CloudSandboxRunBranch)
            } else if tools.workspace_write {
                Ok(WorkspaceRouteDecision::HarnessRunBranch)
            } else {
                Ok(WorkspaceRouteDecision::HarnessDefaultBranch)
            }
        }
        Some(value) => Err(format!(
            "unsupported project source_type for workspace routing: {value}"
        )),
        None => Err("project source_type is required for workspace routing".to_string()),
    }
}

pub(crate) async fn prepare_task_run_workspace(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
) -> Result<Option<RuntimeWorkspaceRouteTarget>, String> {
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
        run.workspace_execution = Some(TaskRunWorkspaceExecution {
            status: WorkspacePreparationStatus::Pending,
            route: None,
            branch_target: None,
            prepared_at: None,
            finalized_at: None,
            finalization_error: None,
            error: None,
        });
        run.updated_at = now_rfc3339();
        persist_workspace_execution(service, run).await?;
    }

    let prepared = prepare_workspace_inner(service, task, run).await;
    match prepared {
        Ok(prepared) => {
            run.workspace_execution = Some(TaskRunWorkspaceExecution {
                status: WorkspacePreparationStatus::Ready,
                route: Some(prepared.route.clone()),
                branch_target: Some(prepared.branch_target),
                prepared_at: Some(now_rfc3339()),
                finalized_at: None,
                finalization_error: None,
                error: None,
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
                status: WorkspacePreparationStatus::Failed,
                route: None,
                branch_target: None,
                prepared_at: None,
                finalized_at: None,
                finalization_error: None,
                error: Some(error.clone()),
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
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
) -> Result<PreparedWorkspaceExecution, String> {
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    let project = super::project_management_api_client::sync_get_project(
        &service.config,
        project_id.as_str(),
    )
    .await?
    .ok_or_else(|| format!("project not found while preparing Task Run workspace: {project_id}"))?;
    let decision = decide_workspace_route(project.source_type.as_deref(), &run.effective_tools)?;
    match decision {
        WorkspaceRouteDecision::None => {
            Err("workspace preparation was requested without workspace tools".to_string())
        }
        WorkspaceRouteDecision::LocalConnector => Ok(PreparedWorkspaceExecution {
            route: RuntimeWorkspaceRouteTarget::LocalConnector,
            branch_target: TaskRunBranchTarget::Local,
        }),
        WorkspaceRouteDecision::HarnessDefaultBranch
        | WorkspaceRouteDecision::HarnessRunBranch
        | WorkspaceRouteDecision::CloudSandboxRunBranch => {
            let create_run_branch = matches!(
                decision,
                WorkspaceRouteDecision::HarnessRunBranch
                    | WorkspaceRouteDecision::CloudSandboxRunBranch
            );
            let create_cloud_sandbox = decision == WorkspaceRouteDecision::CloudSandboxRunBranch;
            let response = super::project_management_api_client::prepare_run_workspace(
                &service.config,
                project_id.as_str(),
                run.id.as_str(),
                &PrepareRunWorkspaceRequest {
                    owner_user_id: task_owner_user_id(task)?,
                    tenant_id: task.tenant_id.trim().to_string(),
                    create_run_branch,
                    create_cloud_sandbox,
                },
            )
            .await?;
            validate_prepared_identity(&response, project_id.as_str(), run.id.as_str())?;
            match decision {
                WorkspaceRouteDecision::HarnessDefaultBranch => {
                    let branch_ref = response.default_branch;
                    Ok(PreparedWorkspaceExecution {
                        route: RuntimeWorkspaceRouteTarget::Harness {
                            branch: HarnessBranchTarget::Default {
                                branch_ref: branch_ref.clone(),
                            },
                        },
                        branch_target: TaskRunBranchTarget::Default { branch_ref },
                    })
                }
                WorkspaceRouteDecision::HarnessRunBranch => {
                    let branch = response.branch.ok_or_else(|| {
                        "Project Service did not return the required run branch".to_string()
                    })?;
                    Ok(PreparedWorkspaceExecution {
                        route: RuntimeWorkspaceRouteTarget::Harness {
                            branch: HarnessBranchTarget::Run {
                                branch_id: branch.branch_id.clone(),
                                branch_ref: branch.branch_ref.clone(),
                                base_branch: branch.base_branch.clone(),
                                base_commit: branch.base_commit.clone(),
                            },
                        },
                        branch_target: TaskRunBranchTarget::Run {
                            branch_id: branch.branch_id,
                            branch_ref: branch.branch_ref,
                            base_branch: branch.base_branch,
                            base_commit: branch.base_commit,
                        },
                    })
                }
                WorkspaceRouteDecision::CloudSandboxRunBranch => {
                    if response.branch.is_none() {
                        return Err(
                            "Project Service did not prepare the required sandbox source branch"
                                .to_string(),
                        );
                    }
                    let target = response.sandbox_target.ok_or_else(|| {
                        "Project Service did not return the required cloud sandbox target"
                            .to_string()
                    })?;
                    let branch = response.branch.expect("branch checked above");
                    Ok(PreparedWorkspaceExecution {
                        route: RuntimeWorkspaceRouteTarget::CloudSandbox { target },
                        branch_target: TaskRunBranchTarget::Run {
                            branch_id: branch.branch_id,
                            branch_ref: branch.branch_ref,
                            base_branch: branch.base_branch,
                            base_commit: branch.base_commit,
                        },
                    })
                }
                WorkspaceRouteDecision::None | WorkspaceRouteDecision::LocalConnector => {
                    unreachable!("non-cloud decisions returned from cloud preparation")
                }
            }
        }
    }
}

async fn persist_workspace_execution(
    service: &RunService,
    run: &mut TaskRunRecord,
) -> Result<(), String> {
    let saved = service.store.save_run(run.clone()).await?;
    *run = saved;
    Ok(())
}

fn task_owner_user_id(task: &TaskRecord) -> Result<String, String> {
    let owner_user_id = task
        .owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .unwrap_or(task.subject_id.as_str())
        .trim()
        .to_string();
    if owner_user_id.is_empty() {
        Err("task owner user id is required for workspace preparation".to_string())
    } else {
        Ok(owner_user_id)
    }
}

fn validate_prepared_identity(
    response: &PrepareRunWorkspaceResponse,
    project_id: &str,
    run_id: &str,
) -> Result<(), String> {
    if response.project_id.trim() != project_id || response.run_id.trim() != run_id {
        return Err("Project Service returned a workspace for a different Task Run".to_string());
    }
    Ok(())
}

pub(crate) async fn finalize_task_run_workspace(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
) -> Result<(), String> {
    let Some(execution) = run.workspace_execution.clone() else {
        return Ok(());
    };
    if execution.finalized_at.is_some() {
        return Ok(());
    }
    if execution.status != WorkspacePreparationStatus::Ready {
        return Ok(());
    }
    let branch = match execution.branch_target.as_ref() {
        Some(TaskRunBranchTarget::Run {
            branch_id,
            branch_ref,
            base_branch,
            base_commit,
        }) => Some(PreparedRunBranch {
            branch_id: branch_id.clone(),
            branch_ref: branch_ref.clone(),
            base_branch: base_branch.clone(),
            base_commit: base_commit.clone(),
        }),
        Some(TaskRunBranchTarget::Local | TaskRunBranchTarget::Default { .. }) | None => None,
    };
    let sandbox_target = execution
        .route
        .as_ref()
        .and_then(RuntimeWorkspaceRouteTarget::sandbox_target)
        .cloned();
    if branch.is_none() && sandbox_target.is_none() {
        mark_workspace_finalized(service, run, None).await?;
        return Ok(());
    }
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    let response = super::project_management_api_client::finalize_run_workspace(
        &service.config,
        project_id.as_str(),
        run.id.as_str(),
        &FinalizeRunWorkspaceRequest {
            owner_user_id: task_owner_user_id(task)?,
            promote_changes: run.status == crate::models::TaskRunStatus::Succeeded,
            branch,
            sandbox_target,
        },
    )
    .await;
    match response {
        Ok(response) => {
            if response.project_id.trim() != project_id || response.run_id.trim() != run.id {
                return Err("Project Service finalized a different Task Run workspace".to_string());
            }
            if run.status == crate::models::TaskRunStatus::Succeeded
                && execution
                    .branch_target
                    .as_ref()
                    .is_some_and(|target| matches!(target, TaskRunBranchTarget::Run { .. }))
                && !response.promoted
            {
                return Err("Task Run workspace changes were not promoted".to_string());
            }
            mark_workspace_finalized(service, run, response.result_commit).await
        }
        Err(error) => {
            if let Some(execution) = run.workspace_execution.as_mut() {
                execution.finalization_error = Some(error.clone());
            }
            run.updated_at = now_rfc3339();
            let _ = persist_workspace_execution(service, run).await;
            Err(error)
        }
    }
}

async fn mark_workspace_finalized(
    service: &RunService,
    run: &mut TaskRunRecord,
    result_commit: Option<String>,
) -> Result<(), String> {
    if let Some(execution) = run.workspace_execution.as_mut() {
        execution.finalized_at = Some(now_rfc3339());
        execution.finalization_error = None;
    }
    run.updated_at = now_rfc3339();
    persist_workspace_execution(service, run).await?;
    service
        .store
        .append_run_event(crate::models::TaskRunEventRecord::new(
            run.id.clone(),
            "workspace_finalized",
            Some("任务工作区已完成回收".to_string()),
            Some(serde_json::json!({"result_commit": result_commit})),
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
    fn cloud_terminal_can_only_choose_cloud_sandbox() {
        assert_eq!(
            decide_workspace_route(Some("cloud"), &tools(true, true, true)).unwrap(),
            WorkspaceRouteDecision::CloudSandboxRunBranch
        );
    }

    #[test]
    fn cloud_write_uses_a_harness_run_branch_without_a_sandbox() {
        assert_eq!(
            decide_workspace_route(Some("cloud"), &tools(true, true, false)).unwrap(),
            WorkspaceRouteDecision::HarnessRunBranch
        );
    }

    #[test]
    fn cloud_read_only_uses_the_default_harness_branch() {
        assert_eq!(
            decide_workspace_route(Some("cloud"), &tools(true, false, false)).unwrap(),
            WorkspaceRouteDecision::HarnessDefaultBranch
        );
    }

    #[test]
    fn local_projects_never_choose_a_cloud_sandbox() {
        assert_eq!(
            decide_workspace_route(Some("local_connector"), &tools(true, true, true)).unwrap(),
            WorkspaceRouteDecision::LocalConnector
        );
    }
}
