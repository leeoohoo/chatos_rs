use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::users as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub status: String,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub status: String,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl UserRow {
    pub fn to_user(self) -> User {
        User {
            id: self.id,
            email: self.email,
            password_hash: self.password_hash,
            display_name: self.display_name,
            status: self.status,
            last_login_at: self.last_login_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl User {
    pub fn new(email: String, password_hash: String, display_name: Option<String>) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            email,
            password_hash,
            display_name,
            status: "active".to_string(),
            last_login_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct UserService;

impl UserService {
    pub async fn create(data: &User) -> Result<(), String> {
        repo::create_user(data).await
    }

    pub async fn get_by_email(email: &str) -> Result<Option<User>, String> {
        repo::get_user_by_email(email).await
    }

    pub async fn get_by_id(id: &str) -> Result<Option<User>, String> {
        repo::get_user_by_id(id).await
    }

    pub async fn update_last_login_at(id: &str) -> Result<(), String> {
        repo::update_last_login_at(id).await
    }
}
