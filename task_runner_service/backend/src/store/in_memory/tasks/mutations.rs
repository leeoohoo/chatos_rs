use super::*;

impl InMemoryStore {
    pub(in crate::store) fn save_task(&self, task: TaskRecord) -> TaskRecord {
        let mut data = self.inner.write();
        data.tasks.insert(task.id.clone(), task.clone());
        task
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
        data.ui_prompts.retain(|_, prompt| {
            prompt.task_id.as_deref() != Some(id)
                && prompt
                    .run_id
                    .as_deref()
                    .is_none_or(|run_id| !run_ids.iter().any(|candidate| candidate == run_id))
        });
        true
    }
}
