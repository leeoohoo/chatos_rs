use std::collections::{HashSet, VecDeque};

use serde_json::json;

use super::*;

const MAX_CASCADE_CANCEL_TASKS: usize = 500;

struct CancelledTaskSnapshot {
    task: TaskRecord,
    active_run_ids: Vec<String>,
}

impl TaskService {
    pub async fn cancel_task(
        &self,
        id: &str,
        input: CancelTaskRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<CancelTaskResponse>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let reason = input.reason.trim().to_string();
        if reason.is_empty() {
            return Err("取消原因不能为空".to_string());
        }
        if task.status == TaskStatus::Cancelled {
            let reason = task.task_tool_state.cancel_reason.clone().unwrap_or(reason);
            let cascade = self
                .cascade_cancel_dependent_tasks(task.id.as_str(), &reason, current_user)
                .await?;
            for cancelled in &cascade {
                self.try_send_task_callback("task.cancelled", cancelled.task.id.as_str(), None)
                    .await;
            }
            return Ok(Some(CancelTaskResponse {
                cancelled: true,
                task_id: task.id.clone(),
                status: task.status,
                reason,
                active_run_ids: Vec::new(),
                cascade_cancelled_task_ids: cascade
                    .iter()
                    .map(|item| item.task.id.clone())
                    .collect::<Vec<_>>(),
                callback_event: "task.cancelled".to_string(),
                task,
            }));
        }
        if task.status == TaskStatus::Succeeded {
            let cascade = self
                .cascade_cancel_dependent_tasks(task.id.as_str(), &reason, current_user)
                .await?;
            for cancelled in &cascade {
                self.try_send_task_callback("task.cancelled", cancelled.task.id.as_str(), None)
                    .await;
            }
            return Ok(Some(CancelTaskResponse {
                cancelled: !cascade.is_empty(),
                task_id: task.id.clone(),
                status: task.status,
                reason,
                active_run_ids: Vec::new(),
                cascade_cancelled_task_ids: cascade
                    .iter()
                    .map(|item| item.task.id.clone())
                    .collect::<Vec<_>>(),
                callback_event: "task.cancelled".to_string(),
                task,
            }));
        }
        ensure_task_status_cancellable(task.status)?;

        let replacement_task_ids = sanitize_id_list(input.replacement_task_ids);
        let root = self
            .cancel_one_task(
                task,
                &reason,
                current_user,
                replacement_task_ids,
                None,
                None,
            )
            .await?;

        let cascade = self
            .cascade_cancel_dependent_tasks(root.task.id.as_str(), &reason, current_user)
            .await?;
        self.try_send_task_callback("task.cancelled", root.task.id.as_str(), None)
            .await;
        for cancelled in &cascade {
            self.try_send_task_callback("task.cancelled", cancelled.task.id.as_str(), None)
                .await;
        }

        Ok(Some(CancelTaskResponse {
            cancelled: true,
            task_id: root.task.id.clone(),
            status: root.task.status,
            reason,
            active_run_ids: root.active_run_ids,
            cascade_cancelled_task_ids: cascade
                .iter()
                .map(|item| item.task.id.clone())
                .collect::<Vec<_>>(),
            callback_event: "task.cancelled".to_string(),
            task: root.task,
        }))
    }

    async fn cascade_cancel_dependent_tasks(
        &self,
        root_task_id: &str,
        root_reason: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<Vec<CancelledTaskSnapshot>, String> {
        let dependent_ids = self.resolve_dependent_task_ids(root_task_id).await?;
        let mut cancelled = Vec::new();
        for dependent_task_id in dependent_ids {
            let Some(task) = self.store.get_task(&dependent_task_id).await? else {
                continue;
            };
            if !is_task_status_cancellable(task.status) {
                continue;
            }
            let reason = format!("前置任务 {root_task_id} 已取消：{root_reason}");
            let snapshot = self
                .cancel_one_task(
                    task,
                    &reason,
                    current_user,
                    Vec::new(),
                    Some(root_task_id.to_string()),
                    Some(root_task_id.to_string()),
                )
                .await?;
            cancelled.push(snapshot);
        }
        Ok(cancelled)
    }

    async fn resolve_dependent_task_ids(&self, root_task_id: &str) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([root_task_id.to_string()]);
        visited.insert(root_task_id.to_string());

        while let Some(current_task_id) = queue.pop_front() {
            for edge in self.store.list_task_dependents(&current_task_id).await? {
                let dependent_task_id = edge.task_id;
                if !visited.insert(dependent_task_id.clone()) {
                    continue;
                }
                if out.len() >= MAX_CASCADE_CANCEL_TASKS {
                    return Err("级联取消任务数量超过上限，请拆分处理".to_string());
                }
                out.push(dependent_task_id.clone());
                queue.push_back(dependent_task_id);
            }
        }
        Ok(out)
    }

    async fn cancel_one_task(
        &self,
        mut task: TaskRecord,
        reason: &str,
        current_user: Option<&CurrentUser>,
        replacement_task_ids: Vec<String>,
        cancelled_because_task_id: Option<String>,
        cascade_root_task_id: Option<String>,
    ) -> Result<CancelledTaskSnapshot, String> {
        ensure_task_status_cancellable(task.status)?;
        let active_run_ids = self
            .cancel_active_runs_for_task(task.id.as_str(), reason)
            .await?;
        let now = now_rfc3339();
        task.status = TaskStatus::Cancelled;
        task.result_summary = Some(format!("任务已取消：{reason}"));
        if let Some(last_run_id) = active_run_ids.first() {
            task.last_run_id = Some(last_run_id.clone());
        }
        task.task_tool_state.cancel_reason = Some(reason.to_string());
        task.task_tool_state.cancelled_at = Some(now.clone());
        task.task_tool_state.cancelled_by_user_id = current_user.map(|user| user.id.clone());
        task.task_tool_state.cancelled_by_username = current_user.map(|user| user.username.clone());
        task.task_tool_state.cancelled_by_display_name =
            current_user.map(|user| user.display_name.clone());
        task.task_tool_state.replacement_task_ids = replacement_task_ids;
        task.task_tool_state.cancelled_because_task_id = cancelled_because_task_id;
        task.task_tool_state.cascade_root_task_id = cascade_root_task_id;
        task.updated_at = now;
        let task = self.store.save_task(task).await?;
        Ok(CancelledTaskSnapshot {
            task,
            active_run_ids,
        })
    }

    async fn cancel_active_runs_for_task(
        &self,
        task_id: &str,
        reason: &str,
    ) -> Result<Vec<String>, String> {
        let mut active_run_ids = Vec::new();
        for mut run in self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .filter(|run| matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running))
        {
            active_run_ids.push(run.id.clone());
            if !run.cancel_requested {
                let _ = self.store.mark_cancel_requested(run.id.as_str()).await?;
                self.store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "cancel_requested",
                        Some(format!("任务已取消，请求停止运行：{reason}")),
                        Some(json!({ "reason": reason })),
                    ))
                    .await?;
            }
            if run.status == TaskRunStatus::Queued {
                run.status = TaskRunStatus::Cancelled;
                run.cancel_requested = true;
                run.finished_at = Some(now_rfc3339());
                run.updated_at = now_rfc3339();
                self.store.save_run(run.clone()).await?;
                self.store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "cancelled",
                        Some(format!("任务在启动前已取消：{reason}")),
                        Some(json!({ "reason": reason })),
                    ))
                    .await?;
            }
        }
        Ok(active_run_ids)
    }
}

fn ensure_task_status_cancellable(status: TaskStatus) -> Result<(), String> {
    if is_task_status_cancellable(status) {
        return Ok(());
    }
    if status == TaskStatus::Succeeded {
        return Err("已成功的任务不允许作废或取消".to_string());
    }
    Err("只有待执行或执行中的任务允许取消".to_string())
}

fn is_task_status_cancellable(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Ready | TaskStatus::Queued | TaskStatus::Running
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use crate::config::{AppConfig, StoreMode};

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-runner-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            chatos_callback_url: None,
            chatos_callback_secret: None,
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

    async fn create_task_with_status(
        service: &TaskService,
        title: &str,
        status: TaskStatus,
        prerequisite_task_ids: Vec<String>,
    ) -> TaskRecord {
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
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    mcp_config: None,
                    prerequisite_task_ids: Some(prerequisite_task_ids),
                },
                None,
                None,
            )
            .await
            .expect("create task")
    }

    #[tokio::test]
    async fn cancel_task_keeps_succeeded_root_but_cascades_pending_dependents() {
        let service = test_service().await;
        let root =
            create_task_with_status(&service, "already done", TaskStatus::Succeeded, Vec::new())
                .await;
        let pending_child = create_task_with_status(
            &service,
            "pending child",
            TaskStatus::Ready,
            vec![root.id.clone()],
        )
        .await;

        let response = service
            .cancel_task(
                root.id.as_str(),
                CancelTaskRequest {
                    reason: "user changed their mind".to_string(),
                    replacement_task_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("cancel succeeded root dependents")
            .expect("task exists");

        assert_eq!(response.task_id, root.id);
        assert_eq!(response.status, TaskStatus::Succeeded);
        assert_eq!(
            response.cascade_cancelled_task_ids,
            vec![pending_child.id.clone()]
        );

        let root_after = service
            .get_task(root.id.as_str())
            .await
            .expect("get root")
            .expect("root");
        assert_eq!(root_after.status, TaskStatus::Succeeded);

        let pending_after = service
            .get_task(pending_child.id.as_str())
            .await
            .expect("get pending child")
            .expect("pending child");
        assert_eq!(pending_after.status, TaskStatus::Cancelled);
        assert_eq!(
            pending_after.task_tool_state.cancelled_because_task_id,
            Some(root.id.clone())
        );
        assert_eq!(
            pending_after.task_tool_state.cascade_root_task_id,
            Some(root.id.clone())
        );
    }

    #[tokio::test]
    async fn cancel_task_cascades_to_pending_dependents_but_not_succeeded_dependents() {
        let service = test_service().await;
        let root = create_task_with_status(&service, "root", TaskStatus::Ready, Vec::new()).await;
        let pending_child = create_task_with_status(
            &service,
            "pending child",
            TaskStatus::Ready,
            vec![root.id.clone()],
        )
        .await;
        let succeeded_child = create_task_with_status(
            &service,
            "succeeded child",
            TaskStatus::Succeeded,
            vec![root.id.clone()],
        )
        .await;

        let response = service
            .cancel_task(
                root.id.as_str(),
                CancelTaskRequest {
                    reason: "root no longer matches the user intent".to_string(),
                    replacement_task_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("cancel task")
            .expect("task exists");

        assert_eq!(response.status, TaskStatus::Cancelled);
        assert_eq!(
            response.cascade_cancelled_task_ids,
            vec![pending_child.id.clone()]
        );

        let root_after = service
            .get_task(root.id.as_str())
            .await
            .expect("get root")
            .expect("root");
        assert_eq!(root_after.status, TaskStatus::Cancelled);

        let pending_after = service
            .get_task(pending_child.id.as_str())
            .await
            .expect("get pending child")
            .expect("pending child");
        assert_eq!(pending_after.status, TaskStatus::Cancelled);
        assert_eq!(
            pending_after.task_tool_state.cancelled_because_task_id,
            Some(root.id.clone())
        );
        assert_eq!(
            pending_after.task_tool_state.cascade_root_task_id,
            Some(root.id.clone())
        );

        let succeeded_after = service
            .get_task(succeeded_child.id.as_str())
            .await
            .expect("get succeeded child")
            .expect("succeeded child");
        assert_eq!(succeeded_after.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn cancelled_tasks_cannot_be_added_as_prerequisites() {
        let service = test_service().await;
        let root = create_task_with_status(&service, "root", TaskStatus::Ready, Vec::new()).await;
        service
            .cancel_task(
                root.id.as_str(),
                CancelTaskRequest {
                    reason: "not needed anymore".to_string(),
                    replacement_task_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("cancel task");

        let err = service
            .create_task(
                CreateTaskRequest {
                    title: "dependent".to_string(),
                    description: None,
                    objective: "depends on cancelled root".to_string(),
                    input_payload: None,
                    status: Some(TaskStatus::Ready),
                    priority: None,
                    tags: None,
                    default_model_config_id: None,
                    project_id: None,
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    mcp_config: None,
                    prerequisite_task_ids: Some(vec![root.id.clone()]),
                },
                None,
                None,
            )
            .await
            .expect_err("cancelled prerequisite should fail");

        assert!(err.contains("已取消任务不能作为前置任务"));
    }
}
