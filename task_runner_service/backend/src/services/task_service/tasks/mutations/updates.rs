// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::normalize_task_profile;

impl TaskService {
    pub async fn update_task(
        &self,
        id: &str,
        patch: UpdateTaskRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };

        if let Some(title) = patch.title {
            validate_required("title", &title)?;
            task.title = title.trim().to_string();
        }
        if let Some(description) = patch.description {
            task.description = normalized_optional(Some(description));
        }
        if let Some(objective) = patch.objective {
            validate_required("objective", &objective)?;
            task.objective = objective.trim().to_string();
        }
        if let Some(input_payload) = patch.input_payload {
            task.input_payload = Some(input_payload);
        }
        if let Some(status) = patch.status {
            if matches!(status, TaskStatus::Queued | TaskStatus::Running) {
                return Err(
                    "任务排队/运行状态由系统维护，请通过执行任务进入 queued 或 running".to_string(),
                );
            }
            if status == TaskStatus::Cancelled {
                return Err("请使用 cancel_task 并提供取消原因".to_string());
            }
            if self.store.has_active_run_for_task(id).await? {
                return Err("任务仍有运行中的执行记录，请先取消或等待完成".to_string());
            }
            if status != task.status {
                ensure_subtask_can_be_marked_unfinished(&self.store, &task, status).await?;
            }
            if status == TaskStatus::Succeeded {
                ensure_task_has_no_unfinished_subtasks(&self.store, &task).await?;
            }
            task.status = status;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(tags) = patch.tags {
            task.tags = normalize_tags(Some(tags));
        }
        if let Some(model_config_id) = patch.default_model_config_id {
            let model_config_id = model_config_id.trim().to_string();
            if !model_config_id.is_empty() {
                self.ensure_model_config_access(&model_config_id, current_user)
                    .await?;
                task.default_model_config_id = Some(model_config_id);
            } else {
                task.default_model_config_id = None;
            }
        }
        if let Some(task_profile) = patch.task_profile {
            task.task_profile = normalize_task_profile(Some(task_profile.as_str()))?;
        }
        if let Some(schedule) = patch.schedule {
            task.schedule = sanitize_task_schedule_config(schedule, Some(&task.schedule))?;
        }
        if let Some(mcp_config) = patch.mcp_config {
            task.mcp_config = sanitize_task_mcp_config(mcp_config);
            let task_owner_user_id = task_owner_or_creator(&task);
            self.validate_task_mcp_config(&task.mcp_config, current_user, task_owner_user_id)
                .await?;
        }
        let prerequisite_task_ids = patch
            .prerequisite_task_ids
            .map(normalize_prerequisite_task_ids);
        if let Some(prerequisite_task_ids) = prerequisite_task_ids.as_ref() {
            self.validate_task_prerequisites(id, prerequisite_task_ids, current_user)
                .await?;
            task.prerequisite_task_ids = prerequisite_task_ids.clone();
        }
        align_task_tenant_to_owner(&mut task);
        task.updated_at = now_rfc3339();
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        if let Some(prerequisite_task_ids) = prerequisite_task_ids {
            self.store
                .set_task_prerequisites(id, prerequisite_task_ids)
                .await?;
        }
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn record_task_process(
        &self,
        id: &str,
        input: RecordTaskProcessRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        task.process_log = apply_task_process_log_update(task.process_log, input, now.as_str())?;
        task.updated_at = now;
        let saved = self.store.save_task(task).await?;
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn update_task_mcp(
        &self,
        id: &str,
        patch: UpdateTaskMcpRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        if let Some(enabled) = patch.enabled {
            task.mcp_config.enabled = enabled;
        }
        if let Some(init_mode) = patch.init_mode {
            task.mcp_config.init_mode = init_mode;
        }
        if let Some(prompt_mode) = patch.builtin_prompt_mode {
            task.mcp_config.builtin_prompt_mode = prompt_mode;
        }
        if let Some(prompt_locale) = patch.builtin_prompt_locale {
            let normalized = prompt_locale.trim();
            if !normalized.is_empty() {
                task.mcp_config.builtin_prompt_locale = normalized.to_string();
            }
        }
        if let Some(kinds) = patch.enabled_builtin_kinds {
            task.mcp_config.enabled_builtin_kinds = normalize_builtin_kind_names(kinds);
        }
        if let Some(workspace_dir) = patch.workspace_dir {
            task.mcp_config.workspace_dir = normalized_optional(Some(workspace_dir));
        }
        if let Some(default_remote_server_id) = patch.default_remote_server_id {
            task.mcp_config.default_remote_server_id =
                normalized_optional(Some(default_remote_server_id));
        }
        if let Some(external_mcp_config_ids) = patch.external_mcp_config_ids {
            task.mcp_config.external_mcp_config_ids = external_mcp_config_ids;
        }
        if let Some(skill_ids) = patch.skill_ids {
            task.mcp_config.skill_ids = skill_ids;
        }
        task.mcp_config = sanitize_task_mcp_config(task.mcp_config);
        let task_owner_user_id = task_owner_or_creator(&task);
        self.validate_task_mcp_config(&task.mcp_config, current_user, task_owner_user_id)
            .await?;
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }
}

fn task_owner_or_creator(task: &TaskRecord) -> Option<&str> {
    task.owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            task.creator_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-update-test".to_string(),
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
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
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

    async fn create_subtask(
        service: &TaskService,
        parent: &TaskRecord,
        title: &str,
        status: TaskStatus,
    ) -> TaskRecord {
        let mut child = create_task(service, title, status).await;
        child.parent_task_id = Some(parent.id.clone());
        service.store.save_task(child).await.expect("save child")
    }

    #[tokio::test]
    async fn update_task_rejects_succeeded_parent_when_subtask_unfinished() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        create_subtask(&service, &parent, "child", TaskStatus::Ready).await;

        let err = service
            .update_task(
                parent.id.as_str(),
                UpdateTaskRequest {
                    status: Some(TaskStatus::Succeeded),
                    ..UpdateTaskRequest::default()
                },
                None,
            )
            .await
            .expect_err("parent should not succeed with unfinished child");

        assert!(err.contains("还有未完成子任务"));
        let parent_after = service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(parent_after.status, TaskStatus::Ready);
    }

    #[tokio::test]
    async fn reopening_subtask_after_parent_success_is_rejected() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = create_subtask(&service, &parent, "child", TaskStatus::Succeeded).await;
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
            .expect("parent can succeed when child succeeded");

        let err = service
            .update_task(
                child.id.as_str(),
                UpdateTaskRequest {
                    status: Some(TaskStatus::Blocked),
                    ..UpdateTaskRequest::default()
                },
                None,
            )
            .await
            .expect_err("child cannot be reopened after parent succeeded");
        assert!(err.contains("已经成功"));

        let parent_after = service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(parent_after.status, TaskStatus::Succeeded);
        let child_after = service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(child_after.status, TaskStatus::Succeeded);
    }
}
