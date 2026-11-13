// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{UserModelConfigRecord, UserModelProviderRecord, UserModelSettingsRecord};
use crate::secrets::{decrypt_optional_secret, encrypt_optional_secret};

use super::{fetch_all, fetch_optional, json, now_rfc3339, timestamp, AppStore};

impl AppStore {
    /// Repair provider-derived availability for every account after the importer
    /// incorrectly coupled it to chat budgeting. Preserve task-list preferences,
    /// identities, credentials, and explicitly published model-specific budgets.
    pub async fn reconcile_provider_model_catalog(&self) -> Result<usize, String> {
        let providers = self.list_user_model_providers(None).await?;
        let configs = self.list_user_model_configs(None).await?;
        let mut repaired = 0;
        for mut config in configs {
            let Some(provider) = providers.iter().find(|provider| {
                provider.owner_user_id == config.owner_user_id
                    && (config.source_provider_id.as_deref() == Some(&provider.id)
                        || (config.source_provider_id.is_none()
                            && config.provider == provider.provider
                            && normalized_base_url(config.base_url.as_deref())
                                == normalized_base_url(provider.base_url.as_deref())))
            }) else {
                continue;
            };
            let changed = config.enabled != provider.enabled;
            config.enabled = provider.enabled;
            if changed {
                config.updated_at = now_rfc3339();
                self.save_user_model_config(&config).await?;
                repaired += 1;
            }
        }
        Ok(repaired)
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
            Self::decrypt_model_secret(config.api_key, "user_model_config", &config.id)?;
        config.has_api_key = Self::has_usable_api_key(config.api_key.as_deref());
        apply_published_capability(&mut config)?;
        Ok(config)
    }

    fn encrypt_user_model_config(
        mut config: UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        clear_model_capability(&mut config);
        let has_api_key = Self::has_usable_api_key(config.api_key.as_deref());
        config.api_key = encrypt_optional_secret(config.api_key)?;
        config.has_api_key = has_api_key;
        Ok(config)
    }

    fn decrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        provider.api_key =
            Self::decrypt_model_secret(provider.api_key, "user_model_provider", &provider.id)?;
        provider.has_api_key = Self::has_usable_api_key(provider.api_key.as_deref());
        Ok(provider)
    }

    fn encrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let has_api_key = Self::has_usable_api_key(provider.api_key.as_deref());
        provider.api_key = encrypt_optional_secret(provider.api_key)?;
        provider.has_api_key = has_api_key;
        Ok(provider)
    }

    pub async fn list_user_model_configs(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<UserModelConfigRecord>, String> {
        let rows = match owner_user_id {
            Some(owner) => fetch_all(sqlx::query_scalar(
                "SELECT data FROM user_model_configs WHERE owner_user_id=$1 ORDER BY updated_at DESC,created_at DESC,id"
            ).bind(owner), &self.pool).await?,
            None => fetch_all(sqlx::query_scalar(
                "SELECT data FROM user_model_configs ORDER BY updated_at DESC,created_at DESC,id"
            ), &self.pool).await?,
        };
        rows.into_iter()
            .map(Self::decrypt_user_model_config)
            .collect()
    }

    pub async fn migrate_legacy_model_task_enabled(&self) -> Result<usize, String> {
        let providers = self.list_user_model_providers(None).await?;
        let configs = self.list_user_model_configs(None).await?;
        let mut migrated = 0;
        for mut config in configs {
            if config.task_enabled.is_some() {
                continue;
            }
            let legacy_enabled = config.enabled;
            config.task_enabled = Some(legacy_enabled);
            if let Some(provider) = providers.iter().find(|provider| {
                provider.owner_user_id == config.owner_user_id
                    && (config.source_provider_id.as_deref() == Some(&provider.id)
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
        let row = fetch_optional(
            sqlx::query_scalar("SELECT data FROM user_model_configs WHERE id=$1").bind(id),
            &self.pool,
        )
        .await?;
        row.map(Self::decrypt_user_model_config).transpose()
    }

    pub async fn save_user_model_config(
        &self,
        config: &UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let stored = Self::encrypt_user_model_config(config.clone())?;
        sqlx::query(r#"INSERT INTO user_model_configs
            (id,owner_user_id,source_provider_id,enabled,task_enabled,updated_at,created_at,data)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (id) DO UPDATE SET
            owner_user_id=EXCLUDED.owner_user_id,source_provider_id=EXCLUDED.source_provider_id,
            enabled=EXCLUDED.enabled,task_enabled=EXCLUDED.task_enabled,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#)
            .bind(&stored.id).bind(&stored.owner_user_id).bind(&stored.source_provider_id)
            .bind(stored.enabled).bind(stored.task_enabled).bind(timestamp(&stored.updated_at)?)
            .bind(timestamp(&stored.created_at)?).bind(json(&stored)?)
            .execute(&self.pool).await.map_err(|err| err.to_string())?;
        Self::decrypt_user_model_config(stored)
    }

    pub async fn delete_user_model_config(&self, id: &str) -> Result<bool, String> {
        let mut transaction = self.pool.begin().await.map_err(|err| err.to_string())?;
        let deleted = sqlx::query("DELETE FROM user_model_configs WHERE id=$1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(
            r#"UPDATE user_model_settings SET
            data=jsonb_set(jsonb_set(data,'{memory_summary_model_config_id}','null'::jsonb),
                '{memory_summary_thinking_level}','null'::jsonb)
            WHERE data->>'memory_summary_model_config_id'=$1"#,
        )
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(|err| err.to_string())?;
        transaction.commit().await.map_err(|err| err.to_string())?;
        Ok(deleted.rows_affected() > 0)
    }

    pub async fn list_user_model_providers(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<UserModelProviderRecord>, String> {
        let rows = match owner_user_id {
            Some(owner) => fetch_all(sqlx::query_scalar(
                "SELECT data FROM user_model_providers WHERE owner_user_id=$1 ORDER BY updated_at DESC,created_at DESC,id"
            ).bind(owner), &self.pool).await?,
            None => fetch_all(sqlx::query_scalar(
                "SELECT data FROM user_model_providers ORDER BY updated_at DESC,created_at DESC,id"
            ), &self.pool).await?,
        };
        rows.into_iter()
            .map(Self::decrypt_user_model_provider)
            .collect()
    }

    pub async fn find_user_model_provider_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserModelProviderRecord>, String> {
        let row = fetch_optional(
            sqlx::query_scalar("SELECT data FROM user_model_providers WHERE id=$1").bind(id),
            &self.pool,
        )
        .await?;
        row.map(Self::decrypt_user_model_provider).transpose()
    }

    pub async fn save_user_model_provider(
        &self,
        provider: &UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        let stored = Self::encrypt_user_model_provider(provider.clone())?;
        sqlx::query(r#"INSERT INTO user_model_providers (id,owner_user_id,updated_at,created_at,data)
            VALUES ($1,$2,$3,$4,$5) ON CONFLICT (id) DO UPDATE SET owner_user_id=EXCLUDED.owner_user_id,
            updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#)
            .bind(&stored.id).bind(&stored.owner_user_id).bind(timestamp(&stored.updated_at)?)
            .bind(timestamp(&stored.created_at)?).bind(json(&stored)?)
            .execute(&self.pool).await.map_err(|err| err.to_string())?;
        Self::decrypt_user_model_provider(stored)
    }

    pub async fn delete_user_model_provider(&self, id: &str) -> Result<bool, String> {
        sqlx::query("DELETE FROM user_model_providers WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() > 0)
            .map_err(|err| err.to_string())
    }

    pub async fn get_user_model_settings(
        &self,
        user_id: &str,
    ) -> Result<Option<UserModelSettingsRecord>, String> {
        fetch_optional(
            sqlx::query_scalar("SELECT data FROM user_model_settings WHERE user_id=$1")
                .bind(user_id),
            &self.pool,
        )
        .await
    }

    pub async fn save_user_model_settings(
        &self,
        settings: &UserModelSettingsRecord,
    ) -> Result<UserModelSettingsRecord, String> {
        sqlx::query(
            r#"INSERT INTO user_model_settings (user_id,updated_at,data) VALUES ($1,$2,$3)
            ON CONFLICT (user_id) DO UPDATE SET updated_at=EXCLUDED.updated_at,data=EXCLUDED.data"#,
        )
        .bind(&settings.user_id)
        .bind(timestamp(&settings.updated_at)?)
        .bind(json(settings)?)
        .execute(&self.pool)
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

fn clear_model_capability(model: &mut UserModelConfigRecord) {
    model.model_capability_revision = None;
    model.context_window_tokens = None;
    model.output_reserve_tokens = None;
    model.protocol_overhead_estimator_id = None;
    model.tokenizer_id = None;
    model.responses_compaction_threshold = None;
}

fn apply_published_capability(model: &mut UserModelConfigRecord) -> Result<(), String> {
    clear_model_capability(model);
    if !crate::model_catalog::is_selectable_model(&model.model) {
        model.enabled = false;
        return Ok(());
    }
    if chatos_service_runtime::model_workload::ModelWorkload::for_model(&model.model)
        != chatos_service_runtime::model_workload::ModelWorkload::Chat
    {
        return Ok(());
    }
    if let Some((revision, capability)) =
        chatos_service_runtime::model_capability::active_from_env()?
    {
        let compaction_threshold = capability.input_limit()?;
        model.model_capability_revision = Some(revision);
        model.context_window_tokens = Some(
            i64::try_from(capability.context_window_tokens)
                .map_err(|_| "model context budget exceeds supported integer range")?,
        );
        model.output_reserve_tokens = Some(
            i64::try_from(capability.output_reserve_tokens)
                .map_err(|_| "model output budget exceeds supported integer range")?,
        );
        model.protocol_overhead_estimator_id = Some(capability.protocol_overhead_estimator_id);
        model.tokenizer_id = capability.tokenizer_id;
        model.responses_compaction_threshold = Some(
            i64::try_from(compaction_threshold)
                .map_err(|_| "model compaction threshold exceeds supported integer range")?,
        );
    }
    Ok(())
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
