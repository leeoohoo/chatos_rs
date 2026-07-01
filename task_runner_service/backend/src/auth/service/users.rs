// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AuthService {
    pub async fn ensure_default_admin(&self, config: &AppConfig) -> Result<(), String> {
        let configured_admin_username = normalize_username(config.admin_username.as_str())?;
        if self.store.count_users().await? > 0 {
            if let Some(mut user) = self
                .store
                .get_user_by_username(configured_admin_username.as_str())
                .await?
            {
                if user.role != UserRole::Admin {
                    user.role = UserRole::Admin;
                    user.updated_at = now_rfc3339();
                    let user = self.store.save_user(user).await?;
                    self.sync_user_sessions(&user);
                }
            }
            return Ok(());
        }

        let now = now_rfc3339();
        let username = configured_admin_username;
        let display_name = normalize_display_name(config.admin_display_name.as_str(), &username);
        let user = UserRecord {
            id: Uuid::new_v4().to_string(),
            username,
            display_name,
            password_hash: hash_password(config.admin_password.as_str())?,
            role: UserRole::Admin,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_login_at: None,
        };
        self.store.save_user(user).await?;
        Ok(())
    }

    pub async fn list_users(&self) -> Result<Vec<UserSummaryRecord>, String> {
        Ok(self
            .store
            .list_users()
            .await?
            .iter()
            .map(UserSummaryRecord::from)
            .collect())
    }

    pub async fn create_user(&self, input: CreateUserRequest) -> Result<UserSummaryRecord, String> {
        let username = normalize_username(input.username.as_str())?;
        if self
            .store
            .get_user_by_username(username.as_str())
            .await?
            .is_some()
        {
            return Err(format!("用户名已存在: {username}"));
        }

        let now = now_rfc3339();
        let display_name =
            normalize_display_name(input.display_name.as_deref().unwrap_or_default(), &username);
        let user = UserRecord {
            id: Uuid::new_v4().to_string(),
            username,
            display_name,
            password_hash: hash_password(input.password.as_str())?,
            role: input.role.unwrap_or(UserRole::Agent),
            enabled: input.enabled.unwrap_or(true),
            created_at: now.clone(),
            updated_at: now,
            last_login_at: None,
        };
        let user = self.store.save_user(user).await?;
        Ok(UserSummaryRecord::from(&user))
    }

    pub async fn update_user(
        &self,
        id: &str,
        input: UpdateUserRequest,
        current_user: &CurrentUser,
    ) -> Result<Option<UserSummaryRecord>, String> {
        let Some(mut user) = self.store.get_user(id).await? else {
            return Ok(None);
        };

        if let Some(display_name) = input.display_name {
            user.display_name =
                normalize_display_name(display_name.as_str(), user.username.as_str());
        }
        if let Some(password) = input.password {
            user.password_hash = hash_password(password.as_str())?;
        }
        if let Some(role) = input.role {
            if user.id == current_user.id && role != UserRole::Admin {
                return Err("不能取消当前登录管理员的管理员角色".to_string());
            }
            if user.role == UserRole::Admin
                && role != UserRole::Admin
                && user.enabled
                && self.enabled_admin_count().await? <= 1
            {
                return Err("至少需要保留一个启用管理员".to_string());
            }
            user.role = role;
        }
        if let Some(enabled) = input.enabled {
            if !enabled && user.id == current_user.id {
                return Err("不能禁用当前登录用户".to_string());
            }
            if user.role == UserRole::Admin
                && user.enabled
                && !enabled
                && self.enabled_admin_count().await? <= 1
            {
                return Err("至少需要保留一个启用管理员".to_string());
            }
            if user.enabled && !enabled && self.enabled_user_count().await? <= 1 {
                return Err("至少需要保留一个启用用户".to_string());
            }
            user.enabled = enabled;
        }
        user.updated_at = now_rfc3339();
        let user = self.store.save_user(user).await?;
        self.sync_user_sessions(&user);
        Ok(Some(UserSummaryRecord::from(&user)))
    }

    pub async fn delete_user(&self, id: &str, current_user: &CurrentUser) -> Result<bool, String> {
        if id == current_user.id {
            return Err("不能删除当前登录用户".to_string());
        }
        let Some(user) = self.store.get_user(id).await? else {
            return Ok(false);
        };
        if user.enabled && self.enabled_user_count().await? <= 1 {
            return Err("至少需要保留一个启用用户".to_string());
        }
        if user.role == UserRole::Admin && user.enabled && self.enabled_admin_count().await? <= 1 {
            return Err("至少需要保留一个启用管理员".to_string());
        }
        let deleted = self.store.delete_user(id).await?;
        if deleted {
            self.remove_user_sessions(id);
        }
        Ok(deleted)
    }

    async fn enabled_user_count(&self) -> Result<usize, String> {
        Ok(self
            .store
            .list_users()
            .await?
            .into_iter()
            .filter(|user| user.enabled)
            .count())
    }

    async fn enabled_admin_count(&self) -> Result<usize, String> {
        Ok(self
            .store
            .list_users()
            .await?
            .into_iter()
            .filter(|user| user.enabled && user.role == UserRole::Admin)
            .count())
    }
}
