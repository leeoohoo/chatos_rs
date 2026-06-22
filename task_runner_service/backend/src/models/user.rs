use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Agent,
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Agent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    #[serde(default)]
    pub role: UserRole,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
}

impl From<&UserRecord> for AuthUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            role: value.role,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummaryRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

impl From<&UserRecord> for UserSummaryRecord {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            role: value.role,
            enabled: value.enabled,
            created_at: value.created_at.clone(),
            updated_at: value.updated_at.clone(),
            last_login_at: value.last_login_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTokenRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub contact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTokenResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub role: Option<UserRole>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub role: Option<UserRole>,
    pub enabled: Option<bool>,
}
