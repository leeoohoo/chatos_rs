use super::*;

impl AppStore {
    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_configs()),
            Self::Sqlite(store) => store.list_model_configs().await,
            Self::Mongo(store) => store.list_model_configs().await,
        }
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_model_config(id)),
            Self::Sqlite(store) => store.get_model_config(id).await,
            Self::Mongo(store) => store.get_model_config(id).await,
        }
    }

    pub async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_model_config(model)),
            Self::Sqlite(store) => store.save_model_config(model).await,
            Self::Mongo(store) => store.save_model_config(model).await,
        }
    }

    pub async fn get_runtime_settings(&self) -> Result<Option<RuntimeSettingsRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_runtime_settings()),
            Self::Sqlite(store) => store.get_runtime_settings().await,
            Self::Mongo(store) => store.get_runtime_settings().await,
        }
    }

    pub async fn save_runtime_settings(
        &self,
        settings: RuntimeSettingsRecord,
    ) -> Result<RuntimeSettingsRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_runtime_settings(settings)),
            Self::Sqlite(store) => store.save_runtime_settings(settings).await,
            Self::Mongo(store) => store.save_runtime_settings(settings).await,
        }
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_model_config(id)),
            Self::Sqlite(store) => store.delete_model_config(id).await,
            Self::Mongo(store) => store.delete_model_config(id).await,
        }
    }

    pub async fn list_task_projects(&self) -> Result<Vec<TaskProjectRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_projects()),
            Self::Sqlite(store) => store.list_task_projects().await,
            Self::Mongo(store) => store.list_task_projects().await,
        }
    }

    pub async fn get_task_project(&self, id: &str) -> Result<Option<TaskProjectRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task_project(id)),
            Self::Sqlite(store) => store.get_task_project(id).await,
            Self::Mongo(store) => store.get_task_project(id).await,
        }
    }

    pub async fn save_task_project(
        &self,
        project: TaskProjectRecord,
    ) -> Result<TaskProjectRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task_project(project)),
            Self::Sqlite(store) => store.save_task_project(project).await,
            Self::Mongo(store) => store.save_task_project(project).await,
        }
    }

    pub async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_remote_servers()),
            Self::Sqlite(store) => store.list_remote_servers().await,
            Self::Mongo(store) => store.list_remote_servers().await,
        }
    }

    pub async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_remote_server(id)),
            Self::Sqlite(store) => store.get_remote_server(id).await,
            Self::Mongo(store) => store.get_remote_server(id).await,
        }
    }

    pub async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_remote_server(server)),
            Self::Sqlite(store) => store.save_remote_server(server).await,
            Self::Mongo(store) => store.save_remote_server(server).await,
        }
    }

    pub async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_remote_server(id)),
            Self::Sqlite(store) => store.delete_remote_server(id).await,
            Self::Mongo(store) => store.delete_remote_server(id).await,
        }
    }

    pub async fn list_external_mcp_configs(&self) -> Result<Vec<ExternalMcpConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_external_mcp_configs()),
            Self::Sqlite(store) => store.list_external_mcp_configs().await,
            Self::Mongo(store) => store.list_external_mcp_configs().await,
        }
    }

    pub async fn get_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_external_mcp_config(id)),
            Self::Sqlite(store) => store.get_external_mcp_config(id).await,
            Self::Mongo(store) => store.get_external_mcp_config(id).await,
        }
    }

    pub async fn save_external_mcp_config(
        &self,
        config: ExternalMcpConfigRecord,
    ) -> Result<ExternalMcpConfigRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_external_mcp_config(config)),
            Self::Sqlite(store) => store.save_external_mcp_config(config).await,
            Self::Mongo(store) => store.save_external_mcp_config(config).await,
        }
    }

    pub async fn delete_external_mcp_config(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_external_mcp_config(id)),
            Self::Sqlite(store) => store.delete_external_mcp_config(id).await,
            Self::Mongo(store) => store.delete_external_mcp_config(id).await,
        }
    }

    pub async fn list_model_config_usage(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_config_usage()),
            Self::Sqlite(store) => store.list_model_config_usage().await,
            Self::Mongo(store) => store.list_model_config_usage().await,
        }
    }
}
