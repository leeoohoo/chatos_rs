// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, to_document, Bson};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument, UpdateOptions};

use crate::models::{UserModelConfigRecord, UserModelProviderRecord, UserModelSettingsRecord};
use crate::secrets::{decrypt_optional_secret, encrypt_optional_secret};

use super::{to_set_document, AppStore};

impl AppStore {
    pub async fn migrate_legacy_model_revisions(&self) -> Result<u64, String> {
        self.user_model_configs
            .update_many(
                doc! { "revision": { "$exists": false } },
                doc! { "$set": { "revision": 1_i64 } },
                None,
            )
            .await
            .map(|result| result.modified_count)
            .map_err(|err| err.to_string())
    }

    fn has_usable_api_key(value: Option<&str>) -> bool {
        value.map(str::trim).is_some_and(|value| !value.is_empty())
    }

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
        config.api_key =
            Self::decrypt_model_secret(config.api_key, "user_model_config", config.id.as_str())?;
        config.has_api_key = Self::has_usable_api_key(config.api_key.as_deref());
        Ok(config)
    }

    fn encrypt_user_model_config(
        mut config: UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let has_usable_api_key = Self::has_usable_api_key(config.api_key.as_deref());
        config.api_key = encrypt_optional_secret(config.api_key)?;
        config.has_api_key = has_usable_api_key;
        Ok(config)
    }

    fn decrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        provider.api_key = Self::decrypt_model_secret(
            provider.api_key,
            "user_model_provider",
            provider.id.as_str(),
        )?;
        provider.has_api_key = Self::has_usable_api_key(provider.api_key.as_deref());
        Ok(provider)
    }

    fn encrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let has_usable_api_key = Self::has_usable_api_key(provider.api_key.as_deref());
        provider.api_key = encrypt_optional_secret(provider.api_key)?;
        provider.has_api_key = has_usable_api_key;
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

    pub async fn migrate_legacy_model_task_enabled(&self) -> Result<usize, String> {
        let providers = self.list_user_model_providers(None).await?;
        let configs = self.list_user_model_configs(None).await?;
        let mut migrated = 0usize;
        for mut config in configs {
            if config.task_enabled.is_some() {
                continue;
            }
            let legacy_enabled = config.enabled;
            config.task_enabled = Some(legacy_enabled);
            if let Some(provider) = providers.iter().find(|provider| {
                provider.owner_user_id == config.owner_user_id
                    && (config.source_provider_id.as_deref() == Some(provider.id.as_str())
                        || (config.source_provider_id.is_none()
                            && config.provider == provider.provider
                            && normalized_base_url(config.base_url.as_deref())
                                == normalized_base_url(provider.base_url.as_deref())))
            }) {
                config.source_provider_id = Some(provider.id.clone());
                config.enabled = provider.enabled;
            }
            self.save_user_model_config(&config).await?;
            migrated += 1;
        }
        Ok(migrated)
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
        let mut set_document = to_document(&stored).map_err(|err| err.to_string())?;
        set_document.remove("_id");
        set_document.remove("revision");
        let expected_revision = i64::try_from(stored.revision)
            .map_err(|_| "model config revision exceeds MongoDB integer range".to_string())?;
        let filter = if stored.revision == 0 {
            doc! { "id": &stored.id, "revision": { "$exists": false } }
        } else {
            doc! { "id": &stored.id, "revision": expected_revision }
        };
        let options = FindOneAndUpdateOptions::builder()
            .upsert(stored.revision == 0)
            .return_document(ReturnDocument::After)
            .build();
        let saved = self
            .user_model_configs
            .find_one_and_update(
                filter,
                doc! {
                    "$set": set_document,
                    "$inc": { "revision": 1_i64 },
                },
                options,
            )
            .await
            .map_err(|err| err.to_string())?;
        let saved = saved.ok_or_else(|| {
            format!(
                "model config revision conflict: id={}, expected_revision={}",
                stored.id, stored.revision
            )
        })?;
        Self::decrypt_user_model_config(saved)
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

fn normalized_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::AppStore;

    #[test]
    fn api_key_availability_requires_a_real_secret() {
        assert!(!AppStore::has_usable_api_key(None));
        assert!(!AppStore::has_usable_api_key(Some("")));
        assert!(!AppStore::has_usable_api_key(Some("   ")));
        assert!(AppStore::has_usable_api_key(Some("provider-token")));
    }
}
