// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRemoteAuthenticationType {
    PrivateKey,
    PrivateKeyCert,
    Password,
}

impl LocalRemoteAuthenticationType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrivateKey => "private_key",
            Self::PrivateKeyCert => "private_key_cert",
            Self::Password => "password",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRemoteHostKeyPolicy {
    Strict,
    AcceptNew,
}

impl LocalRemoteHostKeyPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::AcceptNew => "accept_new",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRemoteConnectionSpec {
    pub name: Option<String>,
    pub host: String,
    pub port: u32,
    pub username: String,
    pub authentication_type: LocalRemoteAuthenticationType,
    pub default_remote_path: Option<String>,
    pub host_key_policy: LocalRemoteHostKeyPolicy,
    pub local_connector_device_id: String,
    pub local_connector_workspace_id: String,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<u32>,
    pub jump_username: Option<String>,
}

impl LocalRemoteConnectionSpec {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_text("name", self.name.as_deref(), 1_000)?;
        validate_text("host", &self.host, 1_000)?;
        validate_port("port", self.port)?;
        validate_text("username", &self.username, 1_000)?;
        validate_optional_text(
            "default_remote_path",
            self.default_remote_path.as_deref(),
            4_000,
        )?;
        validate_text(
            "local_connector_device_id",
            &self.local_connector_device_id,
            1_000,
        )?;
        validate_text(
            "local_connector_workspace_id",
            &self.local_connector_workspace_id,
            1_000,
        )?;
        if let Some(id) = self.jump_connection_id.as_deref() {
            if !id.trim().is_empty() {
                validate_identifier("jump_connection_id", id)?;
            }
        }
        validate_optional_text("jump_host", self.jump_host.as_deref(), 1_000)?;
        if let Some(port) = self.jump_port {
            validate_port("jump_port", port)?;
        }
        validate_optional_text("jump_username", self.jump_username.as_deref(), 1_000)?;
        let has_jump_connection = self
            .jump_connection_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let has_manual_jump = self
            .jump_host
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self.jump_port.is_some()
            && self
                .jump_username
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
        if self.jump_enabled && !has_jump_connection && !has_manual_jump {
            return Err(
                "enabled jump routing requires jump_connection_id or host, port, and username"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRemoteConnectionsCommand {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRemoteConnectionCommand {
    pub owner_user_id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRemoteConnectionCommand {
    pub owner_user_id: String,
    pub spec: LocalRemoteConnectionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateRemoteConnectionCommand {
    pub owner_user_id: String,
    pub connection_id: String,
    pub expected_version: u64,
    pub spec: LocalRemoteConnectionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteRemoteConnectionCommand {
    pub owner_user_id: String,
    pub connection_id: String,
    pub expected_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRemoteConnection {
    pub connection_id: String,
    pub owner_user_id: String,
    pub name: String,
    pub host: String,
    pub port: u32,
    pub username: String,
    pub authentication_type: LocalRemoteAuthenticationType,
    pub has_password: bool,
    pub has_private_key_path: bool,
    pub has_certificate_path: bool,
    pub default_remote_path: Option<String>,
    pub host_key_policy: LocalRemoteHostKeyPolicy,
    pub local_connector_device_id: String,
    pub local_connector_workspace_id: String,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<u32>,
    pub jump_username: Option<String>,
    pub has_jump_private_key_path: bool,
    pub has_jump_certificate_path: bool,
    pub has_jump_password: bool,
    pub last_active_at_unix_ms: Option<i64>,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

macro_rules! validate_owner {
    ($value:expr) => {
        validate_identifier("owner_user_id", &$value.owner_user_id)
    };
}

impl ListRemoteConnectionsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)
    }
}

impl GetRemoteConnectionCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("connection_id", &self.connection_id)
    }
}

impl CreateRemoteConnectionCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        self.spec.validate()
    }
}

impl UpdateRemoteConnectionCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("connection_id", &self.connection_id)?;
        validate_expected_version(self.expected_version)?;
        self.spec.validate()
    }
}

impl DeleteRemoteConnectionCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("connection_id", &self.connection_id)?;
        validate_expected_version(self.expected_version)
    }
}

fn validate_port(name: &str, value: u32) -> Result<(), String> {
    if !(1..=65_535).contains(&value) {
        return Err(format!("{name} must be between 1 and 65535"));
    }
    Ok(())
}

fn validate_expected_version(value: u64) -> Result<(), String> {
    if value == 0 {
        return Err("expected_version must be greater than zero".to_string());
    }
    Ok(())
}

fn validate_optional_text(name: &str, value: Option<&str>, maximum: usize) -> Result<(), String> {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        validate_text(name, value, maximum)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> LocalRemoteConnectionSpec {
        LocalRemoteConnectionSpec {
            name: Some("Production".to_string()),
            host: "server.example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            authentication_type: LocalRemoteAuthenticationType::PrivateKey,
            default_remote_path: Some("/srv/app".to_string()),
            host_key_policy: LocalRemoteHostKeyPolicy::Strict,
            local_connector_device_id: "local-device".to_string(),
            local_connector_workspace_id: "local-workspace".to_string(),
            jump_enabled: false,
            jump_connection_id: None,
            jump_host: None,
            jump_port: None,
            jump_username: None,
        }
    }

    #[test]
    fn validates_ports_and_complete_manual_jump_routes() {
        let mut value = spec();
        value.port = 0;
        assert!(value.validate().is_err());
        value.port = 22;
        value.jump_enabled = true;
        assert!(value.validate().is_err());
        value.jump_connection_id = Some("jump-1".to_string());
        assert!(value.validate().is_ok());
    }
}
