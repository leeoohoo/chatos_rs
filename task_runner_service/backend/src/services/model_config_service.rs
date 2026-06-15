use chatos_ai_runtime::{
    AiRequestHandler, SimplePromptOptions, build_responses_text_input, run_compatible_prompt_with,
    select_preferred_response_text,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    CreateModelConfigRequest, ModelCatalogResponse, ModelConfigRecord, ModelConfigTestResponse,
    ModelConfigUsageRecord, PreviewModelCatalogRequest, TestModelConfigRequest,
    UpdateModelConfigRequest, now_rfc3339,
};
use crate::store::AppStore;

use super::model_catalog::{
    fetch_model_catalog_for_record, normalize_model_base_url_input, normalize_model_config_record,
    normalize_model_provider_input, normalize_model_thinking_level_input,
};
use super::{ModelConfigService, normalized_optional, validate_required};

mod catalog;
mod mutation;
mod testing;

impl ModelConfigService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_using_model_config(
        &self,
        model_config_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.default_model_config_id.as_deref() == Some(model_config_id))
            .map(|task| task.id))
    }

    async fn normalized_model_config_by_id(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        self.store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()
    }

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        let records = self.store.list_model_configs().await?;
        records
            .into_iter()
            .map(normalize_model_config_record)
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.normalized_model_config_by_id(id).await
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        self.store.delete_model_config(id).await
    }

    pub async fn usage_stats(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        self.store.list_model_config_usage().await
    }
}
