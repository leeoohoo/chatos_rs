// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{HarnessBranchTarget, RuntimeWorkspaceRouteTarget};
use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};

use crate::models::{
    now_rfc3339, EffectiveTaskToolSnapshot, TaskMcpConfig, TaskRecord, TaskRunBranchTarget,
    TaskRunRecord, TaskRunWorkspaceExecution, WorkspaceIntegrationStatus,
    WorkspacePreparationStatus,
};

use super::project_management_api_client::{
    FinalizeRunWorkspaceRequest, IntegrateRunWorkspaceRequest, PrepareRunWorkspaceRequest,
    PrepareRunWorkspaceResponse, PreparedRunBranch, RunWorkspaceIntegrationResultStatus,
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
        conflict_files: Vec::new(),
        conflict_message: None,
        integration_last_error: None,
        prepared_at: None,
        finalized_at: None,
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

pub(crate) async fn model_execution_lane_key(
    service: &RunService,
    task: &TaskRecord,
    tools: &EffectiveTaskToolSnapshot,
) -> Result<Option<String>, String> {
    if !tools.mutates_workspace() {
        return Ok(None);
    }
    let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
    if project_id == crate::models::PUBLIC_PROJECT_ID {
        return Ok(None);
    }
    let project = super::project_management_api_client::sync_get_project(
        &service.config,
        project_id.as_str(),
    )
    .await?
    .ok_or_else(|| format!("project not found while resolving execution lane: {project_id}"))?;
    match project
        .source_type
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("local_connector") | Some("local") => Ok(Some(format!("project:{project_id}"))),
        Some("cloud") => Ok(None),
        Some(value) => Err(format!(
            "unsupported project source_type for execution lane: {value}"
        )),
        None => Err("project source_type is required for execution lane".to_string()),
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
        run.workspace_execution = Some(workspace_execution(WorkspacePreparationStatus::Pending));
        run.updated_at = now_rfc3339();
        persist_workspace_execution(service, run).await?;
    }

    let prepared = prepare_workspace_inner(service, task, run).await;
    match prepared {
        Ok(prepared) => {
            let integration_status =
                if matches!(prepared.branch_target, TaskRunBranchTarget::Run { .. }) {
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
            execution_group_id: None,
            execution_branch_ref: None,
            execution_base_commit: None,
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
            let execution_group_id = create_run_branch.then(|| execution_group_id_for_task(task));
            let response = super::project_management_api_client::prepare_run_workspace(
                &service.config,
                project_id.as_str(),
                run.id.as_str(),
                &PrepareRunWorkspaceRequest {
                    owner_user_id: task_owner_user_id(task)?,
                    tenant_id: task.tenant_id.trim().to_string(),
                    create_run_branch,
                    create_cloud_sandbox,
                    execution_group_id: execution_group_id.clone(),
                    expected_execution_commit: None,
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
                        execution_group_id: None,
                        execution_branch_ref: None,
                        execution_base_commit: None,
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
                        execution_group_id,
                        execution_branch_ref: response.execution_branch_ref,
                        execution_base_commit: response.execution_base_commit,
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
                        execution_group_id,
                        execution_branch_ref: response.execution_branch_ref,
                        execution_base_commit: response.execution_base_commit,
                    })
                }
                WorkspaceRouteDecision::None | WorkspaceRouteDecision::LocalConnector => {
                    unreachable!("non-cloud decisions returned from cloud preparation")
                }
            }
        }
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

pub(crate) async fn load_task_run_workspace_changes(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
) -> Result<super::project_management_api_client::GetRunWorkspaceChangesResponse, String> {
    let execution = run
        .workspace_execution
        .as_ref()
        .ok_or_else(|| "当前运行没有代码变更上下文".to_string())?;
    let TaskRunBranchTarget::Run {
        branch_id,
        branch_ref,
        base_branch,
        base_commit,
    } = execution
        .branch_target
        .as_ref()
        .ok_or_else(|| "当前运行没有独立代码分支".to_string())?
    else {
        return Err("当前运行没有独立代码分支".to_string());
    };
    let changes_base_commit =
        if execution.integration_status == WorkspaceIntegrationStatus::Integrated {
            execution
                .integration_base_commit
                .as_ref()
                .unwrap_or(base_commit)
        } else {
            base_commit
        };
    let owner_user_id = task_owner_user_id(task)?;
    let changes = super::project_management_api_client::get_run_workspace_changes(
        &service.config,
        task.project_id.as_str(),
        run.id.as_str(),
        &super::project_management_api_client::GetRunWorkspaceChangesRequest {
            owner_user_id,
            branch: PreparedRunBranch {
                branch_id: branch_id.clone(),
                branch_ref: branch_ref.clone(),
                base_branch: base_branch.clone(),
                base_commit: changes_base_commit.clone(),
            },
        },
    )
    .await?;
    if changes.project_id != task.project_id || changes.run_id != run.id {
        return Err("Project Service returned changes for a different Task Run".to_string());
    }
    Ok(changes)
}

pub(crate) async fn finalize_task_run_workspace(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
) -> Result<(), String> {
    let Some(execution) = run.workspace_execution.clone() else {
        return Ok(());
    };
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
    let owner_user_id = task_owner_user_id(task)?;
    if execution.finalized_at.is_none() {
        let response = super::project_management_api_client::finalize_run_workspace(
            &service.config,
            project_id.as_str(),
            run.id.as_str(),
            &FinalizeRunWorkspaceRequest {
                owner_user_id: owner_user_id.clone(),
                branch: branch.clone(),
                sandbox_target,
            },
        )
        .await;
        match response {
            Ok(response) => {
                if response.project_id.trim() != project_id || response.run_id.trim() != run.id {
                    return Err(
                        "Project Service finalized a different Task Run workspace".to_string()
                    );
                }
                mark_workspace_finalized(service, run, response.result_commit).await?;
            }
            Err(error) => {
                if let Some(execution) = run.workspace_execution.as_mut() {
                    execution.finalization_error = Some(error.clone());
                }
                run.updated_at = now_rfc3339();
                let _ = persist_workspace_execution(service, run).await;
                return Err(error);
            }
        }
    }
    if run.model_phase_status != crate::models::ModelPhaseStatus::Succeeded {
        return Ok(());
    }
    let Some(branch) = branch else {
        return Ok(());
    };
    let execution = run
        .workspace_execution
        .as_ref()
        .ok_or_else(|| "Task Run workspace state disappeared before integration".to_string())?;
    if execution.integration_status == WorkspaceIntegrationStatus::Integrated
        || execution.integration_status == WorkspaceIntegrationStatus::Conflict
    {
        return Ok(());
    }
    let execution_group_id = execution
        .execution_group_id
        .clone()
        .ok_or_else(|| "Task Run workspace is missing execution_group_id".to_string())?;
    let execution_branch_ref = execution
        .execution_branch_ref
        .clone()
        .ok_or_else(|| "Task Run workspace is missing execution_branch_ref".to_string())?;
    let result_commit = execution
        .result_commit
        .clone()
        .ok_or_else(|| "Task Run workspace finalization returned no result commit".to_string())?;
    let integration_ready_at = execution
        .integration_ready_at
        .clone()
        .unwrap_or_else(now_rfc3339);
    if let Some(prior) = service
        .store
        .get_prior_pending_integration_run(
            execution_group_id.as_str(),
            integration_ready_at.as_str(),
            run.created_at.as_str(),
            run.id.as_str(),
        )
        .await?
    {
        return Err(format!(
            "{}: waiting for prior Run {} in execution group {}",
            crate::services::WORKSPACE_INTEGRATION_RETRY_PREFIX,
            prior.id,
            execution_group_id
        ));
    }
    if let Some(execution) = run.workspace_execution.as_mut() {
        execution.integration_status = WorkspaceIntegrationStatus::Integrating;
        execution.integration_started_at = Some(now_rfc3339());
        execution.integration_attempt_count = execution.integration_attempt_count.saturating_add(1);
        execution.integration_base_commit = execution.execution_base_commit.clone();
        execution.integration_last_error = None;
    }
    run.updated_at = now_rfc3339();
    persist_workspace_execution(service, run).await?;
    service
        .store
        .append_run_event(crate::models::TaskRunEventRecord::new(
            run.id.clone(),
            "integration_started",
            Some("开始集成任务代码到执行批次分支".to_string()),
            Some(serde_json::json!({
                "execution_group_id": execution_group_id,
                "execution_branch_ref": execution_branch_ref,
                "result_commit": result_commit,
            })),
        ))
        .await?;
    let response = super::project_management_api_client::integrate_run_workspace(
        &service.config,
        project_id.as_str(),
        run.id.as_str(),
        &IntegrateRunWorkspaceRequest {
            owner_user_id,
            execution_group_id,
            execution_branch_ref,
            integration_ready_at,
            branch,
            result_commit: result_commit.clone(),
        },
    )
    .await?;
    if response.project_id.trim() != project_id || response.run_id.trim() != run.id {
        return Err("Project Service integrated a different Task Run workspace".to_string());
    }
    match response.status {
        RunWorkspaceIntegrationResultStatus::Integrated => {
            if let Some(execution) = run.workspace_execution.as_mut() {
                execution.integration_status = WorkspaceIntegrationStatus::Integrated;
                execution.integrated_at = Some(now_rfc3339());
                execution.result_commit = Some(response.result_commit);
                execution.integrated_commit = response.integrated_commit;
                if response.integration_base_commit.is_some() {
                    execution.integration_base_commit = response.integration_base_commit;
                }
                execution.conflict_files.clear();
                execution.conflict_message = None;
                execution.integration_last_error = None;
            }
            run.updated_at = now_rfc3339();
            persist_workspace_execution(service, run).await?;
            Ok(())
        }
        RunWorkspaceIntegrationResultStatus::Conflict => {
            if let Some(execution) = run.workspace_execution.as_mut() {
                execution.integration_status = WorkspaceIntegrationStatus::Conflict;
                execution.conflict_files = response.conflict_files;
                execution.conflict_message = response.message;
                execution.integration_last_error = None;
            }
            run.updated_at = now_rfc3339();
            persist_workspace_execution(service, run).await?;
            Ok(())
        }
        RunWorkspaceIntegrationResultStatus::RetryableError => {
            let message = response.message.unwrap_or_else(|| {
                "Project Service reported a retryable integration error".to_string()
            });
            if let Some(execution) = run.workspace_execution.as_mut() {
                execution.integration_status = WorkspaceIntegrationStatus::Failed;
                execution.integration_last_error = Some(message.clone());
            }
            run.updated_at = now_rfc3339();
            let _ = persist_workspace_execution(service, run).await;
            Err(format!(
                "{}: {message}",
                crate::services::WORKSPACE_INTEGRATION_RETRY_PREFIX
            ))
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
        execution.result_commit = result_commit.clone();
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
