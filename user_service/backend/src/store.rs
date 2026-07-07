// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, to_document, Bson};
use mongodb::options::{FindOptions, IndexOptions, UpdateOptions};
use mongodb::{Collection, Database, IndexModel};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{hash_password, normalize_display_name, normalize_username};
use crate::config::AppConfig;
use crate::models::{
    AgentAccountListItem, AgentAccountRecord, HarnessProvisioningRecord, UserModelConfigRecord,
    UserModelProviderRecord, UserModelSettingsRecord, UserRecord, UserSummaryRecord,
    USER_ROLE_SUPER_ADMIN,
};
use crate::secrets::{decrypt_optional_secret, encrypt_optional_secret};

#[derive(Clone)]
pub struct AppStore {
    users: Collection<UserRecord>,
    agent_accounts: Collection<AgentAccountRecord>,
    revoked_tokens: Collection<RevokedTokenRecord>,
    user_model_configs: Collection<UserModelConfigRecord>,
    user_model_providers: Collection<UserModelProviderRecord>,
    user_model_settings: Collection<UserModelSettingsRecord>,
    harness_provisioning: Collection<HarnessProvisioningRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevokedTokenRecord {
    jti: String,
    subject_id: String,
    revoked_at: String,
    expires_at_unix: i64,
}

impl AppStore {
    pub fn new(db: Database) -> Self {
        Self {
            users: db.collection("users"),
            agent_accounts: db.collection("agent_accounts"),
            revoked_tokens: db.collection("revoked_tokens"),
            user_model_configs: db.collection("user_model_configs"),
            user_model_providers: db.collection("user_model_providers"),
            user_model_settings: db.collection("user_model_settings"),
            harness_provisioning: db.collection("harness_provisioning"),
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.create_unique_index(&self.users, "username").await?;
        self.create_unique_index(&self.agent_accounts, "username")
            .await?;
        self.create_unique_index(&self.revoked_tokens, "jti")
            .await?;
        self.create_index(&self.agent_accounts, "owner_user_id")
            .await?;
        self.create_index(&self.user_model_configs, "owner_user_id")
            .await?;
        self.create_index(&self.user_model_providers, "owner_user_id")
            .await?;
        self.create_unique_index(&self.user_model_settings, "user_id")
            .await?;
        self.create_unique_index(&self.harness_provisioning, "user_id")
            .await?;
        self.create_index(&self.harness_provisioning, "status")
            .await?;
        Ok(())
    }

    async fn create_index<T>(&self, collection: &Collection<T>, field: &str) -> Result<(), String>
    where
        T: Send + Sync,
    {
        let model = IndexModel::builder().keys(doc! { field: 1 }).build();
        collection
            .create_index(model, None)
            .await
            .map_err(|err| format!("create mongodb index {field} failed: {err}"))?;
        Ok(())
    }

    async fn create_unique_index<T>(
        &self,
        collection: &Collection<T>,
        field: &str,
    ) -> Result<(), String>
    where
        T: Send + Sync,
    {
        let options = IndexOptions::builder().unique(true).build();
        let model = IndexModel::builder()
            .keys(doc! { field: 1 })
            .options(options)
            .build();
        collection
            .create_index(model, None)
            .await
            .map_err(|err| format!("create mongodb unique index {field} failed: {err}"))?;
        Ok(())
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
        config.has_api_key = config
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        config.api_key =
            Self::decrypt_model_secret(config.api_key, "user_model_config", config.id.as_str())?;
        Ok(config)
    }

    fn encrypt_user_model_config(
        mut config: UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        config.api_key = encrypt_optional_secret(config.api_key)?;
        config.has_api_key = config
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        Ok(config)
    }

    fn decrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        provider.has_api_key = provider
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        provider.api_key = Self::decrypt_model_secret(
            provider.api_key,
            "user_model_provider",
            provider.id.as_str(),
        )?;
        Ok(provider)
    }

    fn encrypt_user_model_provider(
        mut provider: UserModelProviderRecord,
    ) -> Result<UserModelProviderRecord, String> {
        provider.api_key = encrypt_optional_secret(provider.api_key)?;
        provider.has_api_key = provider
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        Ok(provider)
    }

    pub async fn ensure_default_super_admin(&self, config: &AppConfig) -> Result<(), String> {
        let count = self
            .users
            .count_documents(None, None)
            .await
            .map_err(|err| err.to_string())?;
        if count > 0 {
            let normalized = normalize_username(config.super_admin_username.as_str())?;
            if let Some(mut user) = self.find_user_by_username(normalized.as_str()).await? {
                if user.role != USER_ROLE_SUPER_ADMIN {
                    user.role = USER_ROLE_SUPER_ADMIN.to_string();
                    user.updated_at = now_rfc3339();
                    self.update_user_record(&user).await?;
                }
            }
            return Ok(());
        }

        let username = normalize_username(config.super_admin_username.as_str())?;
        let now = now_rfc3339();
        let user = UserRecord {
            id: Uuid::new_v4().to_string(),
            username: username.clone(),
            display_name: normalize_display_name(
                Some(config.super_admin_display_name.as_str()),
                &username,
            ),
            password_hash: hash_password(config.super_admin_password.as_str())?,
            role: USER_ROLE_SUPER_ADMIN.to_string(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_login_at: None,
        };
        self.insert_user_record(&user).await?;
        Ok(())
    }

    pub async fn find_user_by_id(&self, id: &str) -> Result<Option<UserRecord>, String> {
        self.users
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        self.users
            .find_one(doc! { "username": username }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_users_summary(&self) -> Result<Vec<UserSummaryRecord>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .build();
        let users: Vec<UserRecord> = self
            .users
            .find(None, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;

        let mut summaries = Vec::with_capacity(users.len());
        for user in users {
            summaries.push(self.user_summary_from_record(user).await?);
        }
        Ok(summaries)
    }

    pub async fn get_user_summary(&self, id: &str) -> Result<Option<UserSummaryRecord>, String> {
        let Some(user) = self.find_user_by_id(id).await? else {
            return Ok(None);
        };
        Ok(Some(self.user_summary_from_record(user).await?))
    }

    async fn user_summary_from_record(
        &self,
        user: UserRecord,
    ) -> Result<UserSummaryRecord, String> {
        let agent_count = self.count_agents_by_owner(user.id.as_str()).await?;
        let harness_provisioning = self
            .find_harness_provisioning_by_user_id(user.id.as_str())
            .await?
            .map(Into::into);
        Ok(UserSummaryRecord {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            enabled: user.enabled,
            created_at: user.created_at,
            updated_at: user.updated_at,
            last_login_at: user.last_login_at,
            agent_count,
            harness_provisioning,
        })
    }

    pub async fn insert_user_record(&self, user: &UserRecord) -> Result<(), String> {
        self.users
            .insert_one(user, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn update_user_record(&self, user: &UserRecord) -> Result<(), String> {
        let update = to_set_document(user)?;
        self.users
            .update_one(doc! { "id": &user.id }, update, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn touch_user_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        self.users
            .update_one(
                doc! { "id": id },
                doc! { "$set": { "last_login_at": &now, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn count_enabled_super_admins(&self) -> Result<i64, String> {
        let count = self
            .users
            .count_documents(
                doc! { "enabled": true, "role": USER_ROLE_SUPER_ADMIN },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        i64::try_from(count).map_err(|err| err.to_string())
    }

    pub async fn list_agent_accounts(&self) -> Result<Vec<AgentAccountListItem>, String> {
        self.list_agent_accounts_inner(None).await
    }

    pub async fn list_agent_accounts_by_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<AgentAccountListItem>, String> {
        self.list_agent_accounts_inner(Some(owner_user_id)).await
    }

    async fn list_agent_accounts_inner(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<AgentAccountListItem>, String> {
        let filter = owner_user_id.map(|owner| doc! { "owner_user_id": owner });
        let options = FindOptions::builder()
            .sort(doc! { "updated_at": -1, "created_at": -1 })
            .build();
        let agents: Vec<AgentAccountRecord> = self
            .agent_accounts
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;

        let mut items = Vec::with_capacity(agents.len());
        for agent in agents {
            let Some(owner) = self.find_user_by_id(agent.owner_user_id.as_str()).await? else {
                continue;
            };
            items.push(AgentAccountListItem {
                id: agent.id,
                username: agent.username,
                display_name: agent.display_name,
                owner_user_id: agent.owner_user_id,
                owner_username: owner.username,
                owner_display_name: owner.display_name,
                enabled: agent.enabled,
                created_at: agent.created_at,
                updated_at: agent.updated_at,
                last_login_at: agent.last_login_at,
            });
        }
        Ok(items)
    }

    pub async fn find_agent_by_id(&self, id: &str) -> Result<Option<AgentAccountRecord>, String> {
        self.agent_accounts
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_agent_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AgentAccountRecord>, String> {
        self.agent_accounts
            .find_one(doc! { "username": username }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn insert_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        self.agent_accounts
            .insert_one(agent, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn update_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        let update = to_set_document(agent)?;
        self.agent_accounts
            .update_one(doc! { "id": &agent.id }, update, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn touch_agent_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        self.agent_accounts
            .update_one(
                doc! { "id": id },
                doc! { "$set": { "last_login_at": &now, "updated_at": &now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn revoke_token(
        &self,
        jti: &str,
        subject_id: &str,
        expires_at_unix: i64,
    ) -> Result<(), String> {
        let record = RevokedTokenRecord {
            jti: jti.to_string(),
            subject_id: subject_id.to_string(),
            revoked_at: now_rfc3339(),
            expires_at_unix,
        };
        self.revoked_tokens
            .update_one(
                doc! { "jti": jti },
                to_set_document(&record)?,
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn is_token_revoked(&self, jti: &str) -> Result<bool, String> {
        self.cleanup_expired_revocations().await?;
        let value = self
            .revoked_tokens
            .find_one(doc! { "jti": jti }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(value.is_some())
    }

    async fn cleanup_expired_revocations(&self) -> Result<(), String> {
        self.revoked_tokens
            .delete_many(
                doc! { "expires_at_unix": { "$lt": Utc::now().timestamp() } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn count_agents_by_owner(&self, owner_user_id: &str) -> Result<i64, String> {
        let count = self
            .agent_accounts
            .count_documents(doc! { "owner_user_id": owner_user_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        i64::try_from(count).map_err(|err| err.to_string())
    }

    pub async fn username_exists_elsewhere(
        &self,
        username: &str,
        current_user_id: Option<&str>,
    ) -> Result<bool, String> {
        let found = self
            .users
            .find_one(doc! { "username": username }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(found.is_some_and(|user| current_user_id != Some(user.id.as_str())))
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

    pub async fn find_harness_provisioning_by_user_id(
        &self,
        user_id: &str,
    ) -> Result<Option<HarnessProvisioningRecord>, String> {
        self.harness_provisioning
            .find_one(doc! { "user_id": user_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn save_harness_provisioning(
        &self,
        record: &HarnessProvisioningRecord,
    ) -> Result<HarnessProvisioningRecord, String> {
        self.harness_provisioning
            .update_one(
                doc! { "user_id": &record.user_id },
                to_set_document(record)?,
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(record.clone())
    }
}

fn to_set_document<T>(value: &T) -> Result<mongodb::bson::Document, String>
where
    T: Serialize,
{
    let mut document = to_document(value).map_err(|err| err.to_string())?;
    document.remove("_id");
    Ok(doc! { "$set": document })
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
