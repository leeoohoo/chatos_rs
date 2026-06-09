use std::collections::BTreeMap;
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use parking_lot::RwLock;
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, AuthUser, CreateUserRequest, LoginResponse, UpdateUserRequest, UserRecord,
    UserSummaryRecord,
};
use crate::store::AppStore;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

impl CurrentUser {
    pub fn public_user(&self) -> AuthUser {
        AuthUser {
            id: self.id.clone(),
            username: self.username.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

impl From<&UserRecord> for CurrentUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuthService {
    store: AppStore,
    sessions: Arc<RwLock<BTreeMap<String, CurrentUser>>>,
}

impl AuthService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn ensure_default_admin(&self, config: &AppConfig) -> Result<(), String> {
        if self.store.count_users().await? > 0 {
            return Ok(());
        }

        let now = now_rfc3339();
        let username = normalize_username(config.admin_username.as_str())?;
        let display_name = normalize_display_name(config.admin_display_name.as_str(), &username);
        let user = UserRecord {
            id: Uuid::new_v4().to_string(),
            username,
            display_name,
            password_hash: hash_password(config.admin_password.as_str())?,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_login_at: None,
        };
        self.store.save_user(user).await?;
        Ok(())
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse, String> {
        let username = normalize_username(username)?;
        let Some(mut user) = self.store.get_user_by_username(username.as_str()).await? else {
            return Err("用户名或密码错误".to_string());
        };
        if !user.enabled {
            return Err("用户已禁用".to_string());
        }
        if !verify_password(password, user.password_hash.as_str()) {
            return Err("用户名或密码错误".to_string());
        }

        user.last_login_at = Some(now_rfc3339());
        user.updated_at = now_rfc3339();
        let user = self.store.save_user(user).await?;
        let current = CurrentUser::from(&user);
        let token = Uuid::new_v4().to_string();
        self.sessions.write().insert(token.clone(), current.clone());
        Ok(LoginResponse {
            token,
            user: current.public_user(),
        })
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
        if let Some(enabled) = input.enabled {
            if !enabled && user.id == current_user.id {
                return Err("不能禁用当前登录用户".to_string());
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
        let deleted = self.store.delete_user(id).await?;
        if deleted {
            self.remove_user_sessions(id);
        }
        Ok(deleted)
    }

    pub fn current_user_for_token(&self, token: &str) -> Option<CurrentUser> {
        self.sessions.read().get(token.trim()).cloned()
    }

    pub fn logout(&self, token: &str) {
        self.sessions.write().remove(token.trim());
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

    fn sync_user_sessions(&self, user: &UserRecord) {
        let mut sessions = self.sessions.write();
        if !user.enabled {
            sessions.retain(|_, current| current.id != user.id);
            return;
        }
        for current in sessions.values_mut() {
            if current.id == user.id {
                current.username = user.username.clone();
                current.display_name = user.display_name.clone();
            }
        }
    }

    fn remove_user_sessions(&self, user_id: &str) {
        self.sessions
            .write()
            .retain(|_, current| current.id != user_id);
    }
}

fn normalize_username(value: &str) -> Result<String, String> {
    let username = value.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    if username.len() > 64 {
        return Err("用户名不能超过 64 个字符".to_string());
    }
    Ok(username)
}

fn normalize_display_name(value: &str, username: &str) -> String {
    let display_name = value.trim();
    if display_name.is_empty() {
        username.to_string()
    } else {
        display_name.to_string()
    }
}

fn hash_password(password: &str) -> Result<String, String> {
    if password.trim().is_empty() {
        return Err("密码不能为空".to_string());
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| err.to_string())
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}
