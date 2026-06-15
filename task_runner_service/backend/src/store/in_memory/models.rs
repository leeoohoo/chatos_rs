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

    pub(in crate::store) fn save_runtime_settings(
        &self,
        settings: RuntimeSettingsRecord,
    ) -> RuntimeSettingsRecord {
        let mut data = self.inner.write();
        data.runtime_settings = Some(settings.clone());
        settings
    }

    pub(in crate::store) fn delete_model_config(&self, id: &str) -> bool {
        let mut data = self.inner.write();
        let deleted = data.model_configs.remove(id).is_some();
        if deleted {
            for task in data.tasks.values_mut() {
                if task.default_model_config_id.as_deref() == Some(id) {
                    task.default_model_config_id = None;
                }
            }
        }
        deleted
    }

    pub(in crate::store) fn list_remote_servers(&self) -> Vec<RemoteServerRecord> {
        let data = self.inner.read();
        let mut items = data.remote_servers.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_remote_server(&self, id: &str) -> Option<RemoteServerRecord> {
        self.inner.read().remote_servers.get(id).cloned()
    }

    pub(in crate::store) fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> RemoteServerRecord {
        let mut data = self.inner.write();
        data.remote_servers
            .insert(server.id.clone(), server.clone());
        server
    }

    pub(in crate::store) fn delete_remote_server(&self, id: &str) -> bool {
        self.inner.write().remote_servers.remove(id).is_some()
    }

    pub(in crate::store) fn list_model_config_usage(&self) -> Vec<ModelConfigUsageRecord> {
        let data = self.inner.read();
        let mut usage = BTreeMap::<String, ModelConfigUsageRecord>::new();

        for task in data.tasks.values() {
            let Some(model_config_id) = task.default_model_config_id.clone() else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.task_count += 1;
        }

        for run in data.runs.values() {
            let entry =
                usage
                    .entry(run.model_config_id.clone())
                    .or_insert(ModelConfigUsageRecord {
                        model_config_id: run.model_config_id.clone(),
                        task_count: 0,
                        run_count: 0,
                    });
            entry.run_count += 1;
        }

        usage.into_values().collect()
    }
}
