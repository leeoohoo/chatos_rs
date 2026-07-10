// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::{FindOptions, UpdateOptions};

use crate::models::{UserModelConfigRecord, UserModelProviderRecord, UserModelSettingsRecord};
use crate::secrets::{decrypt_optional_secret, encrypt_optional_secret};

use super::{to_set_document, AppStore};

impl AppStore {
    fn decrypt_model_secret(
        value: Option<String>,
        record_type: &str,
        id: &str,
    ) -> Result<Option<String>, String> {
        decrypt_optional_secret(value)
            .map_err(|err| format!("decrypt {record_type} api_key failed for {id}: {err}"))
    }

    fn decrypt_user_model_config(
        mut config: UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let has_stored_api_key = config.has_api_key
            || config
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        config.api_key =
            Self::decrypt_model_secret(config.api_key, "user_model_config", config.id.as_str())?;
        config.has_api_key = has_stored_api_key
            || config
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        Ok(config)
    }

    fn encrypt_user_model_config(
        mut config: UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let has_declared_api_key = config.has_api_key
            || config
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        config.api_key = encrypt_optional_secret(config.api_key)?;
        config.has_api_key = has_declared_api_key;
        Ok(config)
    }

    fn decrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let has_stored_api_key = provider.has_api_key
            || provider
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        provider.api_key = Self::decrypt_model_secret(
            provider.api_key,
            "user_model_provider",
            provider.id.as_str(),
        )?;
        provider.has_api_key = has_stored_api_key
            || provider
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        Ok(provider)
    }

    fn encrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let has_declared_api_key = provider.has_api_key
            || provider
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        provider.api_key = encrypt_optional_secret(provider.api_key)?;
        provider.has_api_key = has_declared_api_key;
        Ok(provider)
    }

    pub async fn list_user_model_configs(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<UserModelConfigRecord>, String> {
        let filter = owner_user_id.map(|owner| doc! { "owner_user_id": owner });
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .build();
        let rows: Vec<UserModelConfigRecord> = self
            .user_model_configs
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        rows.into_iter()
            .map(Self::decrypt_user_model_config)
            .collect()
    }

    pub async fn find_user_model_config_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserModelConfigRecord>, String> {
        let row = self
            .user_model_configs
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        row.map(Self::decrypt_user_model_config).transpose()
    }

    pub async fn save_user_model_config(
        &self,
        config: &UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let stored = Self::encrypt_user_model_config(config.clone())?;
        self.user_model_configs
            .update_one(
                doc! { "id": &stored.id },
                to_set_document(&stored)?,
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Self::decrypt_user_model_config(stored)
    }

    pub async fn delete_user_model_config(&self, id: &str) -> Result<bool, String> {
        let result = self
            .user_model_configs
            .delete_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.user_model_settings
            .update_many(
                doc! { "memory_summary_model_config_id": id },
                doc! { "$set": {
                    "memory_summary_model_config_id": Bson::Null,
                    "memory_summary_thinking_level": Bson::Null,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.user_model_settings
            .update_many(
                doc! { "project_management_agent_model_config_id": id },
                doc! { "$set": {
                    "project_management_agent_model_config_id": Bson::Null,
                    "project_management_agent_thinking_level": Bson::Null,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.deleted_count > 0)
    }

    pub async fn list_user_model_providers(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<UserModelProviderRecord>, String> {
        let filter = owner_user_id.map(|owner| doc! { "owner_user_id": owner });
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .build();
        let rows: Vec<UserModelProviderRecord> = self
            .user_model_providers
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        rows.into_iter()
            .map(Self::decrypt_user_model_provider)
            .collect()
    }

    pub async fn find_user_model_provider_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserModelProviderRecord>, String> {
        let row = self
            .user_model_providers
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        row.map(Self::decrypt_user_model_provider).transpose()
    }

    pub async fn save_user_model_provider(
        &self,
        provider: &UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let stored = Self::encrypt_user_model_provider(provider.clone())?;
        self.user_model_providers
            .update_one(
                doc! { "id": &stored.id },
                to_set_document(&stored)?,
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Self::decrypt_user_model_provider(stored)
    }

    pub async fn delete_user_model_provider(&self, id: &str) -> Result<bool, String> {
        let result = self
            .user_model_providers
            .delete_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.deleted_count > 0)
    }

    pub async fn get_user_model_settings(
        &self,
        user_id: &str,
    ) -> Result<Option<UserModelSettingsRecord>, String> {
        self.user_model_settings
            .find_one(doc! { "user_id": user_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn save_user_model_settings(
        &self,
        settings: &UserModelSettingsRecord,
    ) -> Result<UserModelSettingsRecord, String> {
        self.user_model_settings
            .update_one(
                doc! { "user_id": &settings.user_id },
                to_set_document(settings)?,
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(settings.clone())
    }
}
