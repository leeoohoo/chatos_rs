use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::auth::{hash_password, normalize_display_name, normalize_username};
use crate::config::AppConfig;
use crate::models::{
    AgentAccountListItem, AgentAccountRecord, UserModelConfigRecord, UserModelSettingsRecord,
    UserRecord, UserSummaryRecord, USER_ROLE_SUPER_ADMIN,
};
use crate::secrets::{decrypt_optional_secret, encrypt_optional_secret};

#[derive(Clone)]
pub struct AppStore {
    pool: SqlitePool,
}

impl AppStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn decrypt_optional_secret_lossy(value: Option<String>) -> Option<String> {
        let fallback = value.clone();
        decrypt_optional_secret(value).unwrap_or(fallback)
    }

    fn decrypt_user_model_config(mut config: UserModelConfigRecord) -> UserModelConfigRecord {
        config.has_api_key = config
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        config.api_key = Self::decrypt_optional_secret_lossy(config.api_key);
        config
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

    pub async fn ensure_default_super_admin(&self, config: &AppConfig) -> Result<(), String> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
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
        sqlx::query_as::<_, UserRecord>(
            "SELECT id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        sqlx::query_as::<_, UserRecord>(
            "SELECT id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn list_users_summary(&self) -> Result<Vec<UserSummaryRecord>, String> {
        sqlx::query_as::<_, UserSummaryRecord>(
            r#"
            SELECT
                u.id,
                u.username,
                u.display_name,
                u.role,
                u.enabled,
                u.created_at,
                u.updated_at,
                u.last_login_at,
                COALESCE(COUNT(a.id), 0) AS agent_count
            FROM users u
            LEFT JOIN agent_accounts a ON a.owner_user_id = u.id
            GROUP BY u.id, u.username, u.display_name, u.role, u.enabled, u.created_at, u.updated_at, u.last_login_at
            ORDER BY u.updated_at DESC, u.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn get_user_summary(&self, id: &str) -> Result<Option<UserSummaryRecord>, String> {
        sqlx::query_as::<_, UserSummaryRecord>(
            r#"
            SELECT
                u.id,
                u.username,
                u.display_name,
                u.role,
                u.enabled,
                u.created_at,
                u.updated_at,
                u.last_login_at,
                COALESCE(COUNT(a.id), 0) AS agent_count
            FROM users u
            LEFT JOIN agent_accounts a ON a.owner_user_id = u.id
            WHERE u.id = ?
            GROUP BY u.id, u.username, u.display_name, u.role, u.enabled, u.created_at, u.updated_at, u.last_login_at
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn insert_user_record(&self, user: &UserRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.role)
        .bind(user.enabled)
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .bind(&user.last_login_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn update_user_record(&self, user: &UserRecord) -> Result<(), String> {
        sqlx::query(
            "UPDATE users SET display_name = ?, password_hash = ?, role = ?, enabled = ?, updated_at = ?, last_login_at = ? WHERE id = ?",
        )
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.role)
        .bind(user.enabled)
        .bind(&user.updated_at)
        .bind(&user.last_login_at)
        .bind(&user.id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn touch_user_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn count_enabled_super_admins(&self) -> Result<i64, String> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE enabled = 1 AND role = ?")
            .bind(USER_ROLE_SUPER_ADMIN)
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())
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
        let base = r#"
            SELECT
                a.id,
                a.username,
                a.display_name,
                a.owner_user_id,
                u.username AS owner_username,
                u.display_name AS owner_display_name,
                a.enabled,
                a.created_at,
                a.updated_at,
                a.last_login_at
            FROM agent_accounts a
            INNER JOIN users u ON u.id = a.owner_user_id
        "#;
        let sql = if owner_user_id.is_some() {
            format!(
                "{base} WHERE a.owner_user_id = ? ORDER BY a.updated_at DESC, a.created_at DESC"
            )
        } else {
            format!("{base} ORDER BY a.updated_at DESC, a.created_at DESC")
        };
        let mut query = sqlx::query_as::<_, AgentAccountListItem>(&sql);
        if let Some(owner_user_id) = owner_user_id {
            query = query.bind(owner_user_id);
        }
        query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_agent_by_id(&self, id: &str) -> Result<Option<AgentAccountRecord>, String> {
        sqlx::query_as::<_, AgentAccountRecord>(
            "SELECT id, username, display_name, password_hash, owner_user_id, enabled, created_at, updated_at, last_login_at FROM agent_accounts WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn find_agent_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AgentAccountRecord>, String> {
        sqlx::query_as::<_, AgentAccountRecord>(
            "SELECT id, username, display_name, password_hash, owner_user_id, enabled, created_at, updated_at, last_login_at FROM agent_accounts WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn insert_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO agent_accounts (id, username, display_name, password_hash, owner_user_id, enabled, created_at, updated_at, last_login_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&agent.id)
        .bind(&agent.username)
        .bind(&agent.display_name)
        .bind(&agent.password_hash)
        .bind(&agent.owner_user_id)
        .bind(agent.enabled)
        .bind(&agent.created_at)
        .bind(&agent.updated_at)
        .bind(&agent.last_login_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn update_agent_record(&self, agent: &AgentAccountRecord) -> Result<(), String> {
        sqlx::query(
            "UPDATE agent_accounts SET display_name = ?, password_hash = ?, owner_user_id = ?, enabled = ?, updated_at = ?, last_login_at = ? WHERE id = ?",
        )
        .bind(&agent.display_name)
        .bind(&agent.password_hash)
        .bind(&agent.owner_user_id)
        .bind(agent.enabled)
        .bind(&agent.updated_at)
        .bind(&agent.last_login_at)
        .bind(&agent.id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn touch_agent_last_login(&self, id: &str) -> Result<(), String> {
        let now = now_rfc3339();
        sqlx::query("UPDATE agent_accounts SET last_login_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
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
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO revoked_tokens (jti, subject_id, revoked_at, expires_at_unix) VALUES (?, ?, ?, ?) ON CONFLICT(jti) DO NOTHING",
        )
        .bind(jti)
        .bind(subject_id)
        .bind(&now)
        .bind(expires_at_unix)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn is_token_revoked(&self, jti: &str) -> Result<bool, String> {
        self.cleanup_expired_revocations().await?;
        let value = sqlx::query("SELECT 1 FROM revoked_tokens WHERE jti = ? LIMIT 1")
            .bind(jti)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(value.is_some())
    }

    async fn cleanup_expired_revocations(&self) -> Result<(), String> {
        sqlx::query("DELETE FROM revoked_tokens WHERE expires_at_unix < ?")
            .bind(Utc::now().timestamp())
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn count_agents_by_owner(&self, owner_user_id: &str) -> Result<i64, String> {
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_accounts WHERE owner_user_id = ?")
            .bind(owner_user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn username_exists_elsewhere(
        &self,
        username: &str,
        current_user_id: Option<&str>,
    ) -> Result<bool, String> {
        let row = sqlx::query("SELECT id FROM users WHERE username = ? LIMIT 1")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if let Some(row) = row {
            let found_id: String = row.try_get("id").map_err(|err| err.to_string())?;
            if current_user_id != Some(found_id.as_str()) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn list_user_model_configs(
        &self,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<UserModelConfigRecord>, String> {
        let base = "SELECT id, owner_user_id, name, provider, model, thinking_level, api_key, base_url, enabled, supports_images, supports_reasoning, supports_responses, created_at, updated_at FROM user_model_configs";
        let sql = if owner_user_id.is_some() {
            format!("{base} WHERE owner_user_id = ? ORDER BY updated_at DESC, created_at DESC")
        } else {
            format!("{base} ORDER BY updated_at DESC, created_at DESC")
        };
        let mut query = sqlx::query_as::<_, UserModelConfigRecord>(&sql);
        if let Some(owner_user_id) = owner_user_id {
            query = query.bind(owner_user_id);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(Self::decrypt_user_model_config)
            .collect())
    }

    pub async fn find_user_model_config_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserModelConfigRecord>, String> {
        let row = sqlx::query_as::<_, UserModelConfigRecord>(
            "SELECT id, owner_user_id, name, provider, model, thinking_level, api_key, base_url, enabled, supports_images, supports_reasoning, supports_responses, created_at, updated_at FROM user_model_configs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.map(Self::decrypt_user_model_config))
    }

    pub async fn save_user_model_config(
        &self,
        config: &UserModelConfigRecord,
    ) -> Result<UserModelConfigRecord, String> {
        let stored = Self::encrypt_user_model_config(config.clone())?;
        sqlx::query(
            r#"
            INSERT INTO user_model_configs (
                id, owner_user_id, name, provider, model, thinking_level, api_key, base_url,
                enabled, supports_images, supports_reasoning, supports_responses, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                owner_user_id = excluded.owner_user_id,
                name = excluded.name,
                provider = excluded.provider,
                model = excluded.model,
                thinking_level = excluded.thinking_level,
                api_key = excluded.api_key,
                base_url = excluded.base_url,
                enabled = excluded.enabled,
                supports_images = excluded.supports_images,
                supports_reasoning = excluded.supports_reasoning,
                supports_responses = excluded.supports_responses,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&stored.id)
        .bind(&stored.owner_user_id)
        .bind(&stored.name)
        .bind(&stored.provider)
        .bind(&stored.model)
        .bind(&stored.thinking_level)
        .bind(&stored.api_key)
        .bind(&stored.base_url)
        .bind(stored.enabled)
        .bind(stored.supports_images)
        .bind(stored.supports_reasoning)
        .bind(stored.supports_responses)
        .bind(&stored.created_at)
        .bind(&stored.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(Self::decrypt_user_model_config(stored))
    }

    pub async fn delete_user_model_config(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        let result = sqlx::query("DELETE FROM user_model_configs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(
            "UPDATE user_model_settings SET memory_summary_model_config_id = NULL WHERE memory_summary_model_config_id = ?",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_model_settings(
        &self,
        user_id: &str,
    ) -> Result<Option<UserModelSettingsRecord>, String> {
        sqlx::query_as::<_, UserModelSettingsRecord>(
            "SELECT user_id, memory_summary_model_config_id, updated_at FROM user_model_settings WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn save_user_model_settings(
        &self,
        settings: &UserModelSettingsRecord,
    ) -> Result<UserModelSettingsRecord, String> {
        sqlx::query(
            r#"
            INSERT INTO user_model_settings (user_id, memory_summary_model_config_id, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(user_id) DO UPDATE SET
                memory_summary_model_config_id = excluded.memory_summary_model_config_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&settings.user_id)
        .bind(&settings.memory_summary_model_config_id)
        .bind(&settings.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(settings.clone())
    }
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
