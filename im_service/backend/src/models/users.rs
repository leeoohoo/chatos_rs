use serde::{Deserialize, Serialize};

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub password_hash: String,
    pub role: String,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImUserRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub password: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateImUserRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}
