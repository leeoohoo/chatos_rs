use std::collections::BTreeMap;
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Utc;
use parking_lot::RwLock;
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, AgentTokenResponse, AuthUser, CreateUserRequest, LoginResponse, UpdateUserRequest,
    UserRecord, UserRole, UserSummaryRecord,
};
use crate::store::AppStore;

const AGENT_TOKEN_TTL_SECONDS: i64 = 3600;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
}

impl CurrentUser {
    pub fn public_user(&self) -> AuthUser {
        AuthUser {
            id: self.id.clone(),
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            role: self.role,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == UserRole::Admin
    }

    pub fn is_agent(&self) -> bool {
        self.role == UserRole::Agent
    }
}

impl From<&UserRecord> for CurrentUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            role: value.role,
        }
    }
}

#[derive(Debug, Clone)]
struct SessionRecord {
    user: CurrentUser,
    expires_at_unix: Option<i64>,
}

#[derive(Clone)]
pub struct AuthService {
    store: AppStore,
    sessions: Arc<RwLock<BTreeMap<String, SessionRecord>>>,
}

impl AuthService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

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
        let token = self.create_session(current.clone(), None);
        Ok(LoginResponse {
            token,
            user: current.public_user(),
        })
    }

    pub async fn issue_agent_token(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AgentTokenResponse, String> {
        let username = normalize_username(username)?;
        let Some(mut user) = self.store.get_user_by_username(username.as_str()).await? else {
            return Err("用户名或密码错误".to_string());
        };
        if !user.enabled {
            return Err("用户已禁用".to_string());
        }
        if user.role != UserRole::Agent {
            return Err("该接口仅允许 AI agent 账号换取 token".to_string());
        }
        if !verify_password(password, user.password_hash.as_str()) {
            return Err("用户名或密码错误".to_string());
        }

        user.last_login_at = Some(now_rfc3339());
        user.updated_at = now_rfc3339();
        let user = self.store.save_user(user).await?;
        let current = CurrentUser::from(&user);
        let token = self.create_session(current.clone(), Some(AGENT_TOKEN_TTL_SECONDS));
        Ok(AgentTokenResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: AGENT_TOKEN_TTL_SECONDS,
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

    pub fn current_user_for_token(&self, token: &str) -> Option<CurrentUser> {
        let token = token.trim();
        let now = Utc::now().timestamp();
        {
            let sessions = self.sessions.read();
            let record = sessions.get(token)?;
            if record
                .expires_at_unix
                .is_none_or(|expires_at| expires_at > now)
            {
                return Some(record.user.clone());
            }
        }
        self.sessions.write().remove(token);
        None
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

    async fn enabled_admin_count(&self) -> Result<usize, String> {
        Ok(self
            .store
            .list_users()
            .await?
            .into_iter()
            .filter(|user| user.enabled && user.role == UserRole::Admin)
            .count())
    }

    fn create_session(&self, user: CurrentUser, ttl_seconds: Option<i64>) -> String {
        let token = Uuid::new_v4().to_string();
        let expires_at_unix = ttl_seconds.map(|ttl| Utc::now().timestamp() + ttl.max(1));
        self.sessions.write().insert(
            token.clone(),
            SessionRecord {
                user,
                expires_at_unix,
            },
        );
        token
    }

    fn sync_user_sessions(&self, user: &UserRecord) {
        let mut sessions = self.sessions.write();
        if !user.enabled {
            sessions.retain(|_, current| current.user.id != user.id);
            return;
        }
        for record in sessions.values_mut() {
            if record.user.id == user.id {
                record.user.username = user.username.clone();
                record.user.display_name = user.display_name.clone();
                record.user.role = user.role;
            }
        }
    }

    fn remove_user_sessions(&self, user_id: &str) {
        self.sessions
            .write()
            .retain(|_, current| current.user.id != user_id);
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
