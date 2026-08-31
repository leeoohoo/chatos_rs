// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::*;
use crate::models::TaskScheduleConfig;
use crate::services::ClonedProjectExecutionTask;

impl TaskService {
    pub(crate) async fn clone_stopped_project_execution_tasks(
        &self,
        project_id: &str,
        requirement_id: &str,
        old_source_session_id: &str,
        old_source_user_message_id: &str,
        new_source_session_id: &str,
        new_source_user_message_id: &str,
    ) -> Result<Vec<ClonedProjectExecutionTask>, String> {
        let tasks = self
            .list_tasks_for_chatos_source(
                old_source_session_id,
                Some(old_source_user_message_id),
                None,
            )
            .await?;
        if tasks.is_empty() {
            return Err("stopped project execution task graph was not found".to_string());
        }
        for task in &tasks {
            let payload = task.input_payload.as_ref();
            let payload_source = payload
                .and_then(|value| value.get("source"))
                .and_then(Value::as_str);
            let payload_requirement_id = payload
                .and_then(|value| value.get("root_requirement_id"))
                .or_else(|| payload.and_then(|value| value.get("requirement_id")))
                .and_then(Value::as_str);
            if task.project_id.as_deref() != Some(project_id)
                || payload_source != Some("chatos_project_requirement_execution")
                || payload_requirement_id != Some(requirement_id)
            {
                return Err(
                    "task graph does not belong to the requested project requirement execution"
                        .to_string(),
                );
            }
            if self.store.has_active_run_for_task(task.id.as_str()).await? {
                return Err(format!(
                    "project execution task still has an active run: {}",
                    task.id
                ));
            }
        }

        let old_task_ids = tasks
            .iter()
            .map(|task| task.id.clone())
            .collect::<BTreeSet<_>>();
        let mut pending = tasks;
        let mut cloned_id_by_old_id = BTreeMap::<String, String>::new();
        let mut cloned = Vec::<ClonedProjectExecutionTask>::new();

        while !pending.is_empty() {
            let ready_index = pending.iter().position(|task| {
                task.prerequisite_task_ids.iter().all(|prerequisite_id| {
                    !old_task_ids.contains(prerequisite_id)
                        || cloned_id_by_old_id.contains_key(prerequisite_id)
                })
            });
            let Some(index) = ready_index else {
                rollback_cloned_tasks(self, cloned.as_slice()).await;
                return Err("stopped project execution task graph contains a cycle".to_string());
            };
            let original = pending.remove(index);
            let project_task_id = original
                .input_payload
                .as_ref()
                .and_then(|payload| payload.get("project_task_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    format!(
                        "project execution task is missing project_task_id: {}",
                        original.id
                    )
                })?;
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = now_rfc3339();
            let prerequisite_task_ids = original
                .prerequisite_task_ids
                .iter()
                .map(|prerequisite_id| {
                    cloned_id_by_old_id
                        .get(prerequisite_id)
                        .cloned()
                        .unwrap_or_else(|| prerequisite_id.clone())
                })
                .collect::<Vec<_>>();
            let mut input_payload = original.input_payload.clone();
            if let Some(Value::Object(payload)) = input_payload.as_mut() {
                payload.insert(
                    "execution_group_id".to_string(),
                    Value::String(new_source_user_message_id.to_string()),
                );
            }
            let task = TaskRecord {
                id: new_id.clone(),
                title: original.title,
                description: original.description,
                objective: original.objective,
                input_payload,
                status: TaskStatus::Ready,
                priority: original.priority,
                tags: original.tags,
                default_model_config_id: original.default_model_config_id,
                memory_thread_id: format!("task-{new_id}"),
                tenant_id: original.tenant_id,
                subject_id: original.subject_id,
                project_id: original.project_id,
                task_profile: original.task_profile,
                creator_user_id: original.creator_user_id,
                creator_username: original.creator_username,
                creator_display_name: original.creator_display_name,
                owner_user_id: original.owner_user_id,
                owner_username: original.owner_username,
                owner_display_name: original.owner_display_name,
                result_summary: None,
                process_log: None,
                last_run_id: None,
                schedule: TaskScheduleConfig::default(),
                parent_task_id: None,
                source_run_id: None,
                source_session_id: Some(new_source_session_id.to_string()),
                source_turn_id: Some(new_source_user_message_id.to_string()),
                source_user_message_id: Some(new_source_user_message_id.to_string()),
                remote_connection_id: original.remote_connection_id,
                prerequisite_task_ids: prerequisite_task_ids.clone(),
                task_tool_state: TaskToolState::default(),
                mcp_config: original.mcp_config,
                plugin_config: original.plugin_config,
                plugin_selection_audit: original.plugin_selection_audit,
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            if let Err(error) = self.ensure_task_thread(&task).await {
                rollback_cloned_tasks(self, cloned.as_slice()).await;
                return Err(error);
            }
            let saved = match self.store.save_task(task).await {
                Ok(task) => task,
                Err(error) => {
                    rollback_cloned_tasks(self, cloned.as_slice()).await;
                    return Err(error);
                }
            };
            if let Err(error) = self
                .store
                .set_task_prerequisites(saved.id.as_str(), prerequisite_task_ids)
                .await
            {
                let _ = self.store.delete_task(saved.id.as_str()).await;
                rollback_cloned_tasks(self, cloned.as_slice()).await;
                return Err(error);
            }
            cloned_id_by_old_id.insert(original.id.clone(), saved.id.clone());
            cloned.push(ClonedProjectExecutionTask {
                old_task_id: original.id,
                project_task_id,
                task: saved,
            });
        }

        Ok(cloned)
    }
}

async fn rollback_cloned_tasks(service: &TaskService, cloned: &[ClonedProjectExecutionTask]) {
    for item in cloned.iter().rev() {
        let _ = service.store.delete_task(item.task.id.as_str()).await;
    }
}
