// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize)]
pub struct RemoteConnectionView {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub has_password: bool,
    pub has_private_key_path: bool,
    pub has_certificate_path: bool,
    pub default_remote_path: Option<String>,
    pub host_key_policy: String,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<i64>,
    pub jump_username: Option<String>,
    pub has_jump_private_key_path: bool,
    pub has_jump_certificate_path: bool,
    pub has_jump_password: bool,
    pub user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

pub(crate) struct NewRemoteConnection {
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
}

impl RemoteConnection {
    pub(crate) fn new(input: NewRemoteConnection) -> Self {
        let NewRemoteConnection {
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
        } = input;
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

    pub fn to_view(&self) -> RemoteConnectionView {
        RemoteConnectionView {
            id: self.id.clone(),
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth_type: self.auth_type.clone(),
            has_password: has_text(self.password.as_deref()),
            has_private_key_path: has_text(self.private_key_path.as_deref()),
            has_certificate_path: has_text(self.certificate_path.as_deref()),
            default_remote_path: self.default_remote_path.clone(),
            host_key_policy: self.host_key_policy.clone(),
            jump_enabled: self.jump_enabled,
            jump_connection_id: self.jump_connection_id.clone(),
            jump_host: self.jump_host.clone(),
            jump_port: self.jump_port,
            jump_username: self.jump_username.clone(),
            has_jump_private_key_path: has_text(self.jump_private_key_path.as_deref()),
            has_jump_certificate_path: has_text(self.jump_certificate_path.as_deref()),
            has_jump_password: has_text(self.jump_password.as_deref()),
            user_id: self.user_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_active_at: self.last_active_at.clone(),
        }
    }

    pub fn requires_local_credential_execution(&self) -> bool {
        self.auth_type != "password"
            || has_text(self.private_key_path.as_deref())
            || has_text(self.certificate_path.as_deref())
            || has_text(self.jump_private_key_path.as_deref())
            || has_text(self.jump_certificate_path.as_deref())
    }
}

fn has_text(value: Option<&str>) -> bool {
    value.is_some_and(|item| !item.trim().is_empty())
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
