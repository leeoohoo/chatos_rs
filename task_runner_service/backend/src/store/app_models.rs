// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_configs()),
            Self::Mongo(store) => store.list_model_configs().await,
        }
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_model_config(id)),
            Self::Mongo(store) => store.get_model_config(id).await,
        }
    }

    pub async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_model_config(model)),
            Self::Mongo(store) => store.save_model_config(model).await,
        }
    }

    pub async fn get_runtime_settings(&self) -> Result<Option<RuntimeSettingsRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_runtime_settings()),
            Self::Mongo(store) => store.get_runtime_settings().await,
        }
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_model_config(id)),
            Self::Mongo(store) => store.delete_model_config(id).await,
        }
    }

    pub async fn list_task_projects(&self) -> Result<Vec<TaskProjectRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_projects()),
            Self::Mongo(store) => store.list_task_projects().await,
        }
    }

    pub async fn get_task_project(&self, id: &str) -> Result<Option<TaskProjectRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task_project(id)),
            Self::Mongo(store) => store.get_task_project(id).await,
        }
    }

    pub async fn save_task_project(
        &self,
        project: TaskProjectRecord,
    ) -> Result<TaskProjectRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task_project(project)),
            Self::Mongo(store) => store.save_task_project(project).await,
        }
    }

    pub async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_remote_servers()),
            Self::Mongo(store) => store.list_remote_servers().await,
        }
    }

    pub async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_remote_server(id)),
            Self::Mongo(store) => store.get_remote_server(id).await,
        }
    }

    pub async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_remote_server(server)),
            Self::Mongo(store) => store.save_remote_server(server).await,
        }
    }

    pub async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_remote_server(id)),
            Self::Mongo(store) => store.delete_remote_server(id).await,
        }
    }

    pub async fn list_external_mcp_configs(&self) -> Result<Vec<ExternalMcpConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_external_mcp_configs()),
            Self::Mongo(store) => store.list_external_mcp_configs().await,
        }
    }

    pub async fn get_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_external_mcp_config(id)),
            Self::Mongo(store) => store.get_external_mcp_config(id).await,
        }
    }

    pub async fn save_external_mcp_config(
        &self,
        config: ExternalMcpConfigRecord,
    ) -> Result<ExternalMcpConfigRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_external_mcp_config(config)),
            Self::Mongo(store) => store.save_external_mcp_config(config).await,
        }
    }

    pub async fn delete_external_mcp_config(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_external_mcp_config(id)),
            Self::Mongo(store) => store.delete_external_mcp_config(id).await,
        }
    }

    pub async fn list_model_config_usage(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_config_usage()),
            Self::Mongo(store) => store.list_model_config_usage().await,
        }
    }
}
