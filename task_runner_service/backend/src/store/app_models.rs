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

    #[cfg(test)]
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
}
