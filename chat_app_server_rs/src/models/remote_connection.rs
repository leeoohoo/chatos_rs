use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::remote_connections as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConnection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: String,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<i64>,
    pub jump_username: Option<String>,
    pub jump_private_key_path: Option<String>,
    pub jump_certificate_path: Option<String>,
    pub jump_password: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

#[derive(Debug, FromRow)]
pub struct RemoteConnectionRow {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: String,
    pub jump_enabled: i64,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<i64>,
    pub jump_username: Option<String>,
    pub jump_private_key_path: Option<String>,
    pub jump_certificate_path: Option<String>,
    pub jump_password: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

impl RemoteConnectionRow {
    pub fn to_remote_connection(self) -> RemoteConnection {
        RemoteConnection {
            id: self.id,
            name: self.name,
            host: self.host,
            port: self.port,
            username: self.username,
            auth_type: self.auth_type,
            password: self.password,
            private_key_path: self.private_key_path,
            certificate_path: self.certificate_path,
            default_remote_path: self.default_remote_path,
            host_key_policy: self.host_key_policy,
            jump_enabled: self.jump_enabled != 0,
            jump_connection_id: self.jump_connection_id,
            jump_host: self.jump_host,
            jump_port: self.jump_port,
            jump_username: self.jump_username,
            jump_private_key_path: self.jump_private_key_path,
            jump_certificate_path: self.jump_certificate_path,
            jump_password: self.jump_password,
            user_id: self.user_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_active_at: self.last_active_at,
        }
    }
}

impl RemoteConnection {
    pub fn new(
        name: String,
        host: String,
        port: i64,
        username: String,
        auth_type: String,
        password: Option<String>,
        private_key_path: Option<String>,
        certificate_path: Option<String>,
        default_remote_path: Option<String>,
        host_key_policy: String,
        jump_enabled: bool,
        jump_connection_id: Option<String>,
        jump_host: Option<String>,
        jump_port: Option<i64>,
        jump_username: Option<String>,
        jump_private_key_path: Option<String>,
        jump_certificate_path: Option<String>,
        jump_password: Option<String>,
        user_id: Option<String>,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            host,
            port,
            username,
            auth_type,
            password,
            private_key_path,
            certificate_path,
            default_remote_path,
            host_key_policy,
            jump_enabled,
            jump_connection_id,
            jump_host,
            jump_port,
            jump_username,
            jump_private_key_path,
            jump_certificate_path,
            jump_password,
            user_id,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_active_at: now,
        }
    }
}

pub struct RemoteConnectionService;

impl RemoteConnectionService {
    pub async fn create(data: RemoteConnection) -> Result<String, String> {
        repo::create_remote_connection(&data).await
    }

    pub async fn get_by_id(id: &str) -> Result<Option<RemoteConnection>, String> {
        repo::get_remote_connection_by_id(id).await
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<RemoteConnection>, String> {
        repo::list_remote_connections(user_id).await
    }

    pub async fn update(id: &str, data: &RemoteConnection) -> Result<(), String> {
        repo::update_remote_connection(id, data).await
    }

    pub async fn touch(id: &str) -> Result<(), String> {
        repo::touch_remote_connection(id).await
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        repo::delete_remote_connection(id).await
    }
}
