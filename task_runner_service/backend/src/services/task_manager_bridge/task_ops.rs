// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{
    apply_manager_patch, shared_outcome_items_into_tool_state, task_belongs_to_context,
    task_manager_status_from_task_status, task_priority_from_manager_label,
    task_status_from_manager_status,
};
use super::*;

impl TaskService {
    pub(super) async fn create_followup_task_for_tool(
        &self,
        root_task_id: &str,
        run_id: &str,
        draft: SharedTaskDraft,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &draft.title)?;
        let Some(parent) = self.store.get_task(root_task_id).await? else {
            warn!(
                root_task_id,
                source_run_id = run_id,
                draft_title = draft.title.as_str(),
                "task manager could not find root task for follow-up task creation"
            );
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        let parent = save_task_if_tenant_aligned(&self.store, parent).await?;
        if parent.status == TaskStatus::Succeeded {
            return Err(format!(
                "父任务「{}」已经成功，不能再新增子任务。",
                parent.title.trim()
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let prerequisite_task_ids = tool_prerequisite_task_ids(&draft);
        self.validate_tool_prerequisite_task_ids(
            root_task_id,
            &id,
            &prerequisite_task_ids,
            parent.project_id.as_str(),
        )
        .await?;
        let title = draft.title.trim().to_string();
        let description = normalized_optional(Some(draft.details));
        let objective = description.clone().unwrap_or_else(|| title.clone());
        let result_summary = normalized_optional(Some(draft.outcome_summary));
        let status = task_status_from_manager_status(draft.status.as_str());
        let mut task_tool_state = TaskToolState {
            due_at: normalized_optional_nested(draft.due_at),
            outcome_items: shared_outcome_items_into_tool_state(draft.outcome_items),
            resume_hint: normalized_optional(Some(draft.resume_hint)),
            blocker_reason: normalized_optional(Some(draft.blocker_reason)),
            blocker_needs: normalize_strings(draft.blocker_needs),
            blocker_kind: normalized_optional(Some(draft.blocker_kind)),
            completed_at: None,
            last_outcome_at: None,
            ..TaskToolState::default()
        };
        if result_summary.is_some() || !task_tool_state.outcome_items.is_empty() {
            task_tool_state.last_outcome_at = Some(now.clone());
        }
        if task_manager_status_from_task_status(status) == "done" {
            task_tool_state.blocker_reason = None;
            task_tool_state.blocker_needs.clear();
            task_tool_state.blocker_kind = None;
            task_tool_state.completed_at = Some(now.clone());
        }

        let mut input_payload = tool_subtask_input_payload(&parent);
        let mut mcp_config = tool_subtask_mcp_config(&parent);
        let project_root = match project_root_from_payload(input_payload.as_ref())
            .or_else(|| project_root_from_payload(parent.input_payload.as_ref()))
        {
            Some(value) => Some(value),
            None => {
                resolve_project_root_for_project_id(
                    &self.config,
                    &self.store,
                    parent.project_id.as_str(),
                )
                .await?
            }
        };
        if let Some(project_root) = project_root {
            let local_kinds = selected_local_connector_builtin_kinds_for_config(
                &mcp_config,
                parent.task_profile.as_str(),
            );
            apply_local_connector_routing(
                &mut mcp_config,
                &mut input_payload,
                project_root.as_str(),
                local_kinds.as_slice(),
            );
        }

        let task = TaskRecord {
            id: id.clone(),
            title,
            description,
            objective,
            input_payload,
            status,
            priority: task_priority_from_manager_label(draft.priority.as_str()),
            tags: normalize_strings(draft.tags),
            default_model_config_id: None,
            memory_thread_id: format!("task-subtask-{id}"),
            tenant_id: parent.tenant_id.clone(),
            subject_id: parent.subject_id.clone(),
            project_id: parent.project_id.clone(),
            task_profile: parent.task_profile.clone(),
            creator_user_id: parent.creator_user_id.clone(),
            creator_username: parent.creator_username.clone(),
            creator_display_name: parent.creator_display_name.clone(),
            owner_user_id: parent.owner_user_id.clone(),
            owner_username: parent.owner_username.clone(),
            owner_display_name: parent.owner_display_name.clone(),
            result_summary,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: Some(parent.id.clone()),
            source_run_id: Some(run_id.to_string()),
            source_session_id: parent.source_session_id.clone(),
            source_turn_id: parent.source_turn_id.clone(),
            source_user_message_id: parent.source_user_message_id.clone(),
            prerequisite_task_ids: prerequisite_task_ids.clone(),
            task_tool_state,
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        let saved = self.store.save_task(task).await?;
        if !prerequisite_task_ids.is_empty() {
            self.store
                .set_task_prerequisites(&id, prerequisite_task_ids)
                .await?;
        }
        let saved = self.hydrate_task_prerequisites(saved).await?;
        info!(
            root_task_id,
            source_run_id = run_id,
            created_task_id = saved.id.as_str(),
            created_task_title = saved.title.as_str(),
            created_task_status = saved.status.status_string(),
            "task manager auto-created follow-up task during task run"
        );
        Ok(saved)
    }

    pub(super) async fn list_tool_tasks(
        &self,
        root_task_id: &str,
        source_run_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        if self.store.get_task(root_task_id).await?.is_none() {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        let mut tasks = self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .filter(|task| task_belongs_to_context(task, root_task_id))
            .collect::<Vec<_>>();
        if let Some(run_id) = source_run_id {
            tasks.retain(|task| task.source_run_id.as_deref() == Some(run_id));
        }
        if !include_done {
            tasks.retain(|task| task_manager_status_from_task_status(task.status) != "done");
        }
        tasks.sort_by(|left, right| {
            if left.id == root_task_id && right.id != root_task_id {
                std::cmp::Ordering::Less
            } else if right.id == root_task_id && left.id != root_task_id {
                std::cmp::Ordering::Greater
            } else {
                right.updated_at.cmp(&left.updated_at)
            }
        });
        tasks.truncate(limit);
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub(super) async fn update_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        let previous_status = task.status;
        apply_manager_patch(&mut task, patch, false, now.as_str())?;
        if task.parent_task_id.is_none()
            && previous_status == TaskStatus::Succeeded
            && task.status != TaskStatus::Succeeded
        {
            return Err(format!(
                "任务「{}」已经成功，不能再改为 {}。",
                task.title.trim(),
                task.status.status_string()
            ));
        }
        if task.status != previous_status {
            ensure_subtask_can_be_marked_unfinished(&self.store, &task, task.status).await?;
        }
        if task.status == TaskStatus::Succeeded {
            ensure_task_has_no_unfinished_subtasks(&self.store, &task).await?;
        }
        align_task_tenant_to_owner(&mut task);
        task.updated_at = now;
        if task.parent_task_id.is_none() {
            self.ensure_task_thread(&task).await?;
        }
        self.store.save_task(task).await
    }

    pub(super) async fn complete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        if let Some(patch) = patch {
            apply_manager_patch(&mut task, patch, true, now.as_str())?;
        } else {
            task.status = TaskStatus::Succeeded;
            task.task_tool_state.blocker_reason = None;
            task.task_tool_state.blocker_needs.clear();
            task.task_tool_state.blocker_kind = None;
            task.task_tool_state.completed_at = Some(now.clone());
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.status = TaskStatus::Succeeded;
        ensure_task_has_no_unfinished_subtasks(&self.store, &task).await?;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.clone());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        align_task_tenant_to_owner(&mut task);
        task.updated_at = now;
        if task.parent_task_id.is_none() {
            self.ensure_task_thread(&task).await?;
        }
        self.store.save_task(task).await
    }

    pub(super) async fn delete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        if task_id == root_task_id {
            return Err("不能删除当前正在执行的根任务".to_string());
        }
        let Some(task) = self.store.get_task(task_id).await? else {
            return Ok(false);
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Ok(false);
        }
        if self.store.has_active_run_for_task(task_id).await? {
            return Err("任务仍有运行中的执行记录，暂时不能删除".to_string());
        }
        self.store.delete_task(task_id).await
    }

    async fn validate_tool_prerequisite_task_ids(
        &self,
        root_task_id: &str,
        task_id: &str,
        prerequisite_task_ids: &[String],
        expected_project_id: &str,
    ) -> Result<(), String> {
        self.validate_task_prerequisites_for_project(
            task_id,
            prerequisite_task_ids,
            None,
            Some(expected_project_id),
        )
        .await?;
        for prerequisite_task_id in prerequisite_task_ids {
            if prerequisite_task_id == root_task_id {
                return Err("前置任务不能是当前正在执行的父任务".to_string());
            }
            let Some(prerequisite) = self.store.get_task(prerequisite_task_id).await? else {
                return Err(format!("前置任务不存在: {prerequisite_task_id}"));
            };
            if !task_belongs_to_context(&prerequisite, root_task_id) {
                return Err(format!(
                    "前置任务不属于当前内部任务上下文: {prerequisite_task_id}"
                ));
            }
        }
        Ok(())
    }
}

fn tool_prerequisite_task_ids(draft: &SharedTaskDraft) -> Vec<String> {
    let mut ids = draft.prerequisite_task_ids.clone();
    if let Some(id) = draft.prerequisite_task_id.clone() {
        ids.push(id);
    }
    normalize_prerequisite_task_ids(ids)
}

fn disabled_tool_subtask_mcp_config() -> TaskMcpConfig {
    TaskMcpConfig {
        enabled: false,
        enabled_builtin_kinds: Vec::new(),
        external_mcp_config_ids: Vec::new(),
        workspace_dir: None,
        default_remote_server_id: None,
        ..TaskMcpConfig::default()
    }
}

fn tool_subtask_mcp_config(parent: &TaskRecord) -> TaskMcpConfig {
    if !parent.mcp_config.enabled {
        return disabled_tool_subtask_mcp_config();
    }
    let mut config = parent.mcp_config.clone();
    config.default_remote_server_id = None;
    config
}

fn tool_subtask_input_payload(parent: &TaskRecord) -> Option<Value> {
    let project_root = project_root_from_payload(parent.input_payload.as_ref())?;
    Some(json!({ "project_root": project_root }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::{CreateTaskRequest, UpdateTaskRequest};
    use crate::store::AppStore;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-manager-bridge-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            local_connector_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> TaskService {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        TaskService::new(config, store)
    }

    async fn create_task(service: &TaskService, title: &str, status: TaskStatus) -> TaskRecord {
        service
            .create_task(
                CreateTaskRequest {
                    title: title.to_string(),
                    description: None,
                    objective: format!("do {title}"),
                    input_payload: None,
                    status: Some(status),
                    priority: None,
                    tags: None,
                    default_model_config_id: None,
                    project_id: None,
                    task_profile: None,
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    mcp_config: None,
                    prerequisite_task_ids: None,
                },
                None,
                None,
            )
            .await
            .expect("create task")
    }

    fn task_draft(title: &str, status: &str) -> SharedTaskDraft {
        SharedTaskDraft {
            title: title.to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: status.to_string(),
            tags: Vec::new(),
            prerequisite_task_id: None,
            prerequisite_task_ids: Vec::new(),
            due_at: None,
            outcome_summary: String::new(),
            outcome_items: Vec::new(),
            resume_hint: String::new(),
            blocker_reason: String::new(),
            blocker_needs: Vec::new(),
            blocker_kind: String::new(),
        }
    }

    #[tokio::test]
    async fn create_followup_task_rejects_succeeded_parent() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Succeeded).await;

        let err = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "todo"))
            .await
            .expect_err("completed parent should not accept new subtasks");

        assert!(err.contains("不能再新增子任务"));
    }

    #[tokio::test]
    async fn done_followup_task_clears_blocker_metadata() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let mut draft = task_draft("child", "done");
        draft.blocker_reason = "waiting".to_string();
        draft.blocker_needs = vec!["input".to_string()];
        draft.blocker_kind = "unknown".to_string();

        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft)
            .await
            .expect("create done child");

        assert_eq!(child.status, TaskStatus::Succeeded);
        assert_eq!(child.task_tool_state.blocker_reason, None);
        assert!(child.task_tool_state.blocker_needs.is_empty());
        assert_eq!(child.task_tool_state.blocker_kind, None);
    }

    #[tokio::test]
    async fn followup_task_inherits_local_connector_mcp_from_parent() {
        let service = test_service().await;
        let mut parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let project_root = "local://connector/device-1/workspace-1/project-a";
        parent.project_id = "project-local".to_string();
        parent.owner_user_id = Some("owner-1".to_string());
        parent.input_payload = Some(json!({ "project_root": project_root }));
        parent.mcp_config = TaskMcpConfig {
            enabled: true,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "TaskManager".to_string(),
            ],
            ..TaskMcpConfig::default()
        };
        let parent = service.store.save_task(parent).await.expect("save parent");

        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "todo"))
            .await
            .expect("create child");

        assert_eq!(
            child
                .input_payload
                .as_ref()
                .and_then(|value| value.get("project_root"))
                .and_then(Value::as_str),
            Some(project_root)
        );
        assert!(child.mcp_config.enabled);
        assert!(child
            .mcp_config
            .enabled_builtin_kinds
            .iter()
            .all(|kind| kind != "CodeMaintainerRead"));
        let server = child
            .mcp_config
            .ephemeral_http_servers
            .iter()
            .find(|server| server.name == "local_connector")
            .expect("local connector server");
        assert!(server
            .url
            .contains("/api/local-connectors/relay/device-1/mcp"));
        assert!(server.url.contains("workspace_id=workspace-1"));
        assert_eq!(
            server.auth_mode.as_deref(),
            Some(crate::models::TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
        );
    }

    #[tokio::test]
    async fn completing_task_clears_blocker_metadata() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let mut draft = task_draft("child", "blocked");
        draft.blocker_reason = "waiting".to_string();
        draft.blocker_needs = vec!["input".to_string()];
        draft.blocker_kind = "unknown".to_string();
        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft)
            .await
            .expect("create blocked child");

        let completed = service
            .complete_task_from_tool(parent.id.as_str(), child.id.as_str(), None)
            .await
            .expect("complete child");

        assert_eq!(completed.status, TaskStatus::Succeeded);
        assert_eq!(completed.task_tool_state.blocker_reason, None);
        assert!(completed.task_tool_state.blocker_needs.is_empty());
        assert_eq!(completed.task_tool_state.blocker_kind, None);
    }

    #[tokio::test]
    async fn update_task_from_tool_rejects_reopening_subtask_after_parent_success() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "done"))
            .await
            .expect("create done child");
        service
            .update_task(
                parent.id.as_str(),
                UpdateTaskRequest {
                    status: Some(TaskStatus::Succeeded),
                    ..UpdateTaskRequest::default()
                },
                None,
            )
            .await
            .expect("complete parent");

        let err = service
            .update_task_from_tool(
                parent.id.as_str(),
                child.id.as_str(),
                SharedTaskUpdatePatch {
                    status: Some("todo".to_string()),
                    ..SharedTaskUpdatePatch::default()
                },
            )
            .await
            .expect_err("subtask should not reopen");

        assert!(err.contains("已经成功"));
        let child_after = service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(child_after.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn update_task_from_tool_rejects_reopening_succeeded_root_task() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Succeeded).await;

        let err = service
            .update_task_from_tool(
                parent.id.as_str(),
                parent.id.as_str(),
                SharedTaskUpdatePatch {
                    status: Some("doing".to_string()),
                    ..SharedTaskUpdatePatch::default()
                },
            )
            .await
            .expect_err("root task should not reopen");

        assert!(err.contains("已经成功"));
        let parent_after = service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(parent_after.status, TaskStatus::Succeeded);
    }
}
