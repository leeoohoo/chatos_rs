// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::normalize_task_profile;
use crate::services::status_display::TaskScheduleModeExt;
use crate::services::task_manager_lifecycle::{apply_task_closure, task_has_manager_lifecycle};

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

        let mut capability_boundary_changed = false;

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
            if task_has_manager_lifecycle(&task) {
                let now = now_rfc3339();
                match status {
                    TaskStatus::Succeeded => apply_task_closure(
                        &mut task,
                        TaskClosureState::Satisfied,
                        None,
                        now.as_str(),
                    )?,
                    TaskStatus::Archived => apply_task_closure(
                        &mut task,
                        TaskClosureState::Superseded,
                        Some("任务已归档，不再阻止所属运行完成".to_string()),
                        now.as_str(),
                    )?,
                    TaskStatus::Draft
                    | TaskStatus::Ready
                    | TaskStatus::Queued
                    | TaskStatus::Running
                    | TaskStatus::Failed
                    | TaskStatus::Blocked
                    | TaskStatus::Cancelled => {
                        task.task_tool_state.closure_state = Some(TaskClosureState::Open);
                        task.task_tool_state.closure_reason = None;
                        task.task_tool_state.completed_at = None;
                        task.task_tool_state.lifecycle_updated_at = Some(now);
                    }
                }
            }
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
            capability_boundary_changed = true;
        }
        if let Some(schedule) = patch.schedule {
            task.schedule = sanitize_task_schedule_config(schedule, Some(&task.schedule))?;
        }
        if let Some(mcp_config) = patch.mcp_config {
            if !mcp_config.enabled_builtin_kinds.is_empty()
                || !mcp_config.external_mcp_config_ids.is_empty()
            {
                return Err(
                    "任务 MCP 选择在创建时由 Agent 固化，不能通过普通任务编辑修改".to_string(),
                );
            }
            if let Some(requires_execution) = mcp_config.requires_execution {
                capability_boundary_changed |=
                    task.mcp_config.requires_execution != requires_execution;
                task.mcp_config.requires_execution = requires_execution;
            }
            if let Some(workspace_changes_required) = mcp_config.workspace_changes_required {
                task.mcp_config.workspace_changes_required = workspace_changes_required;
            }
        }
        if patch.plugin_config.is_some() {
            return Err(
                "Task Plugin selection is frozen at creation and cannot be changed by task editing"
                    .to_string(),
            );
        }
        if capability_boundary_changed {
            let task_owner_user_id = task_owner_or_creator(&task);
            let agent_key = chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase;
            let _ = self
                .validate_task_mcp_config_for_agent(
                    &task.mcp_config,
                    &task.plugin_config,
                    task.project_id.as_deref(),
                    task.project_context.as_ref(),
                    current_user,
                    task_owner_user_id,
                    agent_key,
                    task.task_profile.as_str(),
                    task.schedule.mode.mode_key(),
                )
                .await?;
            if let Some(policy) = self
                .resolve_task_runner_policy_for_agent_project(
                    current_user,
                    task_owner_user_id,
                    agent_key,
                    task.project_id.as_deref(),
                    task.project_context.as_ref(),
                    Some(task.task_profile.as_str()),
                    Some(task.schedule.mode.mode_key()),
                )
                .await?
            {
                task.mcp_config.skill_policy_revision = Some(policy.policy_revision().to_string());
            }
        }
        let prerequisite_task_ids = patch
            .prerequisite_task_ids
            .map(normalize_prerequisite_task_ids);
        if let Some(prerequisite_task_ids) = prerequisite_task_ids.as_ref() {
            self.validate_task_prerequisites_for_project(
                id,
                prerequisite_task_ids,
                current_user,
                task.project_id.as_deref(),
            )
            .await?;
            task.prerequisite_task_ids = prerequisite_task_ids.clone();
        }
        if task.project_id.is_some() || task.project_context.is_some() {
            task.validate_project_context()?;
        } else {
            align_task_tenant_to_owner(&mut task);
        }
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
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-update-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            memory_engine_http_client: reqwest::Client::new(),
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            chatos_callback_url: String::new(),
            chatos_callback_http_client: reqwest::Client::new(),
            chatos_internal_api_secret: None,
            mcp_management_internal_api_secret: None,
            user_service_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
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
                    project_context: None,
                    task_profile: None,
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    plugin_config: Default::default(),
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
        child.task_tool_state.required_for_parent_completion = Some(true);
        child.task_tool_state.closure_state = Some(if status == TaskStatus::Succeeded {
            TaskClosureState::Satisfied
        } else {
            TaskClosureState::Open
        });
        service.store.save_task(child).await.expect("save child")
    }

    #[test]
    fn update_rejects_project_rebinding_and_forged_authorization() {
        for field in [
            "project_id",
            "project_context",
            "owner_user_id",
            "tenant_id",
        ] {
            let value = serde_json::json!({field: "replacement"});
            assert!(
                serde_json::from_value::<UpdateTaskRequest>(value).is_err(),
                "{field}"
            );
        }
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
