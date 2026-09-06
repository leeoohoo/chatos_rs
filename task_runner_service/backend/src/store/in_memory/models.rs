// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_model_configs(&self) -> Vec<ModelConfigRecord> {
        let data = self.inner.read();
        let mut items = data.model_configs.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_model_config(&self, id: &str) -> Option<ModelConfigRecord> {
        self.inner.read().model_configs.get(id).cloned()
    }

    #[cfg(test)]
    pub(in crate::store) fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> ModelConfigRecord {
        let mut data = self.inner.write();
        data.model_configs.insert(model.id.clone(), model.clone());
        model
    }

    pub(in crate::store) fn get_runtime_settings(&self) -> Option<RuntimeSettingsRecord> {
        self.inner.read().runtime_settings.clone()
    }

    pub(in crate::store) fn list_task_projects(&self) -> Vec<TaskProjectRecord> {
        let data = self.inner.read();
        let mut items = data.task_projects.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_task_project(&self, id: &str) -> Option<TaskProjectRecord> {
        self.inner.read().task_projects.get(id).cloned()
    }

    pub(in crate::store) fn save_task_project(
        &self,
        project: TaskProjectRecord,
    ) -> TaskProjectRecord {
        let mut data = self.inner.write();
        data.task_projects
            .insert(project.id.clone(), project.clone());
        project
    }
}
