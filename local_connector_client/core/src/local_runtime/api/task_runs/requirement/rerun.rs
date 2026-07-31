// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chatos_project_execution::{
    ExecutionPlanIdentity, ExecutionPlane, STATUS_EXECUTION_STARTED, STATUS_STOPPED,
    STATUS_STOPPING,
};
use serde_json::{json, Value};
use tokio::process::Command;
use uuid::Uuid;

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::local_runtime::storage::{BeginLocalTurnInput, BeginLocalTurnResult};
use crate::local_runtime::task_runner::CreateLocalConversationTaskInput;
use crate::workspace::paths::normalize_relative_workspace_path;
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::RerunRequirementExecutionPayload;

pub(in crate::local_runtime::api::task_runs) async fn rerun_requirement_execution(
    AxumPath((project_id, requirement_id)): AxumPath<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<RerunRequirementExecutionPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let identity = ExecutionPlanIdentity::required(
        payload.execution_group_id.as_str(),
        payload.conversation_id.as_str(),
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_execution_plan_identity_required", message)
    })?;
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let project = database
        .get_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_project_not_found",
                "Local project was not found",
            )
        })?;
    if project.execution_plane != "local_connector" {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plane_mismatch",
            "Local requirement execution is only available for local_connector projects",
        ));
    }
    let source_messages = database
        .list_turn_messages(
            owner.owner_user_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await?;
    let source_message = source_messages
        .iter()
        .find(|message| message.role == "user")
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_execution_plan_source_missing",
                "Local execution plan source message was not found",
            )
        })?;
    if source_message.session_id != identity.conversation_id {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_conversation_mismatch",
            "Local execution plan does not belong to this conversation",
        ));
    }
    let source_metadata = source_message
        .metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .unwrap_or_else(|| json!({}));
    validate_source_scope(
        &source_metadata,
        project_id.as_str(),
        requirement_id.as_str(),
    )?;
    let source_status = source_status(&source_metadata);
    let old_tasks = database
        .list_local_project_execution_tasks(owner.owner_user_id.as_str(), project_id.as_str())
        .await?
        .into_iter()
        .filter(|task| {
            task.execution_group_id.as_deref() == Some(identity.execution_group_id.as_str())
                && task.conversation_id == identity.conversation_id
        })
        .collect::<Vec<_>>();
    if old_tasks.is_empty() {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_not_ready",
            "The stopped local execution task graph is unavailable",
        ));
    }
    let old_runs = database
        .list_local_execution_group_task_runs(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await?;
    match resolve_local_execution_batch_state(
        source_status.as_str(),
        old_runs.iter().map(|run| run.status.as_str()),
    ) {
        LocalExecutionBatchState::ReplacementReady => {
            if source_status == STATUS_STOPPING
                || !source_status_is_stopped_terminal(source_status.as_str())
            {
                database
                    .set_turn_task_runner_status(
                        owner.owner_user_id.as_str(),
                        identity.execution_group_id.as_str(),
                        STATUS_STOPPED,
                        STATUS_STOPPED,
                    )
                    .await?;
            }
        }
        LocalExecutionBatchState::CancellationSettling(_) => {
            return Err(LocalRuntimeApiError::conflict(
                "local_execution_rerun_has_active_runs",
                "The stopped local execution batch still has active runs",
            ));
        }
        LocalExecutionBatchState::NotStopped => {
            return Err(LocalRuntimeApiError::conflict(
                "local_execution_rerun_requires_stopped_batch",
                "Only a cancelled or stopped local execution batch can be run again",
            ));
        }
    }

    let new_execution_group_id = format!("lc_execution_group_{}", Uuid::new_v4());
    let new_metadata = replacement_metadata(
        source_metadata,
        identity.execution_group_id.as_str(),
        new_execution_group_id.as_str(),
    );
    let begin_result = database
        .begin_turn(BeginLocalTurnInput {
            session_id: identity.conversation_id.clone(),
            owner_user_id: owner.owner_user_id.clone(),
            turn_id: new_execution_group_id.clone(),
            idempotency_key: new_execution_group_id.clone(),
            content: source_message.content.clone(),
            metadata_json: Some(new_metadata.to_string()),
        })
        .await?;
    let new_user_message = match begin_result {
        BeginLocalTurnResult::Started(snapshot) => snapshot.user_message,
        BeginLocalTurnResult::Existing(_) => {
            return Err(LocalRuntimeApiError::conflict(
                "local_execution_group_conflict",
                "The replacement local execution group already exists",
            ));
        }
    };
    database
        .complete_background_turn(
            owner.owner_user_id.as_str(),
            new_execution_group_id.as_str(),
        )
        .await?;

    let cloned_tasks = clone_local_execution_tasks(
        database,
        owner.owner_user_id.as_str(),
        project_id.as_str(),
        identity.conversation_id.as_str(),
        new_execution_group_id.as_str(),
        old_tasks.as_slice(),
    )
    .await?;

    let cleanup = cleanup_replaced_local_execution_batch(
        &runtime,
        owner.owner_user_id.as_str(),
        &project,
        &identity,
    )
    .await?;

    let mut started_runs = Vec::new();
    let mut work_item_ids = BTreeSet::new();
    let mut requirement_ids = BTreeSet::new();
    for task in &cloned_tasks {
        if let Some(run) = database
            .enqueue_deferred_local_conversation_task(
                owner.owner_user_id.as_str(),
                project_id.as_str(),
                task,
            )
            .await?
        {
            started_runs.push(run);
        }
        if let Some(work_item_id) = task.project_work_item_id.as_deref() {
            work_item_ids.insert(work_item_id.to_string());
        }
        if let Some(task_requirement_id) = task.requirement_id.as_deref() {
            requirement_ids.insert(task_requirement_id.to_string());
        }
    }
    for work_item_id in &work_item_ids {
        database
            .update_local_work_item(
                owner.owner_user_id.as_str(),
                work_item_id.as_str(),
                UpdateLocalWorkItemInput {
                    status: Some("in_progress".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    for task_requirement_id in &requirement_ids {
        database
            .update_local_requirement(
                owner.owner_user_id.as_str(),
                task_requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some("in_progress".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    database
        .set_turn_messages_hidden(
            owner.owner_user_id.as_str(),
            new_execution_group_id.as_str(),
            false,
        )
        .await?;

    Ok(Json(json!({
        "success": true,
        "status": STATUS_EXECUTION_STARTED,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": identity.conversation_id,
        "execution_group_id": new_execution_group_id,
        "message_id": new_user_message.id,
        "message": null,
        "task_ids": cloned_tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
        "root_task_ids": cloned_tasks.iter().filter(|task| task.prerequisite_task_ids.is_empty()).map(|task| task.id.clone()).collect::<Vec<_>>(),
        "started_runs": started_runs,
        "has_started_runs": true,
        "confirmation_status": STATUS_EXECUTION_STARTED,
        "replaced_execution_group_id": identity.execution_group_id,
        "cleanup": cleanup,
    })))
}

pub(super) async fn cleanup_replaced_local_execution_batch(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    project: &crate::local_runtime::storage::LocalProjectRecord,
    identity: &ExecutionPlanIdentity,
) -> Result<Value, LocalRuntimeApiError> {
    let database = runtime.local_database()?;
    let tasks = database
        .list_local_project_execution_tasks(owner_user_id, project.project_id.as_str())
        .await?
        .into_iter()
        .filter(|task| {
            task.execution_group_id.as_deref() == Some(identity.execution_group_id.as_str())
                && task.conversation_id == identity.conversation_id
        })
        .collect::<Vec<_>>();
    let runs = database
        .list_local_execution_group_task_runs(
            owner_user_id,
            project.project_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await?;
    if runs
        .iter()
        .any(|run| matches!(run.status.as_str(), "queued" | "running"))
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_cleanup_has_active_runs",
            "The replaced local execution batch still has active runs",
        ));
    }
    let project_root = local_project_root(runtime, project).await?;
    let cleaned_artifacts = cleanup_local_execution_artifacts(
        project_root.as_deref(),
        runs.iter().map(|run| run.id.as_str()),
    )
    .await?;
    let mut deleted_task_ids = Vec::new();
    let mut deleted_run_ids = Vec::new();
    for task in tasks {
        deleted_run_ids.extend(
            database
                .delete_local_execution_task_with_runs(
                    owner_user_id,
                    identity.conversation_id.as_str(),
                    task.id.as_str(),
                )
                .await?,
        );
        deleted_task_ids.push(task.id);
    }
    Ok(json!({
        "deleted_task_ids": deleted_task_ids,
        "deleted_run_ids": deleted_run_ids,
        "cleaned_artifacts": cleaned_artifacts,
    }))
}

pub(super) fn validate_source_scope(
    metadata: &Value,
    project_id: &str,
    requirement_id: &str,
) -> Result<(), LocalRuntimeApiError> {
    let execution = metadata.get("project_requirement_execution");
    if execution
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        != Some(project_id)
        || execution
            .and_then(|value| value.get("requirement_id"))
            .and_then(Value::as_str)
            != Some(requirement_id)
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_scope_mismatch",
            "Local execution plan does not belong to this project requirement",
        ));
    }
    Ok(())
}

pub(super) fn source_status(metadata: &Value) -> String {
    metadata
        .get("task_runner_async")
        .and_then(|value| value.get("overall_status"))
        .and_then(Value::as_str)
        .or_else(|| {
            metadata
                .get("task_runner_async")
                .and_then(|value| value.get("confirmation_status"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

pub(super) fn source_status_is_stopped_terminal(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "stopped" | "cancelled" | "canceled"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalExecutionBatchState {
    ReplacementReady,
    CancellationSettling(usize),
    NotStopped,
}

pub(super) fn resolve_local_execution_batch_state<'a>(
    source_status: &str,
    run_statuses: impl IntoIterator<Item = &'a str>,
) -> LocalExecutionBatchState {
    let normalized_run_statuses = run_statuses
        .into_iter()
        .map(|status| status.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let active_count = normalized_run_statuses
        .iter()
        .filter(|status| local_execution_run_status_is_active(status.as_str()))
        .count();
    if active_count > 0 {
        return LocalExecutionBatchState::CancellationSettling(active_count);
    }

    let normalized_source_status = source_status.trim().to_ascii_lowercase();
    if source_status_is_stopped_terminal(normalized_source_status.as_str())
        || normalized_source_status == STATUS_STOPPING
        || inactive_runs_record_a_cancelled_batch(normalized_run_statuses.as_slice())
    {
        LocalExecutionBatchState::ReplacementReady
    } else {
        LocalExecutionBatchState::NotStopped
    }
}

fn local_execution_run_status_is_active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

fn inactive_runs_record_a_cancelled_batch(statuses: &[String]) -> bool {
    !statuses.is_empty()
        && statuses
            .iter()
            .all(|status| !local_execution_run_status_is_active(status.as_str()))
        && statuses
            .iter()
            .any(|status| matches!(status.as_str(), "cancelled" | "canceled"))
}

fn replacement_metadata(
    mut metadata: Value,
    replaced_execution_group_id: &str,
    new_execution_group_id: &str,
) -> Value {
    if !metadata.is_object() {
        metadata = json!({});
    }
    let map = metadata
        .as_object_mut()
        .expect("metadata normalized as object");
    map.insert("hidden".to_string(), Value::Bool(false));
    map.insert(
        "conversation_turn_id".to_string(),
        Value::String(new_execution_group_id.to_string()),
    );
    let execution = map
        .entry("project_requirement_execution".to_string())
        .or_insert_with(|| json!({}));
    if !execution.is_object() {
        *execution = json!({});
    }
    if let Some(execution) = execution.as_object_mut() {
        execution.insert(
            "execution_group_id".to_string(),
            Value::String(new_execution_group_id.to_string()),
        );
        execution.insert(
            "replaced_execution_group_id".to_string(),
            Value::String(replaced_execution_group_id.to_string()),
        );
    }
    let async_meta = map
        .entry("task_runner_async".to_string())
        .or_insert_with(|| json!({}));
    if !async_meta.is_object() {
        *async_meta = json!({});
    }
    if let Some(async_meta) = async_meta.as_object_mut() {
        async_meta.insert(
            "overall_status".to_string(),
            Value::String(STATUS_EXECUTION_STARTED.to_string()),
        );
        async_meta.insert(
            "confirmation_status".to_string(),
            Value::String(STATUS_EXECUTION_STARTED.to_string()),
        );
        async_meta.insert(
            "source_turn_id".to_string(),
            Value::String(new_execution_group_id.to_string()),
        );
        async_meta.insert("created_task_ids".to_string(), json!([]));
        async_meta.insert("running_task_ids".to_string(), json!([]));
        async_meta.insert("terminal_task_ids".to_string(), json!([]));
    }
    metadata
}

async fn clone_local_execution_tasks(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    project_id: &str,
    conversation_id: &str,
    new_execution_group_id: &str,
    old_tasks: &[crate::local_runtime::task_board::LocalTaskBoardTaskRecord],
) -> Result<Vec<crate::local_runtime::task_board::LocalTaskBoardTaskRecord>, LocalRuntimeApiError> {
    let old_ids = old_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();
    let mut pending = old_tasks.to_vec();
    let mut new_id_by_old_id = BTreeMap::<String, String>::new();
    let mut cloned = Vec::new();
    while !pending.is_empty() {
        let ready_index = pending.iter().position(|task| {
            task.prerequisite_task_ids.iter().all(|prerequisite_id| {
                !old_ids.contains(prerequisite_id) || new_id_by_old_id.contains_key(prerequisite_id)
            })
        });
        let Some(index) = ready_index else {
            rollback_local_clones(database, owner_user_id, conversation_id, cloned.as_slice())
                .await;
            return Err(LocalRuntimeApiError::conflict(
                "local_execution_graph_cycle",
                "The stopped local execution graph contains a cycle",
            ));
        };
        let old = pending.remove(index);
        let model_config_id = old
            .model_config_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                LocalRuntimeApiError::conflict(
                    "local_task_runner_model_required",
                    "A copied local execution task has no model configuration",
                )
            })?;
        let prerequisites = old
            .prerequisite_task_ids
            .iter()
            .map(|prerequisite_id| {
                new_id_by_old_id
                    .get(prerequisite_id)
                    .cloned()
                    .unwrap_or_else(|| prerequisite_id.clone())
            })
            .collect::<Vec<_>>();
        let created = database
            .create_local_conversation_task(CreateLocalConversationTaskInput {
                owner_user_id: owner_user_id.to_string(),
                project_id: project_id.to_string(),
                session_id: conversation_id.to_string(),
                source_turn_id: new_execution_group_id.to_string(),
                title: old.title.clone(),
                description: old.details.clone(),
                objective: old.objective.clone(),
                priority: priority_value(old.priority.as_str()),
                tags: old.tags.clone(),
                model_config_id,
                is_planning_task: old.is_planning_task,
                enabled_builtin_kinds: old.enabled_builtin_kinds.clone(),
                external_mcp_config_ids: old.external_mcp_config_ids.clone(),
                selected_skill_ids: old.selected_skill_ids.clone(),
                prerequisite_task_ids: prerequisites,
                project_work_item_id: old.project_work_item_id.clone(),
                requirement_id: old.requirement_id.clone(),
                execution_group_id: Some(new_execution_group_id.to_string()),
                execution_client_ref: old.execution_client_ref.clone(),
                dependency_context_refs: old.dependency_context_refs.clone(),
                defer_execution: true,
            })
            .await?;
        new_id_by_old_id.insert(old.id, created.id.clone());
        cloned.push(created);
    }
    Ok(cloned)
}

async fn rollback_local_clones(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    conversation_id: &str,
    cloned: &[crate::local_runtime::task_board::LocalTaskBoardTaskRecord],
) {
    for task in cloned.iter().rev() {
        let _ = database
            .delete_local_execution_task_with_runs(owner_user_id, conversation_id, task.id.as_str())
            .await;
    }
}

fn priority_value(priority: &str) -> i64 {
    match priority.trim() {
        "high" => 10,
        "low" => -10,
        _ => 0,
    }
}

async fn local_project_root(
    runtime: &LocalRuntime,
    project: &crate::local_runtime::storage::LocalProjectRecord,
) -> Result<Option<PathBuf>, LocalRuntimeApiError> {
    let state = runtime.state.read().await;
    let Some(workspace) = state.workspace_by_id(project.workspace_id.as_str()) else {
        return Ok(None);
    };
    let workspace_root = workspace.absolute_root.canonicalize().map_err(|error| {
        LocalRuntimeApiError::bad_request("local_workspace_invalid", error.to_string())
    })?;
    let relative =
        normalize_relative_workspace_path(project.root_relative_path.as_deref().unwrap_or("."))
            .map_err(|error| {
                LocalRuntimeApiError::bad_request("local_project_path_invalid", error.to_string())
            })?;
    let candidate = if relative == "." {
        workspace_root.clone()
    } else {
        workspace_root.join(relative)
    };
    let root = candidate.canonicalize().map_err(|error| {
        LocalRuntimeApiError::bad_request("local_project_path_invalid", error.to_string())
    })?;
    if !root.starts_with(workspace_root.as_path()) {
        return Err(LocalRuntimeApiError::bad_request(
            "local_project_path_invalid",
            "Local project path escapes its registered workspace",
        ));
    }
    Ok(Some(root))
}

async fn cleanup_local_execution_artifacts<'a>(
    project_root: Option<&Path>,
    run_ids: impl Iterator<Item = &'a str>,
) -> Result<Vec<String>, LocalRuntimeApiError> {
    let mut cleaned = Vec::new();
    for run_id in run_ids {
        cleaned.extend(cleanup_local_temp_dirs(run_id)?);
        if let Some(project_root) = project_root {
            cleaned.extend(cleanup_local_git_run_branch(project_root, run_id).await?);
        }
    }
    Ok(cleaned)
}

fn cleanup_local_temp_dirs(run_id: &str) -> Result<Vec<String>, LocalRuntimeApiError> {
    let component = normalize_run_component(run_id);
    let prefixes = [
        format!("chatos-local-task-run-{component}-"),
        format!("chatos-local-run-{component}-"),
    ];
    let mut cleaned = Vec::new();
    for entry in std::fs::read_dir(std::env::temp_dir()).map_err(|error| {
        LocalRuntimeApiError::internal(format!("local execution cleanup failed: {error}"))
    })? {
        let entry = entry.map_err(|error| {
            LocalRuntimeApiError::internal(format!("local execution cleanup failed: {error}"))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path.as_path()).map_err(|error| {
                LocalRuntimeApiError::internal(format!("local execution cleanup failed: {error}"))
            })?;
            cleaned.push(format!("directory:{}", path.display()));
        }
    }
    Ok(cleaned)
}

async fn cleanup_local_git_run_branch(
    project_root: &Path,
    run_id: &str,
) -> Result<Vec<String>, LocalRuntimeApiError> {
    if !project_root.join(".git").exists() {
        return Ok(Vec::new());
    }
    let branch = format!("chatos/runs/{}", normalize_run_component(run_id));
    let branch_ref = format!("refs/heads/{branch}");
    let mut cleaned = Vec::new();
    let worktrees = git_output(project_root, &["worktree", "list", "--porcelain"]).await?;
    let mut current_path: Option<PathBuf> = None;
    for line in worktrees.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
        } else if line == format!("branch {branch_ref}") {
            if let Some(path) = current_path.take() {
                let canonical = path.canonicalize().unwrap_or(path.clone());
                if canonical != project_root {
                    git_status(
                        project_root,
                        &[
                            "worktree",
                            "remove",
                            "--force",
                            path.to_string_lossy().as_ref(),
                        ],
                    )
                    .await?;
                    cleaned.push(format!("worktree:{}", path.display()));
                }
            }
        } else if line.is_empty() {
            current_path = None;
        }
    }
    if git_success(
        project_root,
        &["show-ref", "--verify", "--quiet", branch_ref.as_str()],
    )
    .await?
    {
        git_status(project_root, &["branch", "-D", branch.as_str()]).await?;
        cleaned.push(format!("branch:{branch}"));
    }
    if git_success(
        project_root,
        &[
            "ls-remote",
            "--exit-code",
            "--heads",
            "origin",
            branch_ref.as_str(),
        ],
    )
    .await?
    {
        git_status(
            project_root,
            &["push", "origin", "--delete", branch.as_str()],
        )
        .await?;
        cleaned.push(format!("remote-branch:{branch}"));
    }
    Ok(cleaned)
}

async fn git_output(root: &Path, args: &[&str]) -> Result<String, LocalRuntimeApiError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| {
            LocalRuntimeApiError::internal(format!("local git cleanup failed: {error}"))
        })?;
    if !output.status.success() {
        return Err(LocalRuntimeApiError::internal(format!(
            "local git cleanup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn git_success(root: &Path, args: &[&str]) -> Result<bool, LocalRuntimeApiError> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .map_err(|error| {
            LocalRuntimeApiError::internal(format!("local git cleanup failed: {error}"))
        })
}

async fn git_status(root: &Path, args: &[&str]) -> Result<(), LocalRuntimeApiError> {
    git_output(root, args).await.map(|_| ())
}

fn normalize_run_component(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_cancelled_source_status_is_terminal_for_replacement() {
        let metadata = json!({
            "task_runner_async": {
                "overall_status": "cancelled",
                "confirmation_status": "cancelled"
            }
        });

        let status = source_status(&metadata);
        assert_eq!(status, "cancelled");
        assert!(source_status_is_stopped_terminal(status.as_str()));
        assert!(source_status_is_stopped_terminal("canceled"));
        assert!(source_status_is_stopped_terminal("stopped"));
        assert!(!source_status_is_stopped_terminal("stopping"));
    }

    #[test]
    fn local_cancelled_runs_recover_replacement_readiness_when_source_status_is_stale() {
        assert_eq!(
            resolve_local_execution_batch_state("failed", ["done", "cancelled"]),
            LocalExecutionBatchState::ReplacementReady
        );
    }

    #[test]
    fn local_active_runs_keep_replacement_in_cancellation_settling() {
        assert_eq!(
            resolve_local_execution_batch_state("stopped", ["running", "cancelled"]),
            LocalExecutionBatchState::CancellationSettling(1)
        );
    }

    #[test]
    fn local_failed_runs_without_stop_intent_are_not_replacement_ready() {
        assert_eq!(
            resolve_local_execution_batch_state("failed", ["failed"]),
            LocalExecutionBatchState::NotStopped
        );
    }
}
