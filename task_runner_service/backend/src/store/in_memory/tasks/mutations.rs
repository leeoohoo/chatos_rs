// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn save_task(&self, task: TaskRecord) -> TaskRecord {
        let mut data = self.inner.write();
        data.tasks.insert(task.id.clone(), task.clone());
        task
    }

    pub(in crate::store) fn update_task_schedule_if_next_run_at(
        &self,
        task_id: &str,
        expected_next_run_at: &str,
        schedule: TaskScheduleConfig,
        updated_at: &str,
    ) -> Option<TaskRecord> {
        let mut data = self.inner.write();
        let task = data.tasks.get_mut(task_id)?;
        if task.schedule.next_run_at.as_deref() != Some(expected_next_run_at) {
            return None;
        }
        task.schedule = schedule;
        task.updated_at = updated_at.to_string();
        Some(task.clone())
    }

    pub(in crate::store) fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Vec<TaskPrerequisiteRecord> {
        let data = self.inner.read();
        data.task_prerequisites
            .get(task_id)
            .into_iter()
            .flat_map(|items| items.iter())
            .map(|prerequisite_task_id| TaskPrerequisiteRecord {
                task_id: task_id.to_string(),
                prerequisite_task_id: prerequisite_task_id.clone(),
                created_at: now_rfc3339(),
            })
            .collect()
    }

    pub(in crate::store) fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Vec<TaskPrerequisiteRecord> {
        let data = self.inner.read();
        data.task_prerequisites
            .iter()
            .filter(|(_, items)| items.contains(prerequisite_task_id))
            .map(|(task_id, _)| TaskPrerequisiteRecord {
                task_id: task_id.clone(),
                prerequisite_task_id: prerequisite_task_id.to_string(),
                created_at: now_rfc3339(),
            })
            .collect()
    }

    pub(in crate::store) fn set_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
    ) -> Vec<TaskPrerequisiteRecord> {
        let now = now_rfc3339();
        let mut data = self.inner.write();
        let items = prerequisite_task_ids.into_iter().collect::<BTreeSet<_>>();
        if items.is_empty() {
            data.task_prerequisites.remove(task_id);
        } else {
            data.task_prerequisites.insert(task_id.to_string(), items);
        }
        data.task_prerequisites
            .get(task_id)
            .into_iter()
            .flat_map(|items| items.iter())
            .map(|prerequisite_task_id| TaskPrerequisiteRecord {
                task_id: task_id.to_string(),
                prerequisite_task_id: prerequisite_task_id.clone(),
                created_at: now.clone(),
            })
            .collect()
    }

    pub(in crate::store) fn delete_task(&self, id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(_) = data.tasks.remove(id) else {
            return false;
        };
        data.task_prerequisites.remove(id);
        for prerequisites in data.task_prerequisites.values_mut() {
            prerequisites.remove(id);
        }
        let run_ids = data
            .runs
            .values()
            .filter(|run| run.task_id == id)
            .map(|run| run.id.clone())
            .collect::<Vec<_>>();
        data.runs.retain(|_, run| run.task_id != id);
        for run_id in &run_ids {
            data.run_events.remove(run_id.as_str());
            data.cancel_requested_runs.remove(run_id.as_str());
        }
        data.ask_user_prompts.retain(|_, prompt| {
            prompt.task_id.as_deref() != Some(id)
                && prompt
                    .run_id
                    .as_deref()
                    .is_none_or(|run_id| !run_ids.iter().any(|candidate| candidate == run_id))
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskMcpConfig, TaskToolState};

    fn test_store() -> InMemoryStore {
        let (sender, _) = broadcast::channel(16);
        InMemoryStore::new(sender)
    }

    fn scheduled_task(next_run_at: &str) -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: "task-1".to_string(),
            title: "scheduled".to_string(),
            description: None,
            objective: "run".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: Some("model-1".to_string()),
            memory_thread_id: "thread-1".to_string(),
            tenant_id: "tenant".to_string(),
            subject_id: "subject".to_string(),
            project_id: crate::models::PUBLIC_PROJECT_ID.to_string(),
            task_profile: crate::models::TASK_PROFILE_DEFAULT.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig {
                mode: TaskScheduleMode::Interval,
                run_at: None,
                interval_seconds: Some(60),
                next_run_at: Some(next_run_at.to_string()),
                last_scheduled_at: None,
            },
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            mcp_config: TaskMcpConfig::default(),
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }

    #[test]
    fn schedule_compare_and_swap_only_advances_matching_due_slot() {
        let store = test_store();
        store.save_task(scheduled_task("2026-01-01T00:00:00Z"));
        let first_schedule = TaskScheduleConfig {
            mode: TaskScheduleMode::Interval,
            run_at: None,
            interval_seconds: Some(60),
            next_run_at: Some("2026-01-01T00:01:00Z".to_string()),
            last_scheduled_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let second_schedule = TaskScheduleConfig {
            mode: TaskScheduleMode::Interval,
            run_at: None,
            interval_seconds: Some(60),
            next_run_at: Some("2026-01-01T00:02:00Z".to_string()),
            last_scheduled_at: Some("2026-01-01T00:00:00Z".to_string()),
        };

        let first = store.update_task_schedule_if_next_run_at(
            "task-1",
            "2026-01-01T00:00:00Z",
            first_schedule,
            "2026-01-01T00:00:01Z",
        );
        let second = store.update_task_schedule_if_next_run_at(
            "task-1",
            "2026-01-01T00:00:00Z",
            second_schedule,
            "2026-01-01T00:00:02Z",
        );

        assert!(first.is_some());
        assert!(second.is_none());
        let task = store.get_task("task-1").expect("task");
        assert_eq!(
            task.schedule.next_run_at.as_deref(),
            Some("2026-01-01T00:01:00Z")
        );
    }
}
